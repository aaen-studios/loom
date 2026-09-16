//! Model discovery: `GET /models` (or Anthropic's `/v1/models`) after the
//! user enters a key + base URL, merged over the bundled catalog.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{anthropic, openai};
use crate::catalog;
use crate::provider::{MetadataSource, Modality, ModelSpec, ProviderConfig, ProviderKind};
use crate::{Error, Result};

fn models_endpoint(provider: &ProviderConfig) -> String {
    match provider.kind {
        ProviderKind::Anthropic => anthropic::models_endpoint(provider),
        ProviderKind::OpenaiCompatible => format!("{}/models", provider.normalized_base_url()),
    }
}

/// Fetches model ids and enriches each with catalog metadata.
/// Returns `(model_id, spec)` pairs sorted by id.
pub async fn fetch_models(
    client: &reqwest::Client,
    provider: &ProviderConfig,
    api_key: Option<&str>,
) -> Result<Vec<(String, ModelSpec)>> {
    let mut request = client.get(models_endpoint(provider));
    if let Some(key) = api_key.filter(|k| !k.trim().is_empty()) {
        if provider.kind == ProviderKind::Anthropic {
            request = request
                .header("x-api-key", key)
                .header("anthropic-version", "2023-06-01");
        } else {
            request = request.header("authorization", format!("Bearer {key}"));
        }
    }
    for (name, value) in &provider.headers {
        request = request.header(name, value);
    }

    let response = request
        .send()
        .await
        .map_err(|e| Error::Http(format!("{}: {e}", provider.name)))?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| Error::Http(e.to_string()))?;

    if !(200..300).contains(&status) {
        let value: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
        let message = match provider.kind {
            ProviderKind::Anthropic => anthropic::error_message(&value, status),
            ProviderKind::OpenaiCompatible => openai::error_message(&value, status),
        };
        return Err(Error::Provider(message));
    }

    let value: Value = serde_json::from_str(&body)
        .map_err(|e| Error::Provider(format!("unexpected /models response: {e}")))?;

    Ok(parse_models(&value))
}

/// Extracts each model id and any metadata the gateway volunteers.
///
/// Most gateways return bare ids (`{"id": "gpt-5"}`), and Ollama exposes
/// `models[].name`; both work. OpenRouter-style entries also carry a context
/// window, modalities, supported parameters, and per-token prices, which are
/// read here and tagged `api`.
pub fn parse_models(value: &Value) -> Vec<(String, ModelSpec)> {
    let mut items: Vec<(String, &Value)> = Vec::new();

    for key in ["data", "models"] {
        if let Some(entries) = value.get(key).and_then(Value::as_array) {
            for item in entries {
                let id = item
                    .get("id")
                    .or_else(|| item.get("name"))
                    .and_then(Value::as_str);
                if let Some(id) = id {
                    items.push((id.to_string(), item));
                }
            }
        }
    }

    items.sort_by(|left, right| left.0.cmp(&right.0));
    items.dedup_by(|left, right| left.0 == right.0);

    items
        .into_iter()
        .map(|(id, item)| {
            let mut spec = catalog::lookup(&id).unwrap_or_else(catalog::fallback);
            apply_api_metadata(&mut spec, item);
            (id, spec)
        })
        .collect()
}

