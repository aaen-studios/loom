//! The filter lists Loom offers, fetching them, and keeping them.
//!
//! Here rather than in the shell because `loom-core` is what owns an HTTP
//! client. The shell's blocker is the WebView2 half — the request filter and the
//! verdict — and this is the half that knows what a list is.

use std::time::Duration;

use crate::{Error, Result};

/// The lists Loom offers, and where they come from.
///
/// The same lists uBlock Origin uses where they exist in list form: EasyList is
/// what most of the web means by "an ad blocker", and Peter Lowe's list is the
/// compact hosts-format one that catches a surprising amount on its own.
///
/// They are fetched once and cached in `~/.loom/browser/filters/`, so a browser
/// that has ever been online keeps blocking when it is not — and so a list is
/// never downloaded twice for nothing.
pub const LIST_PRESETS: [(&str, &str, &str); 4] = [
    (
        "peter-lowe",
        "Peter Lowe's list (hosts, small)",
        "https://pgl.yoyo.org/adservers/serverlist.php?hostformat=hosts&showintro=0&mimetype=plaintext",
    ),
    (
        "easylist",
        "EasyList",
        "https://easylist.to/easylist/easylist.txt",
    ),
    (
        "easyprivacy",
        "EasyPrivacy (trackers)",
        "https://easylist.to/easylist/easyprivacy.txt",
    ),
    (
        "ublock-filters",
        "uBlock Origin filters",
        "https://raw.githubusercontent.com/uBlockOrigin/uAssets/master/filters/filters.txt",
    ),
];

/// How long a cached list is used before it is fetched again.
///
/// Three days. Lists are append-mostly and a day-old one blocks essentially what
/// a fresh one does, so refetching more often is a bandwidth cost with no
/// behavioural gain — and this is the only network traffic the blocker makes.
pub const CACHE_MAX_AGE: Duration = Duration::from_secs(60 * 60 * 24 * 3);

/// Where a cached list lives.
pub fn list_cache_path(id: &str) -> Result<std::path::PathBuf> {
    let dir = crate::paths::browser_dir()?.join("filters");
    std::fs::create_dir_all(&dir).map_err(|error| Error::io(&dir, error))?;
    Ok(dir.join(format!("{id}.txt")))
}

/// One list's fetch state, so a list that failed says so rather than being
/// silently absent — a blocker quietly running on one list instead of three is
/// the kind of thing nobody notices until an ad appears.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceStatus {
    pub id: String,
    pub name: String,
    pub fetched_at: Option<i64>,
    pub error: Option<String>,
    /// Rules this list contributed.
    pub rules: u32,
}

/// Fetches one list, from the cache when it is fresh.
pub async fn fetch_list(
    client: &reqwest::Client,
    id: &str,
    url: &str,
    max_age: Duration,
) -> Result<String> {
    let path = list_cache_path(id)?;

    if let Ok(metadata) = std::fs::metadata(&path) {
        let fresh = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age < max_age);
        if fresh {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if !text.trim().is_empty() {
                    return Ok(text);
                }
            }
        }
    }

    let response = client
        .get(url)
        .header("user-agent", "Loom")
        .send()
        .await
        .map_err(|error| Error::Http(format!("could not fetch {url}: {error}")))?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        // A stale cache beats no blocking at all, so it is used when the
        // *network* is the thing that failed.
        if let Ok(text) = std::fs::read_to_string(&path) {
            if !text.trim().is_empty() {
                return Ok(text);
            }
        }
        return Err(Error::Http(format!("{url} returned HTTP {status}")));
    }
    let text = response
        .text()
        .await
        .map_err(|error| Error::Http(format!("could not read {url}: {error}")))?;
    if text.trim().is_empty() {
        return Err(Error::Http(format!("{url} was empty")));
    }
    // A failed write is not a failed load: the rules are in memory either way
    // and the next launch simply refetches.
    let _ = std::fs::write(&path, &text);
    Ok(text)
}

/// What a load produced.
pub struct Loaded {
    pub filters: super::filters::Filters,
    pub sources: Vec<SourceStatus>,
}

/// Builds a filter set from the configured lists.
pub async fn load(
    client: &reqwest::Client,
    config: &crate::config::BlockingConfig,
) -> Loaded {
    let mut filters = super::filters::Filters::new();
    let mut sources = Vec::new();

    for (id, name, url) in LIST_PRESETS {
        if !config.lists.iter().any(|wanted| wanted == id) {
            continue;
        }
        let before = filters.stats().rules;
        match fetch_list(client, id, url, CACHE_MAX_AGE).await {
            Ok(text) => {
                filters.extend(name, &text);
                sources.push(SourceStatus {
                    id: id.to_string(),
                    name: name.to_string(),
                    fetched_at: Some(crate::db::now_ms()),
                    error: None,
                    rules: filters.stats().rules.saturating_sub(before),
                });
            }
            Err(error) => {
                eprintln!("[loom] blocking: {id} unavailable: {error}");
                sources.push(SourceStatus {
                    id: id.to_string(),
                    name: name.to_string(),
                    fetched_at: None,
                    error: Some(error.to_string()),
                    rules: 0,
                });
            }
        }
    }

    for (index, url) in config.custom_lists.iter().enumerate() {
        if url.trim().is_empty() {
            continue;
        }
        let id = format!("custom-{index}");
        let before = filters.stats().rules;
        match fetch_list(client, &id, url, CACHE_MAX_AGE).await {
            Ok(text) => {
                filters.extend(url, &text);
                sources.push(SourceStatus {
                    id,
                    name: url.clone(),
                    fetched_at: Some(crate::db::now_ms()),
                    error: None,
                    rules: filters.stats().rules.saturating_sub(before),
                });
            }
            Err(error) => sources.push(SourceStatus {
                id,
                name: url.clone(),
                fetched_at: None,
                error: Some(error.to_string()),
                rules: 0,
            }),
        }
    }

    Loaded { filters, sources }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_has_an_id_a_name_and_an_https_url() {
        // A preset with a non-https URL would be a plaintext fetch of something
        // that decides what the browser is allowed to load.
        for (id, name, url) in LIST_PRESETS {
            assert!(!id.is_empty(), "a preset has no id");
            assert!(!name.is_empty(), "{id} has no name");
            assert!(url.starts_with("https://"), "{id} is not https: {url}");
            // Ids become file names, so a slash or a space would escape the
            // cache directory or produce a path that cannot be written.
            assert!(
                id.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{id} is not a safe file name"
            );
        }
    }

    #[test]
    fn preset_ids_are_unique() {
        // Two lists sharing an id would overwrite each other's cache file, so
        // the second would silently serve the first's rules.
        let mut seen = std::collections::HashSet::new();
        for (id, _, _) in LIST_PRESETS {
            assert!(seen.insert(id), "{id} appears twice");
        }
    }

    #[test]
    fn the_defaults_name_presets_that_exist() {
        // A default naming a preset this build does not have would ship a
        // blocker that loads nothing.
        let config = crate::config::BlockingConfig::default();
        assert!(config.enabled);
        for wanted in &config.lists {
            assert!(
                LIST_PRESETS.iter().any(|(id, _, _)| id == wanted),
                "the default names {wanted}, which is not a preset"
            );
        }
    }
}
