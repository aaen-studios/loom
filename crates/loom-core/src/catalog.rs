//! Bundled model metadata.
//!
//! A models.dev-style lookup by id pattern. It fills context windows,
//! modalities, and reasoning support when `/models` returns bare ids; the user
//! can always override per model in settings.

use crate::provider::{Modality, ModelSpec, ReasoningSpec};

const TEXT: &[Modality] = &[Modality::Text];
const TEXT_IMAGE: &[Modality] = &[Modality::Text, Modality::Image];
const TEXT_IMAGE_PDF: &[Modality] = &[Modality::Text, Modality::Image, Modality::Pdf];

struct Entry {
    pattern: &'static str,
    context: u32,
    output: u32,
    modalities: &'static [Modality],
    reasoning_variants: &'static [&'static str],
    reasoning_default: Option<&'static str>,
}

fn effort_variants() -> Vec<String> {
    ["low", "medium", "high"].iter().map(|v| v.to_string()).collect()
}

fn thinking_variants() -> Vec<String> {
    ["off", "on"].iter().map(|v| v.to_string()).collect()
}

const ENTRIES: &[Entry] = &[
    // OpenAI
    Entry { pattern: "gpt-5", context: 400_000, output: 128_000, modalities: TEXT_IMAGE, reasoning_variants: &["minimal", "low", "medium", "high"], reasoning_default: Some("medium") },
    Entry { pattern: "gpt-4.1", context: 1_000_000, output: 32_000, modalities: TEXT_IMAGE, reasoning_variants: &[], reasoning_default: None },
    Entry { pattern: "gpt-4o", context: 128_000, output: 16_000, modalities: TEXT_IMAGE, reasoning_variants: &[], reasoning_default: None },
    Entry { pattern: "gpt-4", context: 128_000, output: 8_000, modalities: TEXT_IMAGE, reasoning_variants: &[], reasoning_default: None },
    Entry { pattern: "o4-mini", context: 200_000, output: 100_000, modalities: TEXT_IMAGE, reasoning_variants: &["low", "medium", "high"], reasoning_default: Some("medium") },
    Entry { pattern: "o3", context: 200_000, output: 100_000, modalities: TEXT_IMAGE, reasoning_variants: &["low", "medium", "high"], reasoning_default: Some("medium") },
    // Anthropic
    Entry { pattern: "claude-opus-4", context: 200_000, output: 32_000, modalities: TEXT_IMAGE_PDF, reasoning_variants: &["low", "medium", "high"], reasoning_default: Some("medium") },
    Entry { pattern: "claude-sonnet-4", context: 200_000, output: 64_000, modalities: TEXT_IMAGE_PDF, reasoning_variants: &["low", "medium", "high"], reasoning_default: Some("medium") },
    Entry { pattern: "claude-3-7", context: 200_000, output: 64_000, modalities: TEXT_IMAGE_PDF, reasoning_variants: &["low", "medium", "high"], reasoning_default: Some("medium") },
    Entry { pattern: "claude-3-5", context: 200_000, output: 8_192, modalities: TEXT_IMAGE_PDF, reasoning_variants: &[], reasoning_default: None },
    Entry { pattern: "claude", context: 200_000, output: 8_192, modalities: TEXT_IMAGE_PDF, reasoning_variants: &[], reasoning_default: None },
    // Google
    Entry { pattern: "gemini-2.5", context: 1_048_576, output: 65_536, modalities: TEXT_IMAGE_PDF, reasoning_variants: &["off", "low", "high"], reasoning_default: Some("low") },
    Entry { pattern: "gemini", context: 1_048_576, output: 8_192, modalities: TEXT_IMAGE_PDF, reasoning_variants: &[], reasoning_default: None },
    // DeepSeek
    Entry { pattern: "deepseek-reasoner", context: 128_000, output: 64_000, modalities: TEXT, reasoning_variants: &[], reasoning_default: None },
    Entry { pattern: "deepseek", context: 128_000, output: 8_192, modalities: TEXT, reasoning_variants: &[], reasoning_default: None },
    // Z.ai / GLM
    Entry { pattern: "glm-5", context: 1_000_000, output: 128_000, modalities: TEXT_IMAGE, reasoning_variants: &["low", "high", "max"], reasoning_default: Some("max") },
    Entry { pattern: "glm-4.7", context: 200_000, output: 128_000, modalities: TEXT_IMAGE, reasoning_variants: &["off", "on"], reasoning_default: Some("on") },
    Entry { pattern: "glm-4.6", context: 200_000, output: 128_000, modalities: TEXT, reasoning_variants: &["off", "on"], reasoning_default: Some("on") },
    Entry { pattern: "glm", context: 128_000, output: 32_000, modalities: TEXT, reasoning_variants: &[], reasoning_default: None },
    // xAI
    Entry { pattern: "grok-4", context: 256_000, output: 32_000, modalities: TEXT_IMAGE, reasoning_variants: &["low", "high"], reasoning_default: Some("high") },
    Entry { pattern: "grok", context: 131_072, output: 16_000, modalities: TEXT_IMAGE, reasoning_variants: &[], reasoning_default: None },
    // Meta / Qwen / Mistral / local
    Entry { pattern: "llama-4", context: 1_000_000, output: 16_000, modalities: TEXT_IMAGE, reasoning_variants: &[], reasoning_default: None },
    Entry { pattern: "llama", context: 131_072, output: 8_192, modalities: TEXT, reasoning_variants: &[], reasoning_default: None },
    Entry { pattern: "qwen3", context: 262_144, output: 32_000, modalities: TEXT, reasoning_variants: &["off", "on"], reasoning_default: Some("off") },
    Entry { pattern: "qwen", context: 131_072, output: 16_000, modalities: TEXT_IMAGE, reasoning_variants: &[], reasoning_default: None },
    Entry { pattern: "mistral", context: 131_072, output: 16_000, modalities: TEXT, reasoning_variants: &[], reasoning_default: None },
    Entry { pattern: "phi", context: 131_072, output: 8_192, modalities: TEXT, reasoning_variants: &[], reasoning_default: None },
    Entry { pattern: "gemma", context: 131_072, output: 8_192, modalities: TEXT_IMAGE, reasoning_variants: &[], reasoning_default: None },
];

