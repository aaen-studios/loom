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
    // This dialect's `tool` role is text-only, so screenshots a tool produced
    // follow as user messages. They must come after the whole run of tool
    // results: strict validators (DeepSeek) reject the request if anything
    // sits between an assistant `tool_calls` turn and the tool messages
    // answering it.
    let mut pending_images: Vec<Value> = Vec::new();
    for message in &request.messages {
        if message.role != "tool" && !pending_images.is_empty() {
            messages.append(&mut pending_images);
        }
        messages.push(message_json(message));
        if message.role == "tool" {
            if let Some(follow_up) = tool_images_message(message) {
                pending_images.push(follow_up);
            }
        }
    }
    messages.append(&mut pending_images);

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
    if let Some(temperature) = request.temperature {
        body["temperature"] = json!(temperature);
    }
    if let Some(top_p) = request.top_p {
        body["top_p"] = json!(top_p);
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
fn message_json(message: &super::WireMessage) -> Value {    // Tool results use a dedicated shape in this dialect.
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

    // DeepSeek-family gateways require the assistant's own thinking back:
    // "The `reasoning_content` in the thinking mode must be passed back".
    if message.role == "assistant" {
        if let Some(reasoning) = message
            .reasoning
            .as_deref()
            .filter(|text| !text.trim().is_empty())
        {
            object["reasoning_content"] = json!(reasoning);
        }
    }

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

/// The follow-up user message that carries a tool's images, or `None` when the
/// tool result had none.
fn tool_images_message(message: &super::WireMessage) -> Option<Value> {
    let mut parts: Vec<Value> = Vec::new();
    for part in &message.parts {
        if let super::ContentPart::Image { mime, base64, .. } = part {
            parts.push(json!({
                "type": "image_url",
                "image_url": { "url": format!("data:{mime};base64,{base64}") }
            }));
        }
    }
    if parts.is_empty() {
        return None;
    }
    let mut content = vec![json!({
        "type": "text",
        "text": "Screenshot from the computer tool:"
    })];
    content.append(&mut parts);
    Some(json!({ "role": "user", "content": content }))
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
                deltas.push(Delta::Text {
                    text: text.to_string(),
                });
            }
        }
        // DeepSeek / Z.ai / OpenRouter expose chain-of-thought here.
        for key in ["reasoning_content", "reasoning"] {
            if let Some(text) = delta.get(key).and_then(Value::as_str) {
                if !text.is_empty() {
                    deltas.push(Delta::Reasoning {
                        text: text.to_string(),
                    });
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

    Ok(if deltas.is_empty() {
        None
    } else {
        Some(deltas)
    })
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
            temperature: None,
            top_p: None,
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
    fn assistant_reasoning_is_echoed_back() {
        let provider = ProviderConfig::default();
        let assistant = super::super::WireMessage {
            role: "assistant".into(),
            parts: vec![super::super::ContentPart::Text {
                text: "Oslo.".into(),
            }],
            tool_calls: Vec::new(),
            tool_call_id: None,
            reasoning: Some("The user asked about Norway.".into()),
        };
        let user = super::super::WireMessage::text("user", "capital of Norway?");

        let mut req = request(&provider);
        req.messages = vec![user, assistant];
        let body = build_body(&req);

        // The request helper includes a system message, so the assistant turn
        // is the last entry.
        let sent = body["messages"].as_array().expect("messages");
        let last = sent.last().expect("an assistant message");
        assert_eq!(last["role"], "assistant");
        // DeepSeek-family gateways reject the request without this field.
        assert_eq!(last["reasoning_content"], "The user asked about Norway.");
        assert_eq!(last["content"], "Oslo.");
        // Plain user turns carry no reasoning field at all.
        assert!(sent[1].get("reasoning_content").is_none());
    }

    #[test]
    fn empty_reasoning_is_omitted() {
        let provider = ProviderConfig::default();
        let assistant = super::super::WireMessage {
            role: "assistant".into(),
            parts: vec![super::super::ContentPart::Text { text: "hi".into() }],
            tool_calls: Vec::new(),
            tool_call_id: None,
            reasoning: Some("   ".into()),
        };
        let mut req = request(&provider);
        req.messages = vec![assistant];
        let body = build_body(&req);
        assert!(body["messages"][0].get("reasoning_content").is_none());
    }

    #[test]
    fn tool_images_follow_the_whole_run_of_tool_results() {
        let provider = ProviderConfig::default();
        let assistant = super::super::WireMessage {
            role: "assistant".into(),
            parts: Vec::new(),
            tool_calls: vec![
                super::super::WireToolCall {
                    id: "call_screen".into(),
                    name: "screenshot".into(),
                    arguments: "{}".into(),
                },
                super::super::WireToolCall {
                    id: "call_windows".into(),
                    name: "list_windows".into(),
                    arguments: "{}".into(),
                },
            ],
            tool_call_id: None,
            reasoning: None,
        };
        let screenshot = super::super::WireMessage::tool_result_with_images(
            "call_screen",
            "captured",
            vec![super::super::ContentPart::Image {
                mime: "image/jpeg".into(),
                base64: "AAAA".into(),
                name: "screen.jpg".into(),
            }],
        );
        let windows = super::super::WireMessage::tool_result("call_windows", "3 windows");

        let mut req = request(&provider);
        req.messages = vec![assistant, screenshot, windows];
        let body = build_body(&req);

        let sent = body["messages"].as_array().expect("messages");
        let roles: Vec<&str> = sent
            .iter()
            .map(|message| message["role"].as_str().unwrap_or_default())
            .collect();
        // Strict validators reject anything between the assistant tool_calls
        // turn and the tool messages answering each of its calls.
        assert_eq!(roles, vec!["system", "assistant", "tool", "tool", "user"]);
        assert_eq!(sent[2]["tool_call_id"], "call_screen");
        assert_eq!(sent[3]["tool_call_id"], "call_windows");
        assert_eq!(sent[4]["content"][1]["type"], "image_url");
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
        assert_eq!(
            endpoint(&provider),
            "https://api.example.com/v1/chat/completions"
        );
    }
}
