//! Bundled model metadata.
//!
//! A models.dev-style lookup by id pattern. It fills context windows,
//! modalities, reasoning support, and list prices when `/models` returns bare
//! ids; the user can always override per model in settings.
//!
//! Matching is anchored: a pattern only matches when the last path segment of
//! the model id *starts with* it (`anthropic/claude-sonnet-4-20250514` matches
//! `claude-sonnet-4`; `gryphius` does not match `phi`). The longest matching
//! pattern wins, so `gemini-2.5` beats `gemini`.

use crate::provider::{MetadataSource, Modality, ModelSpec, ReasoningSpec};

/// Bumped whenever entry values change enough that previously detected
/// metadata should be corrected. Configs carry `catalog@N`; a refresh with a
/// higher N overwrites them.
pub const CATALOG_VERSION: u32 = 1;

const TEXT: &[Modality] = &[Modality::Text];
const TEXT_IMAGE: &[Modality] = &[Modality::Text, Modality::Image];
const TEXT_IMAGE_PDF: &[Modality] = &[Modality::Text, Modality::Image, Modality::Pdf];

struct Entry {
    pattern: &'static str,
    context: Option<u32>,
    output: Option<u32>,
    modalities: &'static [Modality],
    reasoning_variants: &'static [&'static str],
    reasoning_default: Option<&'static str>,
    /// USD per 1M tokens from published list prices. Gateway prices differ, so
    /// these are an estimate the user can correct per model.
    input_price: Option<f32>,
    output_price: Option<f32>,
}

fn thinking_variants() -> Vec<String> {
    ["off", "on"].iter().map(|v| v.to_string()).collect()
}

macro_rules! entry {
    ($pattern:expr, $context:expr, $output:expr, $modalities:expr, $variants:expr, $default:expr) => {
        Entry {
            pattern: $pattern,
            context: Some($context),
            output: Some($output),
            modalities: $modalities,
            reasoning_variants: $variants,
            reasoning_default: $default,
            input_price: None,
            output_price: None,
        }
    };
    ($pattern:expr, $context:expr, $output:expr, $modalities:expr, $variants:expr, $default:expr, $in:expr, $out:expr) => {
        Entry {
            pattern: $pattern,
            context: Some($context),
            output: Some($output),
            modalities: $modalities,
            reasoning_variants: $variants,
            reasoning_default: $default,
            input_price: Some($in),
            output_price: Some($out),
        }
    };
}

/// A family the catalogue recognises but knows nothing about. Matching keeps
/// the id out of `fallback()` without pretending any value is real.
macro_rules! unknown_entry {
    ($pattern:expr) => {
        Entry {
            pattern: $pattern,
            context: None,
            output: None,
            modalities: &[],
            reasoning_variants: &[],
            reasoning_default: None,
            input_price: None,
            output_price: None,
        }
    };
}

