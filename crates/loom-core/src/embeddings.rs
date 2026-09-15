//! Embeddings via an OpenAI-compatible `/embeddings` endpoint.
//!
//! Used by the workspace index. Kept dependency-free like the rest of the
//! provider layer: request building and response parsing are pure functions.

use serde_json::{json, Value};

use crate::provider::ProviderConfig;
use crate::{Error, Result};

/// Vectors are stored as little-endian f32 blobs in SQLite.
pub fn encode(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

pub fn decode(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

pub fn endpoint(provider: &ProviderConfig) -> String {
    format!("{}/embeddings", provider.normalized_base_url())
}

pub fn build_body(model: &str, inputs: &[String]) -> Value {
    json!({ "model": model, "input": inputs })
}

/// Extracts `data[].embedding` from an OpenAI-shaped response.
pub fn parse_response(value: &Value, status: u16) -> Result<Vec<Vec<f32>>> {
    if !(200..300).contains(&status) {
        let message = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("embedding request failed");
        return Err(Error::Provider(message.to_string()));
    }

    let data = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Provider("response contained no embeddings".into()))?;

    let mut vectors = Vec::with_capacity(data.len());
    for item in data {
        let embedding = item
            .get("embedding")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::Provider("embedding item was malformed".into()))?;
        vectors.push(
            embedding
                .iter()
                .map(|value| value.as_f64().unwrap_or(0.0) as f32)
                .collect::<Vec<f32>>(),
        );
    }

    if vectors.is_empty() {
        return Err(Error::Provider("provider returned zero embeddings".into()));
    }
    Ok(vectors)
}

/// Calls the endpoint, batching inputs so providers with small limits survive.
pub async fn embed(
    client: &reqwest::Client,
    provider: &ProviderConfig,
    api_key: Option<&str>,
    model: &str,
    inputs: &[String],
) -> Result<Vec<Vec<f32>>> {
    const BATCH: usize = 32;

    let mut all = Vec::with_capacity(inputs.len());
    for batch in inputs.chunks(BATCH) {
        let mut request = client
            .post(endpoint(provider))
            .json(&build_body(model, batch));
        if let Some(key) = api_key.filter(|key| !key.trim().is_empty()) {
            request = request.header("authorization", format!("Bearer {key}"));
        }
        for (name, value) in &provider.headers {
            request = request.header(name, value);
        }

        let response = request
            .send()
            .await
            .map_err(|e| Error::Http(format!("embedding request failed: {e}")))?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .await
            .map_err(|e| Error::Http(e.to_string()))?;
        let value: Value = serde_json::from_str(&body)
            .map_err(|e| Error::Provider(format!("unexpected embedding response: {e}")))?;

        let mut vectors = parse_response(&value, status)?;
        if vectors.len() != batch.len() {
            return Err(Error::Provider(format!(
                "expected {} embeddings, got {}",
                batch.len(),
                vectors.len()
            )));
        }
        all.append(&mut vectors);
    }

    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vectors_round_trip_through_bytes() {
        let vector = vec![1.0f32, -0.5, 0.25];
        let bytes = encode(&vector);
        assert_eq!(bytes.len(), 12);
        assert_eq!(decode(&bytes), vector);
    }

    #[test]
    fn parses_openai_shaped_responses() {
        let value = serde_json::json!({
            "data": [
                { "embedding": [0.1, 0.2] },
                { "embedding": [-1.0, 2.0] }
            ]
        });
        let vectors = parse_response(&value, 200).unwrap();
        assert_eq!(vectors.len(), 2);
        assert_eq!(vectors[1], vec![-1.0, 2.0]);
    }

    #[test]
    fn surfaces_provider_errors() {
        let value = serde_json::json!({ "error": { "message": "quota" } });
        assert!(parse_response(&value, 429)
            .unwrap_err()
            .to_string()
            .contains("quota"));
        assert!(parse_response(&serde_json::json!({ "data": [] }), 200).is_err());
    }

    #[test]
    fn endpoint_is_under_the_api_root() {
        let provider = ProviderConfig {
            base_url: "https://api.openai.com".into(),
            ..Default::default()
        };
        assert_eq!(endpoint(&provider), "https://api.openai.com/v1/embeddings");
    }
}
