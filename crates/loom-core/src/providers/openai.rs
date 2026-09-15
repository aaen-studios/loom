//! OpenAI chat-completions dialect (also spoken by OpenRouter, DeepSeek,
//! Groq, xAI, Z.ai, Ollama, LM Studio, â€¦).

use serde_json::{json, Value};

use super::{ChatRequest, Delta, Usage};
use crate::{Error, Result};

pub fn endpoint(provider: &crate::provider::ProviderConfig) -> String {
    format!("{}/chat/completions", provider.normalized_base_url())
}

pub fn build_body(request: &ChatRequest<'_>) -> Value {
    let mut messages: Vec<Value> = Vec::new();
    if let Some(system) = request.system.filter(|s| !s.trim().is_empty()) {
        messages.push(json!({ "role": "system", "content": system }));
    }
    for message in &request.messages {
        messages.push(message_json(message));
    }

    let mut body = json!({
        "model": request.model,
        "messages": messages,
        "stream": request.stream,
    });

    if request.stream {
        body["stream_options"] = json!({ "include_usage": true });
    }
    if let Some(max) = request.max_output_tokens {
        body["max_tokens"] = json!(max);
    }
    if let Some(variant) = request.variant.filter(|v| !v.is_empty() && *v != "off") {
        // Understood by OpenAI o-series and several compatible vendors; only
        // sent when the user explicitly picked a variant.
        body["reasoning_effort"] = json!(variant);
    }

    if !request.tools.is_empty() {
        body["tools"] = json!(request
            .tools
            .iter()
            .map(|tool| json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.parameters,
                }
            }))
            .collect::<Vec<_>>());
        body["tool_choice"] = json!("auto");
    }

    body
}

/// Plain string content when there are no images (maximum compatibility with
/// older and local models), otherwise the multimodal content array.
fn message_json(message: &super::WireMessage) -> Value {
    // Tool results use a dedicated shape in this dialect.
    if message.role == "tool" {
        return json!({
            "role": "tool",
            "tool_call_id": message.tool_call_id.clone().unwrap_or_default(),
            "content": message.joined_text(),
        });
    }

    let mut object = if message.is_text_only() {
        json!({ "role": message.role, "content": message.joined_text() })
    } else {
        let parts: Vec<Value> = message
            .parts
            .iter()
            .map(|part| match part {
                super::ContentPart::Text { text } => json!({ "type": "text", "text": text }),
                super::ContentPart::Image { mime, base64, .. } => json!({
                    "type": "image_url",
                    "image_url": { "url": format!("data:{mime};base64,{base64}") }
                }),
            })
            .collect();
        json!({ "role": message.role, "content": parts })
    };

    if !message.tool_calls.is_empty() {
        object["tool_calls"] = json!(message
            .tool_calls
            .iter()
            .map(|call| json!({
                "id": call.id,
                "type": "function",
                "function": { "name": call.name, "arguments": call.arguments }
            }))
            .collect::<Vec<_>>());
        if message.joined_text().is_empty() {
            object["content"] = Value::Null;
        }
    }

    object
}

/// Parses one `data:` payload of a streaming response. `Ok(None)` means the
/// chunk carried nothing for us (role preamble, empty delta, `[DONE]`).
pub fn parse_chunk(data: &str) -> Result<Option<Vec<Delta>>> {
    if data.trim() == "[DONE]" {
        return Ok(None);
    }
    let value: Value = serde_json::from_str(data)
        .map_err(|e| Error::Provider(format!("malformed stream chunk: {e}")))?;

    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown provider error");
        return Err(Error::Provider(message.to_string()));
    }

    let mut deltas = Vec::new();

    if let Some(usage) = value.get("usage").filter(|u| !u.is_null()) {
        deltas.push(Delta::Usage {
            input_tokens: usage
                .get("prompt_tokens")
                .and_then(Value::as_u64)
                .map(|v| v as u32),
            output_tokens: usage
                .get("completion_tokens")
                .and_then(Value::as_u64)
                .map(|v| v as u32),
        });
    }

    if let Some(delta) = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("delta"))
    {
        if let Some(text) = delta.get("content").and_then(Value::as_str) {
            if !text.is_empty() {
                deltas.push(Delta::Text { text: text.to_string() });
            }
        }
        // DeepSeek / Z.ai / OpenRouter expose chain-of-thought here.
        for key in ["reasoning_content", "reasoning"] {
            if let Some(text) = delta.get(key).and_then(Value::as_str) {
                if !text.is_empty() {
                    deltas.push(Delta::Reasoning { text: text.to_string() });
                }
            }
        }

        // Streaming tool calls: fragments are keyed by index.
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for (position, call) in calls.iter().enumerate() {
                let index = call
                    .get("index")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize)
                    .unwrap_or(position);
                let id = call.get("id").and_then(Value::as_str).map(str::to_string);
                let name = call
                    .pointer("/function/name")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                let argument_fragment = call
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if id.is_some() || name.is_some() || argument_fragment.is_some() {
                    deltas.push(Delta::ToolCall {
                        index,
                        id,
                        name,
                        argument_fragment,
                    });
                }
            }
        }
    }

    Ok(if deltas.is_empty() { None } else { Some(deltas) })
}

