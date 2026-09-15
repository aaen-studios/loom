//! Chat export: renders a session as markdown.

use crate::db::{Message, Role, Session};
use crate::engine::parse_stored_tools;
use crate::providers::Usage;
use crate::attachments;

/// One markdown document per chat: metadata, then every turn, with reasoning
/// collapsed and tool calls summarised.
pub fn session_markdown(session: &Session, messages: &[Message], usage: Option<Usage>) -> String {
    let mut out = String::new();

    let title = if session.title.trim().is_empty() {
        "Untitled chat"
    } else {
        session.title.trim()
    };
    out.push_str(&format!("# {title}\n\n"));

    let mut meta: Vec<String> = Vec::new();
    if let Some(model) = session.model_id.as_deref() {
        match session.provider_id.as_deref() {
            Some(provider) => meta.push(format!("`{provider}/{model}`")),
            None => meta.push(format!("`{model}`")),
        }
    }
    if let Some(variant) = session.variant.as_deref() {
        meta.push(format!("variant: {variant}"));
    }
    meta.push(format!("exported: {}", date_string()));
    if let Some(usage) = usage {
        if let (Some(input), Some(output)) = (usage.input_tokens, usage.output_tokens) {
            meta.push(format!("tokens: {input} in / {output} out"));
        }
    }
    out.push_str(&meta.join(" · "));
    out.push_str("\n\n---\n\n");

    for message in messages {
        match message.role {
            Role::User => {
                out.push_str("## You\n\n");
                let attachments = attachments::parse_extra(message.extra.as_deref());
                for attachment in &attachments {
                    out.push_str(&format!("[attachment: {}]\n\n", attachment.name));
                }
                if !message.content.trim().is_empty() {
                    out.push_str(message.content.trim());
                    out.push_str("\n\n");
                }
            }
            Role::Assistant => {
                out.push_str("## Loom\n\n");
                if let Some(reasoning) = message.reasoning.as_deref() {
                    if !reasoning.trim().is_empty() {
                        out.push_str("<details><summary>Thinking</summary>\n\n");
                        out.push_str(reasoning.trim());
                        out.push_str("\n\n</details>\n\n");
                    }
                }
                for call in parse_stored_tools(message.extra.as_deref()) {
                    out.push_str(&format!(
                        "`{}` — {}\n\n",
                        call.name,
                        match call.status.as_str() {
                            "ok" => "ok",
                            "denied" => "denied",
                            _ => "failed",
                        }
                    ));
                }
                if !message.content.trim().is_empty() {
                    out.push_str(message.content.trim());
                    out.push_str("\n\n");
                }
            }
        }
    }

    out.trim_end().to_string() + "\n"
}

/// `2026-09-15` without pulling in a date library.
fn date_string() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() / 86_400)
        .unwrap_or(0) as i64;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::now_ms;

    fn session() -> Session {
        Session {
            id: "s1".into(),
            title: "Rust help".into(),
            provider_id: Some("opencode-go".into()),
            model_id: Some("deepseek-v4.1-flash".into()),
            variant: Some("medium".into()),
            persona_id: None,
            system_prompt: None,
            workdir: None,
            permission_mode: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn message(role: Role, content: &str) -> Message {
        Message {
            id: uuid::Uuid::new_v4().to_string(),
            session_id: "s1".into(),
            role,
            content: content.into(),
            reasoning: None,
            extra: None,
            created_at: now_ms(),
        }
    }

    #[test]
    fn renders_title_metadata_and_turns() {
        let markdown = session_markdown(
            &session(),
            &[message(Role::User, "why is rust"), message(Role::Assistant, "because")],
            Some(Usage {
                input_tokens: Some(12),
                output_tokens: Some(3),
            }),
        );

        assert!(markdown.starts_with("# Rust help"));
        assert!(markdown.contains("`opencode-go/deepseek-v4.1-flash`"));
        assert!(markdown.contains("variant: medium"));
        assert!(markdown.contains("tokens: 12 in / 3 out"));
        assert!(markdown.contains("## You\n\nwhy is rust"));
        assert!(markdown.contains("## Loom\n\nbecause"));
    }

    #[test]
    fn reasoning_and_tools_are_summarised() {
        let mut assistant = message(Role::Assistant, "answer");
        assistant.reasoning = Some("because reasons".into());
        assistant.extra = Some(
            serde_json::json!({
                "toolCalls": [{
                    "id": "c1",
                    "name": "read_file",
                    "arguments": "{}",
                    "status": "ok",
                    "output": "x"
                }]
            })
            .to_string(),
        );

        let markdown = session_markdown(&session(), &[assistant], None);
        assert!(markdown.contains("<details><summary>Thinking</summary>"));
        assert!(markdown.contains("because reasons"));
        assert!(markdown.contains("`read_file` — ok"));
    }

    #[test]
    fn untitled_and_empty_sessions_still_render() {
        let mut session = session();
        session.title = "   ".into();
        session.model_id = None;
        session.provider_id = None;
        session.variant = None;

        let markdown = session_markdown(&session, &[], None);
        assert!(markdown.starts_with("# Untitled chat"));
        assert!(markdown.contains("exported:"));
        assert!(!markdown.contains("`/"));
    }

    #[test]
    fn dates_look_like_dates() {
        let date = date_string();
        assert_eq!(date.len(), 10);
        assert_eq!(date.matches('-').count(), 2);
    }
}