const ENTRIES: &[Entry] = &[
    // OpenAI
    entry!(
        "gpt-5.6",
        400_000,
        128_000,
        TEXT_IMAGE,
        &["minimal", "low", "medium", "high"],
        Some("medium"),
        1.25,
        10.0
    ),
    entry!(
        "gpt-5",
        400_000,
        128_000,
        TEXT_IMAGE,
        &["minimal", "low", "medium", "high"],
        Some("medium"),
        1.25,
        10.0
    ),
    entry!(
        "gpt-4.1",
        1_000_000,
        32_000,
        TEXT_IMAGE,
        &[],
        None,
        2.0,
        8.0
    ),
    entry!("gpt-4o", 128_000, 16_000, TEXT_IMAGE, &[], None, 2.5, 10.0),
    entry!("gpt-4", 128_000, 8_000, TEXT_IMAGE, &[], None, 2.5, 10.0),
    entry!(
        "o4-mini",
        200_000,
        100_000,
        TEXT_IMAGE,
        &["low", "medium", "high"],
        Some("medium"),
        1.1,
        4.4
    ),
    entry!(
        "o3",
        200_000,
        100_000,
        TEXT_IMAGE,
        &["low", "medium", "high"],
        Some("medium"),
        2.0,
        8.0
    ),
    // Anthropic
    entry!(
        "claude-opus-4",
        200_000,
        32_000,
        TEXT_IMAGE_PDF,
        &["low", "medium", "high"],
        Some("medium"),
        15.0,
        75.0
    ),
    entry!(
        "claude-sonnet-4",
        200_000,
        64_000,
        TEXT_IMAGE_PDF,
        &["low", "medium", "high"],
        Some("medium"),
        3.0,
        15.0
    ),
    entry!(
        "claude-3-7",
        200_000,
        64_000,
        TEXT_IMAGE_PDF,
        &["low", "medium", "high"],
        Some("medium"),
        3.0,
        15.0
    ),
    entry!(
        "claude-3-5",
        200_000,
        8_192,
        TEXT_IMAGE_PDF,
        &[],
        None,
        3.0,
        15.0
    ),
    entry!(
        "claude",
        200_000,
        8_192,
        TEXT_IMAGE_PDF,
        &[],
        None,
        3.0,
        15.0
    ),
    // Google
    entry!(
        "gemini-2.5",
        1_048_576,
        65_536,
        TEXT_IMAGE_PDF,
        &["off", "low", "high"],
        Some("low"),
        1.25,
        10.0
    ),
    entry!(
        "gemini",
        1_048_576,
        8_192,
        TEXT_IMAGE_PDF,
        &[],
        None,
        0.3,
        2.5
    ),
    // DeepSeek. The v4 line is split so the vision and pro rows can carry
    // capabilities the bare family entry does not.
    entry!(
        "deepseek-v4-flash-vision",
        128_000,
        8_192,
        TEXT_IMAGE,
        &[],
        None,
        0.27,
        1.1
    ),
    entry!(
        "deepseek-v4-pro",
        128_000,
        8_192,
        TEXT,
        &["off", "on"],
        Some("on"),
        0.55,
        2.19
    ),
    entry!("deepseek-v4", 128_000, 8_192, TEXT, &[], None, 0.27, 1.1),
    entry!(
        "deepseek-reasoner",
        128_000,
        64_000,
        TEXT,
        &[],
        None,
        0.55,
        2.19
    ),
    entry!("deepseek", 128_000, 8_192, TEXT, &[], None, 0.27, 1.1),
    // Z.ai / GLM
    entry!(
        "glm-5.3-flash",
        1_000_000,
        128_000,
        TEXT_IMAGE,
        &["low", "high", "max"],
        Some("max"),
        0.3,
        1.2
    ),
    entry!(
        "glm-5.3",
        1_000_000,
        128_000,
        TEXT_IMAGE,
        &["low", "high", "max"],
        Some("max"),
        1.0,
        3.2
    ),
    entry!(
        "glm-5",
        1_000_000,
        128_000,
        TEXT_IMAGE,
        &["low", "high", "max"],
        Some("max"),
        1.0,
        3.2
    ),
    entry!(
        "glm-4.7",
        200_000,
        128_000,
        TEXT_IMAGE,
        &["off", "on"],
        Some("on"),
        0.6,
        2.2
    ),
    entry!(
        "glm-4.6",
        200_000,
        128_000,
        TEXT,
        &["off", "on"],
        Some("on"),
        0.6,
        2.2
    ),
    entry!("glm", 128_000, 32_000, TEXT, &[], None, 0.6, 2.2),
    // xAI
    entry!(
        "grok-4.6",
        256_000,
        32_000,
        TEXT_IMAGE,
        &["low", "high"],
        Some("high"),
        3.0,
        15.0
    ),
    entry!(
        "grok-4.5",
        256_000,
        32_000,
        TEXT_IMAGE,
        &["low", "high"],
        Some("high"),
        3.0,
        15.0
    ),
    entry!(
        "grok-4",
        256_000,
        32_000,
        TEXT_IMAGE,
        &["low", "high"],
        Some("high"),
        3.0,
        15.0
    ),
    entry!("grok", 131_072, 16_000, TEXT_IMAGE, &[], None, 3.0, 15.0),
    // Open-weight families: the host sets the price, so none is assumed.
    entry!("llama-4", 1_000_000, 16_000, TEXT_IMAGE, &[], None),
    entry!("llama", 131_072, 8_192, TEXT, &[], None),
    entry!("qwen3", 262_144, 32_000, TEXT, &["off", "on"], Some("off")),
    entry!("qwen", 131_072, 16_000, TEXT_IMAGE, &[], None),
    entry!("mistral", 131_072, 16_000, TEXT, &[], None),
    entry!("phi", 131_072, 8_192, TEXT, &[], None),
    entry!("gemma", 131_072, 8_192, TEXT_IMAGE, &[], None),
    // Tencent Hunyuan: the family is recognised, the metadata is not.
    unknown_entry!("hy"),
];

