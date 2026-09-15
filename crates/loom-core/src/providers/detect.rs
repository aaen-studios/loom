//! Model discovery: `GET /models` (or Anthropic's `/v1/models`) after the
//! user enters a key + base URL, merged over the bundled catalog.

use serde_json::Value;

use super::{anthropic, openai};
use crate::catalog;
use crate::provider::{ModelSpec, ProviderConfig, ProviderKind};
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

/// Extracts `data[].id` from an OpenAI/Anthropic-shaped models response.
/// Ollama exposes `models[].name`, which is handled too.
pub fn parse_models(value: &Value) -> Vec<(String, ModelSpec)> {
    let mut ids: Vec<String> = Vec::new();

    for key in ["data", "models"] {
        if let Some(items) = value.get(key).and_then(Value::as_array) {
            for item in items {
                let id = item
                    .get("id")
                    .or_else(|| item.get("name"))
                    .and_then(Value::as_str);
                if let Some(id) = id {
                    ids.push(id.to_string());
                }
            }
        }
    }

    ids.sort();
    ids.dedup();

    ids.into_iter()
        .map(|id| {
            let spec = catalog::lookup(&id).unwrap_or_else(catalog::fallback);
            (id, spec)
        })
        .collect()
}

/// Merges freshly fetched models into an existing map, preserving the user's
/// metadata edits and never deleting hand-added entries.
pub fn merge_models(
    existing: &std::collections::BTreeMap<String, ModelSpec>,
    fetched: Vec<(String, ModelSpec)>,
) -> std::collections::BTreeMap<String, ModelSpec> {
    let mut merged = existing.clone();
    for (id, spec) in fetched {
        match merged.get_mut(&id) {
            // Keep what the user already tuned for this model.
            Some(current) => {
                if current.input_modalities.is_empty() {
                    current.input_modalities = spec.input_modalities;
                }
                if current.context.is_none() {
                    current.context = spec.context;
                }
                if current.output.is_none() {
                    current.output = spec.output;
                }
                if current.reasoning.is_none() {
                    current.reasoning = spec.reasoning;
                }
            }
            None => {
                merged.insert(id, spec);
            }
        }
    }
    merged
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
        let mut existing = std::collections::BTreeMap::new();
        existing.insert(
            "gpt-4o".to_string(),
            ModelSpec {
                context: Some(99),
                output: None,
                input_modalities: vec![],
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
        assert_eq!(models_endpoint(&provider), "https://api.openai.com/v1/models");
    }
}
