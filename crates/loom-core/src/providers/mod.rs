//! Provider adapters: request building, SSE parsing, and streaming.

pub mod anthropic;
pub mod detect;
pub mod openai;
pub mod sse;
pub mod stream;

use serde::{Deserialize, Serialize};

use crate::provider::ProviderConfig;

/// A normalised streaming delta produced by every adapter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Delta {
    Text {
        text: String,
    },
    Reasoning {
        text: String,
    },
    Usage {
        input_tokens: Option<u32>,
        output_tokens: Option<u32>,
    },
    /// Streaming fragment of a tool call. `id`/`name` arrive once, the
    /// arguments arrive as JSON fragments that must be concatenated.
    ToolCall {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        argument_fragment: Option<String>,
    },
}

/// Tool description sent to the provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// A complete tool call on the wire (assistant turns that invoke tools).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireToolCall {
    pub id: String,
    pub name: String,
    /// JSON string.
    pub arguments: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ContentPart {
    Text { text: String },
    Image {
        mime: String,
        base64: String,
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WireMessage {
    pub role: String,
    pub parts: Vec<ContentPart>,
    /// Set on assistant turns that invoke tools.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<WireToolCall>,
    /// Set on tool-result turns (role `tool`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// The assistant''s own thinking, echoed back for providers that require it
    /// (DeepSeek-family gateways reject the request otherwise).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
}

impl WireMessage {
    pub fn text(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            parts: vec![ContentPart::Text {
                text: content.into(),
            }],
            tool_calls: Vec::new(),
            tool_call_id: None,
            reasoning: None,
        }
    }

    pub fn tool_result(call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            parts: vec![ContentPart::Text {
                text: content.into(),
            }],
            tool_calls: Vec::new(),
            tool_call_id: Some(call_id.into()),
            reasoning: None,
        }
    }

    /// True when every part is text (lets adapters send a plain string, which
    /// older local models require).
    pub fn is_text_only(&self) -> bool {
        self.parts
            .iter()
            .all(|part| matches!(part, ContentPart::Text { .. }))
    }

    pub fn joined_text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|part| match part {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

/// Everything an adapter needs to build one request.
pub struct ChatRequest<'a> {
    pub provider: &'a ProviderConfig,
    pub model: &'a str,
    pub system: Option<&'a str>,
    pub messages: Vec<WireMessage>,
    pub variant: Option<&'a str>,
    pub max_output_tokens: Option<u32>,
    pub stream: bool,
    pub tools: Vec<ToolDef>,
    /// Stable per-conversation id, sent when the provider asks for one.
    pub session_id: Option<&'a str>,
}

impl ChatRequest<'_> {
    /// HTTP headers shared by both dialects (auth + content type + extras).
    pub fn headers(&self, api_key: Option<&str>) -> Vec<(String, String)> {
        let mut headers = vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("accept".to_string(), "text/event-stream".to_string()),
        ];
        if let Some(key) = api_key {
            match self.provider.kind {
                crate::provider::ProviderKind::Anthropic => {
                    headers.push(("x-api-key".to_string(), key.to_string()));
                    headers.push((
                        "anthropic-version".to_string(),
                        "2023-06-01".to_string(),
                    ));
                }
                crate::provider::ProviderKind::OpenaiCompatible => {
                    headers.push(("authorization".to_string(), format!("Bearer {key}")));
                }
            }
        }

        // Gateways such as OpenCode Go require a stable conversation id so they
        // can route requests and cache prompts; without it they reject the call.
        if let (Some(name), Some(session)) = (self.provider.session_header.as_deref(), self.session_id)
        {
            if !name.trim().is_empty() && !session.trim().is_empty() {
                headers.push((name.to_string(), session.to_string()));
            }
        }

        for (name, value) in &self.provider.headers {
            headers.push((name.clone(), value.clone()));
        }
        headers
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ProviderConfig, ProviderKind};

    fn request<'a>(provider: &'a ProviderConfig, session: Option<&'a str>) -> ChatRequest<'a> {
        ChatRequest {
            provider,
            model: "model",
            system: None,
            messages: vec![WireMessage::text("user", "hi")],
            variant: None,
            max_output_tokens: None,
            stream: true,
            tools: Vec::new(),
            session_id: session,
        }
    }

    fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
        headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    #[test]
    fn session_header_is_sent_when_the_provider_asks_for_one() {
        let provider = ProviderConfig {
            base_url: "https://opencode.ai/zen/go/v1".into(),
            session_header: Some("x-opencode-session".into()),
            ..Default::default()
        };

        let headers = request(&provider, Some("chat-123")).headers(Some("key"));
        assert_eq!(header(&headers, "x-opencode-session"), Some("chat-123"));
        assert_eq!(header(&headers, "authorization"), Some("Bearer key"));

        // Without a session id the header is omitted rather than sent empty.
        let headers = request(&provider, None).headers(Some("key"));
        assert_eq!(header(&headers, "x-opencode-session"), None);
    }

    #[test]
    fn session_header_is_not_added_for_ordinary_providers() {
        let provider = ProviderConfig {
            base_url: "https://api.openai.com/v1".into(),
            ..Default::default()
        };
        let headers = request(&provider, Some("chat-123")).headers(Some("key"));
        assert_eq!(header(&headers, "x-opencode-session"), None);
    }

    #[test]
    fn anthropic_uses_its_own_auth_headers() {
        let provider = ProviderConfig {
            kind: ProviderKind::Anthropic,
            base_url: "https://api.anthropic.com".into(),
            ..Default::default()
        };
        let headers = request(&provider, None).headers(Some("key"));
        assert_eq!(header(&headers, "x-api-key"), Some("key"));
        assert_eq!(header(&headers, "anthropic-version"), Some("2023-06-01"));
        assert_eq!(header(&headers, "authorization"), None);
    }
}

