//! Web tools: search and page fetching.
//!
//! Jina AI (`s.jina.ai` / `r.jina.ai`) is the preferred backend: its search
//! needs an API key but returns clean SERP data, and its reader renders pages
//! (including JS-heavy ones) as markdown. DuckDuckGo's HTML endpoint and a
//! local tag stripper are the keyless fallbacks.

use std::time::Duration;

use serde_json::{json, Value};

use crate::config::SearchProvider;
use crate::secrets;
use crate::{Error, Result};

pub const SEARCH_TOOL: &str = "web_search";
pub const FETCH_TOOL: &str = "fetch_url";

const JINA_SEARCH_URL: &str = "https://s.jina.ai/";
const JINA_READER_URL: &str = "https://r.jina.ai/";
/// The reader renders pages in a browser; give up rather than stall a turn.
const JINA_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_SNIPPET_CHARS: usize = 400;

pub fn tool_specs() -> Vec<(String, String, Value)> {
    vec![
        (
            SEARCH_TOOL.to_string(),
            "Search the web and return the top results (title, URL, snippet).".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "max_results": { "type": "integer", "description": "How many results (1-10, default 5)" }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        ),
        (
            FETCH_TOOL.to_string(),
            "Fetch a web page and return its readable text (truncated).".to_string(),
            json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "Absolute http(s) URL" }
                },
                "required": ["url"],
                "additionalProperties": false
            }),
        ),
    ]
}

const MAX_PAGE_CHARS: usize = 20_000;

fn jina_key() -> Option<String> {
    secrets::get_named_secret(secrets::JINA_KEY)
        .ok()
        .flatten()
        .filter(|key| !key.trim().is_empty())
}

pub async fn search(
    client: &reqwest::Client,
    query: &str,
    max_results: usize,
    provider: SearchProvider,
) -> Result<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(Error::other("search query is empty"));
    }
    let limit = max_results.clamp(1, 10);
    let key = jina_key();

    match provider {
        SearchProvider::Jina => {
            let key =
                key.ok_or_else(|| Error::other("Jina search needs an API key (Settings → Tools)"))?;
            jina_search(client, trimmed, limit, &key).await
        }
        SearchProvider::Duckduckgo => ddg_search(client, trimmed, limit).await,
        SearchProvider::Auto => match key {
            Some(key) => match jina_search(client, trimmed, limit, &key).await {
                Ok(results) => Ok(results),
                // A paid backend being down should not cost the model its eyes.
                Err(jina_error) => ddg_search(client, trimmed, limit)
                    .await
                    .map_err(|ddg| Error::Http(format!("jina: {jina_error}; duckduckgo: {ddg}"))),
            },
            None => ddg_search(client, trimmed, limit).await,
        },
    }
}

async fn jina_search(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
    key: &str,
) -> Result<String> {
    let response = client
        .get(JINA_SEARCH_URL)
        .query(&[("q", query), ("num", &limit.to_string())])
        .header("accept", "application/json")
        // Page bodies would swamp the model; the SERP fields are the point.
        .header("x-respond-with", "no-content")
        .header("authorization", format!("Bearer {key}"))
        .timeout(JINA_TIMEOUT)
        .send()
        .await
        .map_err(|e| Error::Http(format!("jina search failed: {e}")))?;

    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| Error::Http(format!("jina search read failed: {e}")))?;
    if !(200..300).contains(&status) {
        return Err(Error::Provider(format!(
            "jina search returned HTTP {status}: {}",
            truncate_chars(body.trim(), 200)
        )));
    }

    let parsed: Value = serde_json::from_str(&body)
        .map_err(|e| Error::Http(format!("jina search returned invalid JSON: {e}")))?;
    let results = format_jina_results(&parsed, limit);
    if results.is_empty() {
        return Ok(format!("No results for \"{query}\"."));
    }
    Ok(results.join("\n\n"))
}