fn spec_from(entry: &Entry) -> ModelSpec {
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

    let mut spec = ModelSpec {
        name: None,
        context: entry.context,
        output: entry.output,
        input_modalities: entry.modalities.to_vec(),
        reasoning,
        favorite: false,
        input_price: entry.input_price,
        output_price: entry.output_price,
        source: MetadataSource::Catalog(CATALOG_VERSION),
    };
    if !spec.has_metadata() {
        // An explicit unknown entry: matching is real, the values are not.
        spec.source = MetadataSource::Unknown;
    }
    spec
}

/// Best-effort metadata for a bare model id. `None` when nothing matches.
pub fn lookup(model_id: &str) -> Option<ModelSpec> {
    let haystack = model_id.to_ascii_lowercase();
    let segment = haystack.rsplit('/').next().unwrap_or(&haystack);
    let entry = ENTRIES
        .iter()
        // Anchored prefix match on the last path segment; longest wins, so
        // `deepseek-v4-pro` beats `deepseek-v4`, which beats `deepseek`.
        .filter(|entry| segment.starts_with(entry.pattern))
        .max_by_key(|entry| entry.pattern.len())?;

    Some(spec_from(entry))
}

/// Fallback used when nothing in the catalog matches. Every field is unknown:
/// empty modalities mean "no idea", not "text only", so a later refresh can
/// still add capabilities.
pub fn fallback() -> ModelSpec {
    ModelSpec::default()
}

/// Reasoning variants for a generic model that supports thinking toggling.
pub fn toggle_variants() -> ReasoningSpec {
    ReasoningSpec {
        enabled: true,
        variants: thinking_variants(),
        default_variant: Some("on".into()),
    }
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
        let fallback = fallback();
        assert!(fallback.context.is_none());
        assert!(fallback.input_modalities.is_empty());
        assert_eq!(fallback.source, MetadataSource::Unknown);
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

    #[test]
    fn prices_come_through_and_are_optional() {
        let hosted = lookup("gpt-4o").unwrap();
        assert_eq!(hosted.input_price, Some(2.5));
        assert_eq!(hosted.output_price, Some(10.0));

        // Open-weight models have no list price: the host decides.
        let local = lookup("qwen3:8b").unwrap();
        assert!(local.input_price.is_none());
        assert!(local.output_price.is_none());
    }

    #[test]
    fn matching_is_anchored_to_the_start_of_the_last_segment() {
        // `phi` must not be found inside `gryphius`, `o3` not inside `foo3`.
        assert!(lookup("gryphius-2").is_none());
        assert!(lookup("foo3").is_none());
        // But a provider-prefixed id still matches on its last segment.
        assert!(lookup("openai/gpt-4o").is_some());
        assert!(lookup("meta-llama/llama-3.1-8b").is_some());
    }

    #[test]
    fn new_entries_carry_their_capabilities() {
        let vision = lookup("deepseek-v4-flash-vision-exp").unwrap();
        assert!(vision.input_modalities.contains(&Modality::Image));
        assert_eq!(vision.source, MetadataSource::Catalog(CATALOG_VERSION));

        let pro = lookup("deepseek-v4-pro").unwrap();
        assert!(pro.reasoning.is_some());
        assert!(!pro.input_modalities.contains(&Modality::Image));

        let flash = lookup("deepseek-v4-flash").unwrap();
        assert!(flash.reasoning.is_none());
        assert!(flash.input_modalities.contains(&Modality::Text));
    }

    #[test]
    fn newest_family_rows_win_over_their_generic_ancestors() {
        assert_eq!(lookup("glm-5.3-flash").unwrap().context, Some(1_000_000));
        assert_eq!(lookup("glm-5.3").unwrap().context, Some(1_000_000));
        assert_eq!(lookup("glm-5.2").unwrap().context, Some(1_000_000));
        assert_eq!(lookup("grok-4.6").unwrap().context, Some(256_000));
        assert_eq!(lookup("gpt-5.6-luna").unwrap().context, Some(400_000));
        assert!(lookup("gpt-5.6-luna").unwrap().reasoning.is_some());
    }

    #[test]
    fn explicitly_unknown_families_match_without_pretending() {
        let hunyuan = lookup("hy3-preview").unwrap();
        assert!(hunyuan.context.is_none());
        assert!(hunyuan.output.is_none());
        assert!(hunyuan.input_modalities.is_empty());
        assert!(hunyuan.reasoning.is_none());
        assert_eq!(hunyuan.source, MetadataSource::Unknown);
    }
}
