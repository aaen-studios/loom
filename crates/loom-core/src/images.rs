//! Image generation via an OpenAI-compatible `/images/generations` endpoint.
//!
//! Generated PNGs are written to `~/.loom/generated/` so they can be shown in
//! chat and reused like any other attachment.

use std::path::PathBuf;

use base64::Engine;
use serde_json::json;

use crate::{paths, Error, Result};

/// Saves a base64 PNG payload and returns its path.
pub fn save_png(encoded: &str) -> Result<PathBuf> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .map_err(|e| Error::other(format!("invalid image payload: {e}")))?;
    save_bytes(&bytes)
}

pub fn save_bytes(bytes: &[u8]) -> Result<PathBuf> {
    if bytes.is_empty() {
        return Err(Error::other("provider returned an empty image"));
    }
    let directory = paths::loom_home()?.join("generated");
    std::fs::create_dir_all(&directory).map_err(|e| Error::io(&directory, e))?;
    let path = directory.join(format!("{}.png", uuid::Uuid::new_v4()));
    std::fs::write(&path, bytes).map_err(|e| Error::io(&path, e))?;
    Ok(path)
}

/// Builds the request body for an OpenAI-compatible image generation call.
pub fn build_body(model: &str, prompt: &str, size: Option<&str>) -> serde_json::Value {
    let mut body = json!({
        "model": model,
        "prompt": prompt,
        "n": 1,
        "response_format": "b64_json",
    });
    if let Some(size) = size.filter(|value| !value.trim().is_empty()) {
        body["size"] = json!(size);
    }
    body
}

/// Pulls the first image out of an OpenAI-shaped response.
pub fn parse_response(value: &serde_json::Value, status: u16) -> Result<PathBuf> {
    if !(200..300).contains(&status) {
        let message = value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("image generation failed");
        return Err(Error::Provider(message.to_string()));
    }

    if let Some(encoded) = value
        .pointer("/data/0/b64_json")
        .and_then(serde_json::Value::as_str)
    {
        return save_png(encoded);
    }

    // Some providers return a URL instead of base64.
    if let Some(url) = value
        .pointer("/data/0/url")
        .and_then(serde_json::Value::as_str)
    {
        return Err(Error::Other(format!(
            "provider returned a URL instead of image data: {url}"
        )));
    }

    Err(Error::Provider("response contained no image".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_defaults_to_b64_json() {
        let body = build_body("gpt-image-1", "a cat", None);
        assert_eq!(body["response_format"], "b64_json");
        assert_eq!(body["n"], 1);
        assert!(body.get("size").is_none());
    }

    #[test]
    fn body_passes_explicit_size() {
        let body = build_body("gpt-image-1", "a cat", Some("1024x1024"));
        assert_eq!(body["size"], "1024x1024");
    }

    #[test]
    fn errors_surface_cleanly() {
        let value = serde_json::json!({ "error": { "message": "nope" } });
        let error = parse_response(&value, 400).unwrap_err();
        assert!(error.to_string().contains("nope"));
    }

    #[test]
    fn url_responses_are_reported_clearly() {
        let value = serde_json::json!({ "data": [{ "url": "https://example.com/x.png" }] });
        let error = parse_response(&value, 200).unwrap_err();
        assert!(error.to_string().contains("URL"));
    }
}