/// Renders Jina's SERP JSON as the numbered list the DuckDuckGo parser also
/// produces. `data` is a list of results, or a single object when one URL was
/// read directly.
pub fn format_jina_results(body: &Value, limit: usize) -> Vec<String> {
    let entries: Vec<&Value> = match body.get("data") {
        Some(Value::Array(entries)) => entries.iter().collect(),
        Some(value @ Value::Object(_)) => vec![value],
        _ => Vec::new(),
    };

    entries
        .into_iter()
        .filter_map(|entry| {
            let title = entry
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            let url = entry
                .get("url")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            let snippet = entry
                .get("description")
                .and_then(Value::as_str)
                .or_else(|| entry.get("content").and_then(Value::as_str))
                .unwrap_or_default()
                .trim();

            if title.is_empty() && url.is_empty() {
                return None;
            }
            Some((title.to_string(), url.to_string(), snippet.to_string()))
        })
        .take(limit)
        .enumerate()
        .map(|(index, (title, url, snippet))| {
            format!(
                "{}. {}\n{}\n{}",
                index + 1,
                title,
                url,
                truncate_chars(&snippet, MAX_SNIPPET_CHARS)
            )
        })
        .collect()
}

async fn ddg_search(client: &reqwest::Client, query: &str, limit: usize) -> Result<String> {
    let response = client
        .get("https://html.duckduckgo.com/html/")
        .query(&[("q", query)])
        .header(
            "user-agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Loom/0.1",
        )
        .send()
        .await
        .map_err(|e| Error::Http(format!("search failed: {e}")))?;

    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| Error::Http(format!("search read failed: {e}")))?;
    if !(200..300).contains(&status) {
        return Err(Error::Provider(format!("search returned HTTP {status}")));
    }

    let results = parse_ddg(&body, limit);
    if results.is_empty() {
        return Ok(format!("No results for \"{query}\"."));
    }
    Ok(results.join("\n\n"))
}

pub async fn fetch(
    client: &reqwest::Client,
    url: &str,
    provider: SearchProvider,
) -> Result<String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(Error::other("url must start with http:// or https://"));
    }

    match provider {
        SearchProvider::Duckduckgo => direct_fetch(client, url).await,
        SearchProvider::Jina => jina_fetch(client, url, jina_key().as_deref()).await,
        SearchProvider::Auto => match jina_fetch(client, url, jina_key().as_deref()).await {
            Ok(text) => Ok(text),
            Err(jina_error) => direct_fetch(client, url)
                .await
                .map_err(|e| Error::Http(format!("jina: {jina_error}; direct: {e}"))),
        },
    }
}

/// Jina Reader renders the page (JS included) into markdown.
async fn jina_fetch(client: &reqwest::Client, url: &str, key: Option<&str>) -> Result<String> {
    let mut request = client
        .get(format!("{JINA_READER_URL}{url}"))
        .header("accept", "text/plain")
        .timeout(JINA_TIMEOUT);
    if let Some(key) = key {
        request = request.header("authorization", format!("Bearer {key}"));
    }

    let response = request
        .send()
        .await
        .map_err(|e| Error::Http(format!("fetch failed: {e}")))?;

    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| Error::Http(format!("fetch read failed: {e}")))?;
    if !(200..300).contains(&status) {
        return Err(Error::Provider(format!(
            "jina reader returned HTTP {status}: {}",
            truncate_chars(body.trim(), 200)
        )));
    }

    let text = body.trim();
    if text.is_empty() {
        return Ok("(page had no readable text)".to_string());
    }
    Ok(truncate_page(text))
}

/// Fetches the page directly and strips the markup locally.
async fn direct_fetch(client: &reqwest::Client, url: &str) -> Result<String> {
    let response = client
        .get(url)
        .header(
            "user-agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Loom/0.1",
        )
        .send()
        .await
        .map_err(|e| Error::Http(format!("fetch failed: {e}")))?;

    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .map_err(|e| Error::Http(format!("fetch read failed: {e}")))?;
    if !(200..300).contains(&status) {
        return Err(Error::Provider(format!("page returned HTTP {status}")));
    }

    let text = html_to_text(&body);
    if text.is_empty() {
        return Ok("(page had no readable text)".to_string());
    }
    Ok(truncate_page(&text))
}

fn truncate_page(text: &str) -> String {
    let mut text = text.trim().to_string();
    if text.len() > MAX_PAGE_CHARS {
        text.truncate(floor_char_boundary(&text, MAX_PAGE_CHARS));
        text.push_str("\n… [truncated]");
    }
    text
}

fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let cut = text
        .char_indices()
        .nth(limit)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    format!("{}…", &text[..cut])
}

fn floor_char_boundary(text: &str, index: usize) -> usize {
    let mut cut = index.min(text.len());
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    cut
}

/// Extracts the `result__a` links and snippets from DuckDuckGo's HTML page.
pub fn parse_ddg(html: &str, limit: usize) -> Vec<String> {
    let mut results = Vec::new();
    for chunk in html.split("result__body").skip(1) {
        if results.len() >= limit {
            break;
        }

        let title = extract_content(chunk, "result__a", "</a>").unwrap_or_default();
        let href = extract_content(chunk, "result__url", "</a>").unwrap_or_default();
        let snippet = extract_content(chunk, "result__snippet", "</div>").unwrap_or_default();

        let title = entity_decode(&strip_tags(&title));
        let url = entity_decode(&strip_tags(&href)).trim().to_string();
        let snippet = entity_decode(&strip_tags(&snippet));

        if title.is_empty() && url.is_empty() {
            continue;
        }
        results.push(format!(
            "{}. {}\n{}\n{}",
            results.len() + 1,
            title.trim(),
            url,
            snippet.trim()
        ));
    }
    results
}

/// Finds the element with `class="<marker>"` (or that marker anywhere) and
/// returns its inner HTML up to the matching `closing` tag. Adequate for
/// DuckDuckGo's simple result markup.
fn extract_content(chunk: &str, marker: &str, closing: &str) -> Option<String> {
    let start = chunk.find(marker)?;
    let tail = &chunk[start..];
    let open_end = tail.find('>')?;
    let rest = &tail[open_end + 1..];
    let end = rest.find(closing).unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

pub fn strip_tags(html: &str) -> String {
    let mut out: Vec<u8> = Vec::with_capacity(html.len());
    let mut in_tag = false;
    let mut skip_until: Option<&'static str> = None;

    // Compared and sliced as bytes: byte offsets into a `&str` are not valid
    // slice indices (a panic on multi-byte characters), bytes always are.
    let lower = html.to_ascii_lowercase();
    let source = html.as_bytes();
    let bytes = lower.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if let Some(closing) = skip_until {
            if bytes[index..].starts_with(closing.as_bytes()) {
                index += closing.len();
                skip_until = None;
                in_tag = false;
            } else {
                index += 1;
            }
            continue;
        }

        match bytes[index] {
            b'<' => {
                let mut skipped = false;
                for (open, close) in [
                    ("<script", "</script>"),
                    ("<style", "</style>"),
                    ("<noscript", "</noscript>"),
                    ("<head", "</head>"),
                ] {
                    if bytes[index..].starts_with(open.as_bytes()) {
                        skip_until = Some(close);
                        index += open.len();
                        skipped = true;
                        break;
                    }
                }
                if skipped {
                    continue;
                }
                in_tag = true;
                index += 1;
            }
            b'>' => {
                in_tag = false;
                index += 1;
            }
            _ if !in_tag => {
                out.push(source[index]);
                index += 1;
            }
            _ => index += 1,
        }
    }

    entity_decode(&String::from_utf8_lossy(&out))
}

pub fn html_to_text(html: &str) -> String {
    let text = strip_tags(html);
    let mut lines: Vec<&str> = Vec::new();
    for raw in text.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        lines.push(trimmed);
    }
    lines.join("\n")
}