/// Non-streaming response â†’ (content, reasoning, usage).
pub fn parse_response(value: &Value, status: u16) -> Result<(String, Option<String>, Usage)> {
    if !(200..300).contains(&status) {
        return Err(Error::Provider(error_message(value, status)));
    }
    let message = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .ok_or_else(|| Error::Provider("response contained no message".into()))?;

    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let reasoning = message
        .get("reasoning_content")
        .or_else(|| message.get("reasoning"))
        .and_then(Value::as_str)
        .map(str::to_string);

    Ok((content, reasoning, usage_of(value)))
}

pub fn usage_of(value: &Value) -> Usage {
    value
        .get("usage")
        .map(|usage| Usage {
            input_tokens: usage
                .get("prompt_tokens")
                .and_then(Value::as_u64)
                .map(|v| v as u32),
            output_tokens: usage
                .get("completion_tokens")
                .and_then(Value::as_u64)
                .map(|v| v as u32),
        })
        .unwrap_or_default()
}

/// Extracts a human-readable error from an OpenAI-shaped error body.
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
    use crate::provider::ProviderConfig;

    fn request(provider: &ProviderConfig) -> ChatRequest<'_> {
        ChatRequest {
            provider,
            model: "gpt-4o",
            system: Some("You are helpful."),
            messages: vec![super::super::WireMessage::text("user", "hi")],
            variant: None,
            max_output_tokens: Some(1024),
            stream: true,
            tools: Vec::new(),
            session_id: None,
        }
    }

    #[test]
    fn body_includes_system_and_stream_options() {
        let provider = ProviderConfig::default();
        let body = build_body(&request(&provider));
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "hi");
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert_eq!(body["max_tokens"], 1024);
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn variant_becomes_reasoning_effort_unless_off() {
        let provider = ProviderConfig::default();
        let mut req = request(&provider);
        req.variant = Some("high");
        assert_eq!(build_body(&req)["reasoning_effort"], "high");

        let mut req = request(&provider);
        req.variant = Some("off");
        assert!(build_body(&req).get("reasoning_effort").is_none());
    }

    #[test]
    fn parses_text_and_reasoning_deltas() {
        let chunk = r#"{"choices":[{"delta":{"content":"Hel","reasoning_content":"hmm"}}]}"#;
        let deltas = parse_chunk(chunk).unwrap().unwrap();
        assert_eq!(deltas.len(), 2);
        assert_eq!(deltas[0], Delta::Text { text: "Hel".into() });
        assert_eq!(deltas[1], Delta::Reasoning { text: "hmm".into() });
    }

    #[test]
    fn parses_usage_and_done() {
        let chunk = r#"{"choices":[],"usage":{"prompt_tokens":12,"completion_tokens":4}}"#;
        let deltas = parse_chunk(chunk).unwrap().unwrap();
        assert_eq!(
            deltas[0],
            Delta::Usage {
                input_tokens: Some(12),
                output_tokens: Some(4)
            }
        );
        assert!(parse_chunk("[DONE]").unwrap().is_none());
    }

    #[test]
    fn surfaces_provider_errors() {
        let chunk = r#"{"error":{"message":"rate limited"}}"#;
        let error = parse_chunk(chunk).unwrap_err();
        assert!(error.to_string().contains("rate limited"));
    }

    #[test]
    fn non_streaming_response_parses() {
        let value = serde_json::json!({
            "choices": [{ "message": { "content": "hello", "reasoning_content": "why" } }],
            "usage": { "prompt_tokens": 3, "completion_tokens": 5 }
        });
        let (content, reasoning, usage) = parse_response(&value, 200).unwrap();
        assert_eq!(content, "hello");
        assert_eq!(reasoning.as_deref(), Some("why"));
        assert_eq!(usage.output_tokens, Some(5));
    }

    #[test]
    fn endpoint_appends_the_chat_route() {
        let provider = ProviderConfig {
            base_url: "https://api.example.com".into(),
            ..Default::default()
        };
        assert_eq!(endpoint(&provider), "https://api.example.com/v1/chat/completions");
    }
}


