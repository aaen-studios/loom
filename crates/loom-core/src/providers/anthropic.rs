//! Anthropic Messages dialect.

use serde_json::{json, Value};

use super::{ChatRequest, Delta, Usage};
use crate::{Error, Result};

pub fn endpoint(provider: &crate::provider::ProviderConfig) -> String {
    format!("{}/v1/messages", provider.normalized_base_url())
}

pub fn models_endpoint(provider: &crate::provider::ProviderConfig) -> String {
    format!("{}/v1/models?limit=1000", provider.normalized_base_url())
}

/// Thinking budget per variant, in tokens.
///
/// Public because the engine computes the declared `max_tokens` from it: the
/// budget lives inside `max_tokens`, so the number reserved for the reply and
/// the number sent on the wire have to account for it together. The adapter
/// used to inflate `max_tokens` on its own, which is exactly the kind of
/// behind-the-budget's-back arithmetic that let a request go out 34k tokens
/// larger than Loom had reserved for it.
pub fn thinking_budget(variant: &str) -> Option<u32> {
    match variant {
        "low" => Some(2_048),
        "medium" => Some(8_192),
        "high" => Some(24_000),
        _ => None,
    }
}

pub fn build_body(request: &ChatRequest<'_>) -> Value {
    let messages: Vec<Value> = request
        .messages
        .iter()
        .map(|message| {
            // Tool results travel as a user turn with tool_result blocks.
            // Screenshots ride along as image blocks inside the same result.
            if message.role == "tool" {
                let mut blocks: Vec<Value> = Vec::new();
                let text = message.joined_text();
                if !text.is_empty() {
                    blocks.push(json!({ "type": "text", "text": text }));
                }
                for part in &message.parts {
                    if let super::ContentPart::Image { mime, base64, .. } = part {
                        blocks.push(json!({
                            "type": "image",
                            "source": { "type": "base64", "media_type": mime, "data": base64 }
                        }));
                    }
                }
                if blocks.is_empty() {
                    blocks.push(json!({ "type": "text", "text": "" }));
                }
                return json!({
                    "role": "user",
                    "content": [{
                        "type": "tool_result",
                        "tool_use_id": message.tool_call_id.clone().unwrap_or_default(),
                        "content": blocks,
                    }]
                });
            }

            if message.is_text_only() && message.tool_calls.is_empty() {
                return json!({ "role": message.role, "content": message.joined_text() });
            }

            let mut blocks: Vec<Value> = Vec::new();
            for part in &message.parts {
                match part {
                    super::ContentPart::Text { text } => {
                        if !text.is_empty() {
                            blocks.push(json!({ "type": "text", "text": text }));
                        }
                    }
                    super::ContentPart::Image { mime, base64, .. } => blocks.push(json!({
                        "type": "image",
                        "source": { "type": "base64", "media_type": mime, "data": base64 }
                    })),
                }
            }
            for call in &message.tool_calls {
                let input: Value = serde_json::from_str(&call.arguments)
                    .unwrap_or_else(|_| json!({ "raw": call.arguments }));
                blocks.push(json!({
                    "type": "tool_use",
                    "id": call.id,
                    "name": call.name,
                    "input": input,
                }));
            }

            json!({ "role": message.role, "content": blocks })
        })
        .collect();

    // Anthropic requires `max_tokens`, and the engine always supplies it. The
    // fallback only covers a request built outside a turn (a test, a probe).
    let max_tokens = request
        .max_output_tokens
        .unwrap_or(crate::context::DEFAULT_OUTPUT_TOKENS);
    let mut body = json!({
        "model": request.model,
        "max_tokens": max_tokens,
        "messages": messages,
        "stream": request.stream,
    });

    if let Some(system) = request.system.filter(|s| !s.trim().is_empty()) {
        body["system"] = json!(system);
    }

    if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }
    if let Some(top_p) = request.top_p {
        body["top_p"] = json!(top_p);
    }

    if let Some(variant) = request.variant {
        // Thinking lives *inside* `max_tokens`, and the API requires
        // max_tokens to exceed the thinking budget. The engine already added
        // room for it when it chose the number it sent, so this only trims the
        // budget for a model whose window cannot hold the configured effort —
        // it never inflates the declared number behind the budget's back.
        if let Some(requested) = thinking_budget(variant) {
            let budget = requested.min(max_tokens.saturating_sub(1_024));
            if budget >= 1_024 {
                body["thinking"] = json!({ "type": "enabled", "budget_tokens": budget });
            }
        }
    }

    if !request.tools.is_empty() {
        body["tools"] = json!(request
            .tools
            .iter()
            .map(|tool| json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.parameters,
            }))
            .collect::<Vec<_>>());
    }

    body
}

