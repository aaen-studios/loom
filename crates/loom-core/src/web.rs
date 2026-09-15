//! Web tools: search and page fetching.
//!
//! Search uses DuckDuckGo's HTML endpoint (no API key required). Pages are
//! converted to plain text with a small tag stripper so the model gets
//! readable content instead of markup.

use serde_json::{json, Value};

use crate::{Error, Result};

pub const SEARCH_TOOL: &str = "web_search";
pub const FETCH_TOOL: &str = "fetch_url";

pub fn tool_specs() -> Vec<(String, String, Value)> {
    vec![
        (
            SEARCH_TOOL.to_string(),
            "Search the web with DuckDuckGo and return the top results (title, URL, snippet).".to_string(),
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
            "Fetch a web page and return its readable text (HTML stripped, truncated).".to_string(),
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

pub async fn search(client: &reqwest::Client, query: &str, max_results: usize) -> Result<String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(Error::other("search query is empty"));
    }
    let limit = max_results.clamp(1, 10);

    let response = client
        .get("https://html.duckduckgo.com/html/")
        .query(&[("q", trimmed)])
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
        return Ok(format!("No results for \"{trimmed}\"."));
    }
    Ok(results.join("\n\n"))
}

pub async fn fetch(client: &reqwest::Client, url: &str) -> Result<String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(Error::other("url must start with http:// or https://"));
    }

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
    let mut text = text.trim().to_string();
    if text.len() > MAX_PAGE_CHARS {
        text.truncate(floor_char_boundary(&text, MAX_PAGE_CHARS));
        text.push_str("\n… [truncated]");
    }
    if text.is_empty() {
        return Ok("(page had no readable text)".to_string());
    }
    Ok(text)
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

    let lower = html.to_ascii_lowercase();
    let source = html.as_bytes();
    let bytes = lower.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if let Some(closing) = skip_until {
            if lower[index..].starts_with(closing) {
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
                    if lower[index..].starts_with(open) {
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
}
