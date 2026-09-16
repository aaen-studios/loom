//! External model metadata, models.dev-shaped.
//!
//! The bundled [`crate::catalog`] is the offline fallback; this module adds
//! detail it does not carry (prices, windows for families it has never heard
//! of) when the network and a fresh on-disk cache allow. Everything it
//! produces is tagged `api`, so it can correct a catalogue guess but never a
//! gateway value or a user edit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::fsutil::atomic_write;
use crate::provider::{MetadataSource, Modality, ModelSpec};
use crate::{paths, Error, Result};

/// models.dev publishes the whole dataset as one JSON document.
const SOURCE_URL: &str = "https://models.dev/api.json";

/// How long a cached table stays fresh. The dataset moves slowly, and a price
/// that is a week old is still a better estimate than none.
const MAX_AGE_MS: i64 = 7 * 24 * 60 * 60 * 1_000;

/// Network budget: metadata is a nicety, a refresh must not hang on it.
const FETCH_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cache {
    fetched_at: i64,
    models: BTreeMap<String, ModelSpec>,
}

fn cache_path() -> Result<PathBuf> {
    Ok(paths::cache_dir()?.join("models-dev.json"))
}

/// A model id -> metadata index, from the on-disk cache when fresh and the
/// network otherwise. `None` when neither is available; a refresh then
/// proceeds with the gateway and the bundled catalog alone.
pub async fn model_index(client: &reqwest::Client) -> Option<BTreeMap<String, ModelSpec>> {
    let path = cache_path().ok()?;
    let cached = read_cache(&path);

    if let Some(cache) = &cached {
        if crate::db::now_ms() - cache.fetched_at < MAX_AGE_MS {
            return Some(cache.models.clone());
        }
    }

    match fetch(client).await {
        Ok(models) => {
            write_cache(&path, &models);
            Some(models)
        }
        Err(error) => {
            eprintln!("[loom] external model metadata unavailable: {error}");
            cached.map(|cache| cache.models)
        }
    }
}

async fn fetch(client: &reqwest::Client) -> Result<BTreeMap<String, ModelSpec>> {
    let response = client
        .get(SOURCE_URL)
        .timeout(FETCH_TIMEOUT)
        .send()
        .await
        .map_err(|e| Error::Http(format!("models.dev: {e}")))?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| Error::Http(e.to_string()))?;
    if !(200..300).contains(&status) {
        return Err(Error::Http(format!("models.dev returned {status}")));
    }
    let value: Value = serde_json::from_str(&body)
        .map_err(|e| Error::Provider(format!("unexpected models.dev dataset: {e}")))?;
    Ok(parse_index(&value))
}

fn read_cache(path: &Path) -> Option<Cache> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_cache(path: &Path, models: &BTreeMap<String, ModelSpec>) {
    let cache = Cache {
        fetched_at: crate::db::now_ms(),
        models: models.clone(),
    };
    let Ok(mut json) = serde_json::to_string(&cache) else {
        return;
    };
    json.push('\n');
    if let Err(error) = atomic_write(path, json.as_bytes()) {
        eprintln!("[loom] could not cache models.dev metadata: {error}");
    }
}

/// Flattens `{provider: {models: {id: {...}}}}` into `id -> spec`.
///
/// The same id can appear under several providers with different prices; the
/// first wins, which only matters for estimates.
pub fn parse_index(value: &Value) -> BTreeMap<String, ModelSpec> {
    let mut index = BTreeMap::new();
    let Some(providers) = value.as_object() else {
        return index;
    };

    for provider in providers.values() {
        let Some(models) = provider.get("models").and_then(Value::as_object) else {
            continue;
        };
        for (id, model) in models {
            if index.contains_key(id) {
                continue;
            }
            if let Some(spec) = parse_model(model) {
                index.insert(id.clone(), spec);
            }
        }
    }
    index
}

/// One models.dev row. Prices are already USD per 1M tokens.
fn parse_model(value: &Value) -> Option<ModelSpec> {
    let input_modalities: Vec<Modality> = value
        .pointer("/modalities/input")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .filter_map(Modality::from_wire)
                .collect()
        })
        .unwrap_or_default();

    let spec = ModelSpec {
        context: u32_field(value.pointer("/limit/context")),
        output: u32_field(value.pointer("/limit/output")),
        input_modalities,
        reasoning: value
            .get("reasoning")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            .then(crate::catalog::toggle_variants),
        input_price: price(value.pointer("/cost/input")),
        output_price: price(value.pointer("/cost/output")),
        source: MetadataSource::Api,
        ..Default::default()
    };

    spec.has_metadata().then_some(spec)
}

fn u32_field(value: Option<&Value>) -> Option<u32> {
    value
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn price(value: Option<&Value>) -> Option<f32> {
    let price = value?.as_f64()?;
    if price < 0.0 {
        return None;
    }
    Some(price as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flattens_providers_into_an_id_index() {
        let index = parse_index(&json!({
            "anthropic": {
                "models": {
                    "claude-sonnet-4": {
                        "limit": { "context": 200000, "output": 64000 },
                        "modalities": { "input": ["text", "image", "pdf"] },
                        "reasoning": true,
                        "cost": { "input": 3, "output": 15 }
                    }
                }
            },
            "empty-provider": { "models": {} }
        }));

        let spec = &index["claude-sonnet-4"];
        assert_eq!(spec.context, Some(200_000));
        assert_eq!(spec.output, Some(64_000));
        assert_eq!(spec.input_modalities.len(), 3);
        assert!(spec.reasoning.is_some());
        assert_eq!(spec.input_price, Some(3.0));
        assert_eq!(spec.output_price, Some(15.0));
        assert_eq!(spec.source, MetadataSource::Api);
    }

    #[test]
    fn rows_without_metadata_are_skipped_and_first_provider_wins() {
        let index = parse_index(&json!({
            "a": { "models": { "mystery": { "name": "Mystery" } } },
            "b": {
                "models": {
                    "same": { "limit": { "context": 1000 } },
                    "other": { "limit": { "output": 500 } }
                }
            },
            "c": { "models": { "same": { "cost": { "input": 1.0, "output": 2.0 } } } }
        }));

        assert!(!index.contains_key("mystery"));
        // The first provider's row wins; the later price-only row is ignored.
        assert!(index["same"].input_price.is_none());
        assert_eq!(index["same"].context, Some(1_000));
        assert_eq!(index["other"].output, Some(500));
    }

    #[test]
    fn cache_round_trips() {
        let _guard = crate::paths::env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("LOOM_HOME", dir.path());

        let mut models = BTreeMap::new();
        models.insert(
            "gpt-4o".to_string(),
            ModelSpec {
                context: Some(128_000),
                source: MetadataSource::Api,
                ..Default::default()
            },
        );
        let path = cache_path().unwrap();
        write_cache(&path, &models);

        let cache = read_cache(&path).unwrap();
        assert_eq!(cache.models["gpt-4o"].context, Some(128_000));
        assert!(crate::db::now_ms() - cache.fetched_at < 5_000);
    }
}