/// Parses one `data:` payload of the Messages stream.
pub fn parse_chunk(data: &str) -> Result<Option<Vec<Delta>>> {
    let value: Value = serde_json::from_str(data)
        .map_err(|e| Error::Provider(format!("malformed stream chunk: {e}")))?;

    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut deltas = Vec::new();

    match event_type {
        "error" => {
            let message = value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("unknown provider error");
            return Err(Error::Provider(message.to_string()));
        }
        "message_start" => {
            if let Some(usage) = value.pointer("/message/usage") {
                deltas.push(Delta::Usage {
                    input_tokens: usage
                        .get("input_tokens")
                        .and_then(Value::as_u64)
                        .map(|v| v as u32),
                    output_tokens: usage
                        .get("output_tokens")
                        .and_then(Value::as_u64)
                        .map(|v| v as u32),
                });
            }
        }
        "content_block_start" => {
            if let Some(block) = value.get("content_block") {
                if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                    let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                    deltas.push(Delta::ToolCall {
                        index,
                        id: block.get("id").and_then(Value::as_str).map(str::to_string),
                        name: block
                            .get("name")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        argument_fragment: None,
                    });
                }
            }
        }
        "content_block_delta" => {
            let delta = value.get("delta").cloned().unwrap_or(Value::Null);
            let index = value.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
            match delta.get("type").and_then(Value::as_str) {
                Some("text_delta") => {
                    if let Some(text) = delta.get("text").and_then(Value::as_str) {
                        deltas.push(Delta::Text {
                            text: text.to_string(),
                        });
                    }
                }
                Some("thinking_delta") => {
                    if let Some(text) = delta.get("thinking").and_then(Value::as_str) {
                        deltas.push(Delta::Reasoning {
                            text: text.to_string(),
                        });
                    }
                }
                Some("input_json_delta") => {
                    if let Some(fragment) = delta.get("partial_json").and_then(Value::as_str) {
                        deltas.push(Delta::ToolCall {
                            index,
                            id: None,
                            name: None,
                            argument_fragment: Some(fragment.to_string()),
                        });
                    }
                }
                _ => {}
            }
        }
        "message_delta" => {
            if let Some(usage) = value.get("usage") {
                deltas.push(Delta::Usage {
                    input_tokens: usage
                        .get("input_tokens")
                        .and_then(Value::as_u64)
                        .map(|v| v as u32),
                    output_tokens: usage
                        .get("output_tokens")
                        .and_then(Value::as_u64)
                        .map(|v| v as u32),
                });
            }
        }
        _ => {}
    }

    Ok(if deltas.is_empty() {
        None
    } else {
        Some(deltas)
    })
}

pub fn parse_response(value: &Value, status: u16) -> Result<(String, Option<String>, Usage)> {
    if !(200..300).contains(&status) {
        return Err(Error::Provider(error_message(value, status)));
    }

    let mut content = String::new();
    let mut reasoning = String::new();
    for block in value
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    content.push_str(text);
                }
            }
            Some("thinking") => {
                if let Some(text) = block.get("thinking").and_then(Value::as_str) {
                    reasoning.push_str(text);
                }
            }
            _ => {}
        }
    }

    let usage = value
        .get("usage")
        .map(|usage| Usage {
            input_tokens: usage
                .get("input_tokens")
                .and_then(Value::as_u64)
                .map(|v| v as u32),
            output_tokens: usage
                .get("output_tokens")
                .and_then(Value::as_u64)
                .map(|v| v as u32),
        })
        .unwrap_or_default();

    Ok((
        content,
        if reasoning.is_empty() {
            None
        } else {
            Some(reasoning)
        },
        usage,
    ))
}

