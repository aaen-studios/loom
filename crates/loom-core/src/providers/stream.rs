//! Streaming transport: runs one chat completion and reports normalised
//! deltas through a callback until completion or cancellation.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;

use super::sse::{data_field, LineBuffer};
use super::{anthropic, openai, ChatRequest, Delta, Usage};
use crate::provider::{ProviderConfig, ProviderKind};
use crate::{Error, Result};

pub type Cancellation = Arc<AtomicBool>;

pub fn cancellation() -> Cancellation {
    Arc::new(AtomicBool::new(false))
}

/// Runs a streamed completion. `on_delta` fires for every text/reasoning chunk
/// and (when the provider reports it) usage.
pub async fn run_stream(
    client: &reqwest::Client,
    request: &ChatRequest<'_>,
    api_key: Option<&str>,
    cancel: &Cancellation,
    mut on_delta: impl FnMut(Delta),
) -> Result<Usage> {
    let provider = request.provider;
    let url = match provider.kind {
        ProviderKind::Anthropic => anthropic::endpoint(provider),
        ProviderKind::OpenaiCompatible => openai::endpoint(provider),
    };
    let body = match provider.kind {
        ProviderKind::Anthropic => anthropic::build_body(request),
        ProviderKind::OpenaiCompatible => openai::build_body(request),
    };

    let mut builder = client.post(&url).json(&body);
    for (name, value) in request.headers(api_key) {
        builder = builder.header(name, value);
    }

    let response = builder
        .send()
        .await
        .map_err(|e| Error::Http(format!("{}: {e}", provider.name)))?;

    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        let body = response.text().await.unwrap_or_default();
        let value: serde_json::Value =
            serde_json::from_str(&body).unwrap_or(serde_json::Value::Null);
        let message = match provider.kind {
            ProviderKind::Anthropic => anthropic::error_message(&value, status),
            ProviderKind::OpenaiCompatible => openai::error_message(&value, status),
        };
        return Err(Error::Provider(message));
    }

    let mut usage = Usage::default();
    let mut buffer = LineBuffer::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let chunk = chunk.map_err(|e| Error::Http(e.to_string()))?;
        let text = String::from_utf8_lossy(&chunk);

        for line in buffer.push(&text) {
            let Some(data) = data_field(&line) else {
                continue;
            };
            if data.trim().is_empty() {
                continue;
            }
            let deltas = match provider.kind {
                ProviderKind::Anthropic => anthropic::parse_chunk(data)?,
                ProviderKind::OpenaiCompatible => openai::parse_chunk(data)?,
            };
            for delta in deltas.into_iter().flatten() {
                match &delta {
                    Delta::Usage {
                        input_tokens,
                        output_tokens,
                    } => {
                        if input_tokens.is_some() {
                            usage.input_tokens = *input_tokens;
                        }
                        if output_tokens.is_some() {
                            usage.output_tokens = *output_tokens;
                        }
                    }
                    _ => {}
                }
                on_delta(delta);
            }
        }
    }

    Ok(usage)
}

/// Non-streaming completion, used for titles and other small background jobs.
pub async fn run_once(
    client: &reqwest::Client,
    request: &ChatRequest<'_>,
    api_key: Option<&str>,
) -> Result<(String, Option<String>, Usage)> {
    let provider: &ProviderConfig = request.provider;
    let url = match provider.kind {
        ProviderKind::Anthropic => anthropic::endpoint(provider),
        ProviderKind::OpenaiCompatible => openai::endpoint(provider),
    };
    let body = match provider.kind {
        ProviderKind::Anthropic => anthropic::build_body(request),
        ProviderKind::OpenaiCompatible => openai::build_body(request),
    };

    let mut builder = client.post(&url).json(&body);
    for (name, value) in request.headers(api_key) {
        builder = builder.header(name, value);
    }

    let response = builder
        .send()
        .await
        .map_err(|e| Error::Http(format!("{}: {e}", provider.name)))?;
    let status = response.status().as_u16();
    let text = response
        .text()
        .await
        .map_err(|e| Error::Http(e.to_string()))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| Error::Provider(format!("unexpected response: {e}")))?;

    match provider.kind {
        ProviderKind::Anthropic => anthropic::parse_response(&value, status),
        ProviderKind::OpenaiCompatible => openai::parse_response(&value, status),
    }
}