pub fn entity_decode(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ")
        .replace("&#x27;", "'")
        .replace("&hellip;", "…")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_tags_removes_scripts_and_tags() {
        let html = "<html><head><title>x</title></head><body><h1>Hello</h1><script>bad()</script><p>World &amp; more</p></body></html>";
        let text = strip_tags(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("World & more"));
        assert!(!text.contains("bad()"));
        assert!(!text.contains("title"));
    }

    #[test]
    fn html_to_text_collapses_blank_lines() {
        let text = html_to_text("<p>a</p>\n\n\n<p>b</p>");
        assert_eq!(text, "a\nb");
    }

    #[test]
    fn duckduckgo_results_parse() {
        let html = r#"
            <div class="result__body">
              <a class="result__a" href="https://example.com">Example &amp; Co</a>
              <a class="result__url">example.com</a>
              <div class="result__snippet">A snippet about <b>stuff</b>.</div>
            </div>
            <div class="result__body">
              <a class="result__a">Second</a>
              <a class="result__url">second.com</a>
              <div class="result__snippet">More.</div>
            </div>
        "#;
        let results = parse_ddg(html, 5);
        assert_eq!(results.len(), 2);
        assert!(results[0].contains("Example & Co"));
        assert!(results[0].contains("A snippet about stuff."));
        assert!(results[1].starts_with("2. Second"));
    }

    #[test]
    fn result_limit_is_respected() {
        let mut html = String::new();
        for index in 0..5 {
            html.push_str(&format!(
                "<div class=\"result__body\"><a class=\"result__a\">T{index}</a><a class=\"result__url\">u{index}</a></div>"
            ));
        }
        assert_eq!(parse_ddg(&html, 2).len(), 2);
    }

    #[test]
    fn entities_are_decoded() {
        assert_eq!(entity_decode("a &amp; b &lt;c&gt;"), "a & b <c>");
    }

    #[test]
    fn jina_results_parse_into_the_ddg_shape() {
        let body: Value = serde_json::from_str(
            r#"{
                "code": 200,
                "status": 20000,
                "data": [
                    { "title": "Rust", "url": "https://rust-lang.org", "description": "A language." },
                    { "title": "Tauri", "url": "https://tauri.app", "content": "Apps, smaller." }
                ]
            }"#,
        )
        .unwrap();

        let results = format_jina_results(&body, 5);
        assert_eq!(results.len(), 2);
        assert!(results[0].starts_with("1. Rust\nhttps://rust-lang.org\nA language."));
        // `content` stands in when there is no description.
        assert!(results[1].contains("Apps, smaller."));
    }

    #[test]
    fn jina_results_handle_a_single_object_and_garbage() {
        let single: Value =
            serde_json::from_str(r#"{ "data": { "title": "One", "url": "https://one.example" } }"#)
                .unwrap();
        assert_eq!(format_jina_results(&single, 5).len(), 1);

        let empty: Value = serde_json::from_str(r#"{ "data": [] }"#).unwrap();
        assert!(format_jina_results(&empty, 5).is_empty());

        // Entries with neither a title nor a URL are dropped.
        let nameless: Value =
            serde_json::from_str(r#"{ "data": [{ "description": "orphan" }] }"#).unwrap();
        assert!(format_jina_results(&nameless, 5).is_empty());
    }

    #[test]
    fn jina_results_respect_the_limit() {
        let mut data = Vec::new();
        for index in 0..8 {
            data.push(json!({ "title": format!("T{index}"), "url": format!("https://t{index}") }));
        }
        let body = json!({ "data": data });
        assert_eq!(format_jina_results(&body, 3).len(), 3);
    }

    #[test]
    fn long_snippets_are_truncated_on_a_char_boundary() {
        let snippet = "漢".repeat(1_000);
        let body =
            json!({ "data": [{ "title": "T", "url": "https://t", "description": snippet }] });
        let results = format_jina_results(&body, 1);
        assert!(results[0].ends_with('…'));
        assert!(results[0].chars().count() < 1_100);
    }

    /// Regression: byte offsets into a &str are not slice indices, and a wiki
    /// page with multi-byte characters used to panic the tool task.
    #[test]
    fn multi_byte_characters_do_not_panic() {
        let html = "<p>Ünïcodé — em dash, ellipsis …, CJK 漢字, emoji 🌸 inside markup</p>\
                    <script>var x = \"漢\";</script><p>tail</p>";
        let text = strip_tags(html);
        assert!(text.contains("漢字"));
        assert!(text.contains("tail"));
        assert!(!text.contains("var x"));

        // A long page of multi-byte text, as produced by a real fetch.
        let long = "漢字とひらがなとカタカナと絵文字🌸".repeat(2_000);
        let wrapped = format!("<div>{long}</div>");
        let text = html_to_text(&wrapped);
        assert!(!text.is_empty());
        assert!(text.len() < wrapped.len());
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        let text = "漢".repeat(10_000);
        let html = format!("<p>{text}</p>");
        let fetched = html_to_text(&html);
        assert!(fetched.chars().count() <= 20_000);
    }
}