/// Overlays whatever the gateway says about one model. The result is tagged
/// `api`, which outranks the bundled catalogue but never the user.
fn apply_api_metadata(spec: &mut ModelSpec, item: &Value) {
    let mut found = false;

    let context = item
        .get("context_length")
        .or_else(|| item.pointer("/top_provider/context_length"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    if let Some(context) = context {
        spec.context = Some(context);
        found = true;
    }

    let output = item
        .get("max_completion_tokens")
        .or_else(|| item.pointer("/top_provider/max_completion_tokens"))
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok());
    if let Some(output) = output {
        spec.output = Some(output);
        found = true;
    }

    if let Some(modalities) = item
        .pointer("/architecture/input_modalities")
        .and_then(Value::as_array)
    {
        let mapped: Vec<Modality> = modalities
            .iter()
            .filter_map(Value::as_str)
            .filter_map(Modality::from_wire)
            .collect();
        if !mapped.is_empty() {
            spec.input_modalities = mapped;
            found = true;
        }
    }

    if let Some(parameters) = item.get("supported_parameters").and_then(Value::as_array) {
        let reasons = parameters
            .iter()
            .filter_map(Value::as_str)
            .any(|name| matches!(name, "reasoning" | "include_reasoning" | "reasoning_effort"));
        if reasons && spec.reasoning.is_none() {
            spec.reasoning = Some(catalog::toggle_variants());
            found = true;
        }
    }

    if let Some(pricing) = item.get("pricing") {
        if let Some(input) = price_per_million(pricing, "prompt") {
            spec.input_price = Some(input);
            found = true;
        }
        if let Some(output) = price_per_million(pricing, "completion") {
            spec.output_price = Some(output);
            found = true;
        }
    }

    if found {
        spec.source = MetadataSource::Api;
    }
}

/// OpenRouter publishes per-token prices as strings, e.g. `"0.0000025"`;
/// some gateways use numbers. Both become USD per 1M tokens.
fn price_per_million(pricing: &Value, key: &str) -> Option<f32> {
    let raw = pricing.get(key)?;
    let per_token = raw
        .as_f64()
        .or_else(|| raw.as_str()?.trim().parse::<f64>().ok())?;
    if per_token < 0.0 {
        return None;
    }
    Some((per_token * 1_000_000.0) as f32)
}

/// Merges freshly fetched models into an existing map, preserving the user's
/// metadata edits and never deleting hand-added entries.
pub fn merge_models(
    existing: &BTreeMap<String, ModelSpec>,
    fetched: Vec<(String, ModelSpec)>,
) -> BTreeMap<String, ModelSpec> {
    let mut merged = existing.clone();
    for (id, incoming) in fetched {
        match merged.get_mut(&id) {
            Some(current) => merge_spec(current, incoming),
            None => {
                merged.insert(id, incoming);
            }
        }
    }
    merged
}

/// Merges one detected spec over the stored one.
///
/// A higher `source` wins field by field: an API value corrects a catalogue
/// guess without a partial response wiping fields it did not mention, and a
/// newer catalogue version corrects an older guess. A `user` spec is never
/// touched; unknown fields on either side are ignored, never erased.
pub(crate) fn merge_spec(current: &mut ModelSpec, incoming: ModelSpec) {
    if current.source == MetadataSource::User {
        return;
    }

    let wins = incoming.source.authority() > current.source.authority();
    let upgrade = wins && incoming.has_metadata();
    if incoming.context.is_some() && (wins || current.context.is_none()) {
        current.context = incoming.context;
    }
    if incoming.output.is_some() && (wins || current.output.is_none()) {
        current.output = incoming.output;
    }
    if !incoming.input_modalities.is_empty() && (wins || current.input_modalities.is_empty()) {
        current.input_modalities = incoming.input_modalities;
    }
    if incoming.reasoning.is_some() && (wins || current.reasoning.is_none()) {
        current.reasoning = incoming.reasoning;
    }
    if incoming.input_price.is_some() && (wins || current.input_price.is_none()) {
        current.input_price = incoming.input_price;
    }
    if incoming.output_price.is_some() && (wins || current.output_price.is_none()) {
        current.output_price = incoming.output_price;
    }
    if incoming.name.is_some() && (wins || current.name.is_none()) {
        current.name = incoming.name;
    }

    if upgrade {
        current.source = incoming.source;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_openai_shaped_and_ollama_shaped_responses() {
        let openai = json!({ "data": [ { "id": "gpt-4o" }, { "id": "gpt-5" } ] });
        let parsed = parse_models(&openai);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, "gpt-4o");
        assert_eq!(parsed[1].1.context, Some(400_000));

        let ollama = json!({ "models": [ { "name": "qwen3:8b" } ] });
        assert_eq!(parse_models(&ollama)[0].0, "qwen3:8b");
    }

    #[test]
    fn merge_preserves_user_edits_and_keeps_manual_entries() {
        let mut existing = BTreeMap::new();
        existing.insert(
            "gpt-4o".to_string(),
            ModelSpec {
                context: Some(99),
                output: None,
                input_modalities: vec![],
                source: MetadataSource::User,
                ..Default::default()
            },
        );
        existing.insert("hand-added".to_string(), ModelSpec::default());

        let fetched = parse_models(&json!({ "data": [ { "id": "gpt-4o" }, { "id": "gpt-5" } ] }));
        let merged = merge_models(&existing, fetched);

        assert_eq!(merged["gpt-4o"].context, Some(99), "user edit preserved");
        assert!(merged.contains_key("hand-added"));
        assert!(merged.contains_key("gpt-5"));
    }

    #[test]
    fn a_newer_source_corrects_guesses_but_not_user_edits() {
        let mut existing = BTreeMap::new();
        // A catalog@0 guess with a stale modality list: the current catalog
        // has more to say, so it wins.
        existing.insert(
            "deepseek-v4-pro".to_string(),
            ModelSpec {
                context: Some(128_000),
                output: Some(8_192),
                input_modalities: vec![Modality::Text],
                source: MetadataSource::Catalog(0),
                ..Default::default()
            },
        );
        // A hand edit: nothing may touch it.
        existing.insert(
            "deepseek-v4-flash".to_string(),
            ModelSpec {
                context: Some(42),
                source: MetadataSource::User,
                ..Default::default()
            },
        );

        let fetched = parse_models(&json!({
            "data": [ { "id": "deepseek-v4-pro" }, { "id": "deepseek-v4-flash" } ]
        }));
        let merged = merge_models(&existing, fetched);

        let pro = &merged["deepseek-v4-pro"];
        assert!(pro.reasoning.is_some(), "catalog@N beats catalog@0");
        assert_eq!(
            pro.source,
            MetadataSource::Catalog(catalog::CATALOG_VERSION)
        );

        let flash = &merged["deepseek-v4-flash"];
        assert_eq!(flash.context, Some(42));
        assert_eq!(flash.source, MetadataSource::User);
        assert!(flash.input_modalities.is_empty(), "user spec untouched");
    }

    #[test]
    fn openrouter_style_rows_supply_api_metadata() {
        let parsed = parse_models(&json!({
            "data": [{
                "id": "some/model",
                "context_length": 131072,
                "top_provider": { "context_length": 65536 },
                "max_completion_tokens": 8192,
                "architecture": { "input_modalities": ["text", "image", "file"] },
                "supported_parameters": ["tools", "reasoning"],
                "pricing": { "prompt": "0.0000015", "completion": "0.000006" }
            }]
        }));
        let spec = &parsed[0].1;
        assert_eq!(spec.context, Some(131_072), "top-level window wins");
        assert_eq!(spec.output, Some(8_192));
        assert!(spec.input_modalities.contains(&Modality::Image));
        assert!(spec.input_modalities.contains(&Modality::Pdf));
        assert!(spec.reasoning.is_some());
        assert!((spec.input_price.unwrap() - 1.5).abs() < f32::EPSILON);
        assert!((spec.output_price.unwrap() - 6.0).abs() < f32::EPSILON);
        assert_eq!(spec.source, MetadataSource::Api);
    }

    #[test]
    fn bare_ids_keep_the_catalog_provenance() {
        let parsed = parse_models(&json!({ "data": [ { "id": "gpt-4o" } ] }));
        assert_eq!(
            parsed[0].1.source,
            MetadataSource::Catalog(catalog::CATALOG_VERSION)
        );

        let unknown = parse_models(&json!({ "data": [ { "id": "totally-custom-9000" } ] }));
        assert_eq!(unknown[0].1.source, MetadataSource::Unknown);
        assert!(unknown[0].1.input_modalities.is_empty());
    }

    #[test]
    fn anthropic_endpoint_differs() {
        let provider = ProviderConfig {
            kind: ProviderKind::Anthropic,
            base_url: "https://api.anthropic.com".into(),
            ..Default::default()
        };
        assert_eq!(
            models_endpoint(&provider),
            "https://api.anthropic.com/v1/models?limit=1000"
        );

        let provider = ProviderConfig {
            base_url: "https://api.openai.com".into(),
            ..Default::default()
        };
        assert_eq!(
            models_endpoint(&provider),
            "https://api.openai.com/v1/models"
        );
    }
}