pub fn error_message(value: &Value, status: u16) -> String {
    value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("HTTP {status}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ProviderConfig, ProviderKind};

    fn request(provider: &ProviderConfig) -> ChatRequest<'_> {
        ChatRequest {
            provider,
            model: "claude-sonnet-4",
            system: Some("Be brief."),
            messages: vec![super::super::WireMessage::text("user", "hi")],
            variant: None,
            max_output_tokens: None,
            temperature: None,
            top_p: None,
            stream: true,
            tools: Vec::new(),
            session_id: None,
        }
    }

    #[test]
    fn body_uses_messages_dialect() {
        let provider = ProviderConfig {
            kind: ProviderKind::Anthropic,
            ..Default::default()
        };
        let body = build_body(&request(&provider));
        assert_eq!(body["system"], "Be brief.");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert_eq!(body["stream"], true);
        assert!(body.get("thinking").is_none());
    }

    #[test]
    fn high_variant_enables_thinking_inside_the_declared_budget() {
        let provider = ProviderConfig {
            kind: ProviderKind::Anthropic,
            ..Default::default()
        };
        let mut req = request(&provider);
        req.variant = Some("high");
        req.max_output_tokens = Some(32_000);
        let body = build_body(&req);
        // max_tokens is what the engine reserved, unchanged: the thinking
        // budget fits inside it rather than inflating it.
        assert_eq!(body["max_tokens"], 32_000);
        assert_eq!(body["thinking"]["budget_tokens"], 24_000);

        // A model whose window cannot hold the configured effort gets a
        // smaller budget, never a bigger declared max_tokens.
        let mut cramped = request(&provider);
        cramped.variant = Some("high");
        cramped.max_output_tokens = Some(8_192);
        let body = build_body(&cramped);
        assert_eq!(body["max_tokens"], 8_192);
        assert_eq!(body["thinking"]["budget_tokens"], 7_168);
    }

    /// Without a variant there is no thinking block, and the declared budget is
    /// always present so no gateway has to guess one.
    #[test]
    fn max_tokens_is_always_declared() {
        let provider = ProviderConfig {
            kind: ProviderKind::Anthropic,
            ..Default::default()
        };
        let body = build_body(&request(&provider));
        assert_eq!(body["max_tokens"], crate::context::DEFAULT_OUTPUT_TOKENS);
        assert!(body.get("thinking").is_none());
    }

    #[test]
    fn parses_text_thinking_and_usage_events() {
        let text = parse_chunk(
            r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello"}}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            text[0],
            Delta::Text {
                text: "Hello".into()
            }
        );

        let thinking = parse_chunk(
            r#"{"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"hm"}}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(thinking[0], Delta::Reasoning { text: "hm".into() });

        let start = parse_chunk(
            r#"{"type":"message_start","message":{"usage":{"input_tokens":9,"output_tokens":0}}}"#,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            start[0],
            Delta::Usage {
                input_tokens: Some(9),
                output_tokens: Some(0)
            }
        );
    }

    #[test]
    fn surfaces_error_events() {
        let error =
            parse_chunk(r#"{"type":"error","error":{"message":"overloaded"}}"#).unwrap_err();
        assert!(error.to_string().contains("overloaded"));
    }

    #[test]
    fn parses_non_streaming_content_blocks() {
        let value = serde_json::json!({
            "content": [
                { "type": "thinking", "thinking": "why" },
                { "type": "text", "text": "hello" }
            ],
            "usage": { "input_tokens": 4, "output_tokens": 6 }
        });
        let (content, reasoning, usage) = parse_response(&value, 200).unwrap();
        assert_eq!(content, "hello");
        assert_eq!(reasoning.as_deref(), Some("why"));
        assert_eq!(usage.input_tokens, Some(4));
    }

    #[test]
    fn endpoints_are_correct() {
        let provider = ProviderConfig {
            kind: ProviderKind::Anthropic,
            base_url: "https://api.anthropic.com".into(),
            ..Default::default()
        };
        assert_eq!(endpoint(&provider), "https://api.anthropic.com/v1/messages");
        assert_eq!(
            models_endpoint(&provider),
            "https://api.anthropic.com/v1/models?limit=1000"
        );
    }
}