/// Best-effort metadata for a bare model id. `None` when nothing matches.
pub fn lookup(model_id: &str) -> Option<ModelSpec> {
    let haystack = model_id.to_ascii_lowercase();
    let entry = ENTRIES
        .iter()
        // Longest pattern wins, so `gemini-2.5` beats `gemini`.
        .filter(|entry| haystack.contains(entry.pattern))
        .max_by_key(|entry| entry.pattern.len())?;

    let reasoning = if entry.reasoning_variants.is_empty() {
        None
    } else {
        Some(ReasoningSpec {
            enabled: true,
            variants: entry
                .reasoning_variants
                .iter()
                .map(|v| v.to_string())
                .collect(),
            default_variant: entry.reasoning_default.map(|v| v.to_string()),
        })
    };

    Some(ModelSpec {
        name: None,
        context: Some(entry.context),
        output: Some(entry.output),
        input_modalities: entry.modalities.to_vec(),
        reasoning,
        favorite: false,
    })
}

/// Fallback used when nothing in the catalog matches.
pub fn fallback() -> ModelSpec {
    ModelSpec {
        name: None,
        context: None,
        output: None,
        input_modalities: TEXT.to_vec(),
        reasoning: None,
        favorite: false,
    }
}

/// Reasoning variants for a generic model that supports thinking toggling.
pub fn toggle_variants() -> ReasoningSpec {
    ReasoningSpec {
        enabled: true,
        variants: thinking_variants(),
        default_variant: Some("on".into()),
    }
}

pub fn default_effort() -> Vec<String> {
    effort_variants()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longest_pattern_wins() {
        let spec = lookup("google/gemini-2.5-pro").unwrap();
        assert_eq!(spec.context, Some(1_048_576));
        assert!(spec.reasoning.is_some());
    }

    #[test]
    fn unknown_models_have_no_metadata() {
        assert!(lookup("totally-custom-model-9000").is_none());
        assert!(fallback().context.is_none());
    }

    #[test]
    fn openrouter_style_ids_match() {
        let spec = lookup("anthropic/claude-sonnet-4-20250514").unwrap();
        assert_eq!(spec.output, Some(64_000));
        assert_eq!(spec.input_modalities.len(), 3);
    }

    #[test]
    fn deepseek_reasoner_is_not_confused_with_chat() {
        let reasoner = lookup("deepseek-reasoner").unwrap();
        assert_eq!(reasoner.context, Some(128_000));
        assert!(reasoner.reasoning.is_none());
    }
}
