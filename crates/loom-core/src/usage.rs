//! Provider usage and subscription limits.
//!
//! A small registry of vendors that expose a usage/quota endpoint for the same
//! API key Loom already stores. Everything is derived from the provider's base
//! URL, so a custom provider pointing at the same gateway works unchanged:
//!
//! | Vendor       | Endpoint                                            | Shape            |
//! | ---          | ---                                                 | ---              |
//! | OpenCode Go  | `{base}/usage`                                      | percent windows  |
//! | OpenRouter   | `https://openrouter.ai/api/v1/key`                  | credits + limits |
//! | DeepSeek     | `https://api.deepseek.com/user/balance`             | balance          |
//! | Z.ai / GLM   | `https://api.z.ai/api/monitor/usage/quota/limit`    | percent windows  |
//!
//! Parsers are pure (fixture-tested) and deliberately tolerant: a changed
//! field or a missing window is skipped rather than reported as zero, so a
//! vendor tweak never invents usage the account does not have.

use std::time::Duration;

use serde::Serialize;

use crate::provider::ProviderConfig;
use crate::{Error, Result};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// One line in the usage card. Percent metrics render as a bar; amount metrics
/// (`used`/`limit`/`remaining`) render as a balance.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageMetric {
    pub id: String,
    pub label: String,
    /// 0-100 when the vendor reports a window percentage.
    pub percent: Option<f64>,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub remaining: Option<f64>,
    /// `percent`, `usd`, `cny`, `tokens`, `calls`.
    pub unit: String,
    /// ISO-8601 reset stamp when the vendor sends one.
    pub resets_at: Option<String>,
    /// Epoch milliseconds reset stamp (Z.ai sends milliseconds).
    pub resets_at_ms: Option<i64>,
    /// Vendor status flag, e.g. `ok` / `unavailable`.
    pub status: Option<String>,
    /// Auxiliary text (e.g. DeepSeek's granted/topped-up split).
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsage {
    pub provider_id: String,
    /// Human label for where the numbers come from, e.g. `OpenCode Go`.
    pub source: String,
    pub fetched_at: i64,
    pub metrics: Vec<UsageMetric>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    OpencodeGo,
    OpenRouter,
    DeepSeek,
    Zai,
}

pub struct Source {
    pub kind: SourceKind,
    pub label: &'static str,
    pub url: String,
}

/// The endpoint a provider's key can be checked against, if the vendor has one.
/// Matched on the base URL host so user-made providers (and renamed presets)
/// still light up.
pub fn source_for(provider: &ProviderConfig) -> Option<Source> {
    let parsed = reqwest::Url::parse(provider.base_url.trim()).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let path = parsed.path().trim_end_matches('/');

    match host.as_str() {
        "opencode.ai" if path.starts_with("/zen/go") => Some(Source {
            kind: SourceKind::OpencodeGo,
            label: "OpenCode Go",
            url: format!("{}/usage", provider.normalized_base_url()),
        }),
        "openrouter.ai" => Some(Source {
            kind: SourceKind::OpenRouter,
            label: "OpenRouter",
            url: "https://openrouter.ai/api/v1/key".into(),
        }),
        "api.deepseek.com" => Some(Source {
            kind: SourceKind::DeepSeek,
            label: "DeepSeek",
            url: "https://api.deepseek.com/user/balance".into(),
        }),
        "api.z.ai" => Some(Source {
            kind: SourceKind::Zai,
            label: "Z.ai",
            url: "https://api.z.ai/api/monitor/usage/quota/limit".into(),
        }),
        "open.bigmodel.cn" => Some(Source {
            kind: SourceKind::Zai,
            label: "Z.ai",
            url: "https://open.bigmodel.cn/api/monitor/usage/quota/limit".into(),
        }),
        _ => None,
    }
}

/// Fetches the provider's usage. `api_key` is the same key used for chat.
pub async fn fetch(
    client: &reqwest::Client,
    provider_id: &str,
    provider: &ProviderConfig,
    api_key: Option<&str>,
) -> Result<ProviderUsage> {
    let source = source_for(provider)
        .ok_or_else(|| Error::Other(format!("{} has no usage endpoint", provider.name.trim())))?;
    let key = api_key
        .ok_or_else(|| Error::Other(format!("Add an API key to see {} usage", source.label)))?;

    let response = client
        .get(&source.url)
        .timeout(REQUEST_TIMEOUT)
        .header("accept", "application/json")
        .header("authorization", format!("Bearer {key}"))
        .send()
        .await
        .map_err(|error| Error::Http(format!("{} usage request failed: {error}", source.label)))?;

    let status = response.status();
    let body = response.text().await.map_err(|error| {
        Error::Http(format!(
            "{} usage response was unreadable: {error}",
            source.label
        ))
    })?;

    if !status.is_success() {
        return Err(status_error(source.label, status.as_u16(), &body));
    }

    let metrics = parse(source.kind, &body)?;
    Ok(ProviderUsage {
        provider_id: provider_id.to_string(),
        source: source.label.to_string(),
        fetched_at: crate::db::now_ms(),
        metrics,
    })
}

fn status_error(label: &str, status: u16, body: &str) -> Error {
    if status == 401 {
        return Error::Other(format!("{label}: the stored API key was rejected (401)."));
    }
    if status == 403 {
        // Entitlement answers are authoritative: the key is valid, the account
        // just has no subscription. A bare 403 could also be a proxy, so only
        // claim "no subscription" when the body says so.
        let entitlement = body.contains("Entitlement") || body.contains("subscription");
        return Error::Other(if entitlement {
            format!("{label}: no active subscription on this API key.")
        } else {
            format!("{label}: usage is not available for this key (403).")
        });
    }
    if status == 429 {
        return Error::Other(format!("{label}: rate limited, try again in a moment."));
    }
    Error::Other(format!("{label}: usage request failed (HTTP {status})."))
}

fn parse(kind: SourceKind, body: &str) -> Result<Vec<UsageMetric>> {
    match kind {
        SourceKind::OpencodeGo => parse_opencode_go(body),
        SourceKind::OpenRouter => parse_openrouter(body),
        SourceKind::DeepSeek => parse_deepseek(body),
        SourceKind::Zai => parse_zai(body),
    }
}

fn json(body: &str, label: &str) -> Result<serde_json::Value> {
    serde_json::from_str(body)
        .map_err(|error| Error::Other(format!("{label} usage response was not JSON: {error}")))
}

fn number(value: Option<&serde_json::Value>) -> Option<f64> {
    value.and_then(serde_json::Value::as_f64)
}

fn percent_of(used: f64, limit: f64) -> Option<f64> {
    if limit > 0.0 {
        Some((used / limit * 100.0).clamp(0.0, 100.0))
    } else {
        None
    }
}

// ---------------------------------------------------------------- OpenCode Go

/// `{ "usage": { "rolling": { "status", "percent", "resetsAt" }, ... } }`
fn parse_opencode_go(body: &str) -> Result<Vec<UsageMetric>> {
    let value = json(body, "OpenCode Go")?;
    let usage = value
        .get("usage")
        .ok_or_else(|| Error::Other("OpenCode Go usage response had no usage object".into()))?;

    let mut metrics = Vec::new();
    for (id, label) in [
        ("rolling", "5 hour window"),
        ("weekly", "Weekly window"),
        ("monthly", "Monthly window"),
    ] {
        let Some(window) = usage.get(id) else {
            continue;
        };
        if window.is_null() {
            continue;
        }
        let Some(percent) = number(window.get("percent")) else {
            continue;
        };
        metrics.push(UsageMetric {
            id: id.to_string(),
            label: label.to_string(),
            percent: Some(percent.clamp(0.0, 100.0)),
            used: None,
            limit: None,
            remaining: None,
            unit: "percent".into(),
            resets_at: window
                .get("resetsAt")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            resets_at_ms: None,
            status: window
                .get("status")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            detail: None,
        });
    }

    if metrics.is_empty() {
        return Err(Error::Other(
            "OpenCode Go usage response carried no windows".into(),
        ));
    }
    Ok(metrics)
}

// ----------------------------------------------------------------- OpenRouter

/// `{ "data": { "usage", "usage_weekly", "limit", "limit_remaining", ... } }`
fn parse_openrouter(body: &str) -> Result<Vec<UsageMetric>> {
    let value = json(body, "OpenRouter")?;
    let data = value
        .get("data")
        .ok_or_else(|| Error::Other("OpenRouter usage response had no data object".into()))?;

    let total = number(data.get("usage"));
    let monthly = number(data.get("usage_monthly"));
    let weekly = number(data.get("usage_weekly"));
    if total.is_none() && monthly.is_none() && weekly.is_none() {
        return Err(Error::Other(
            "OpenRouter usage response carried no usage figures".into(),
        ));
    }

    let mut metrics = Vec::new();
    if let Some(limit) = number(data.get("limit")) {
        let remaining = number(data.get("limit_remaining"));
        let used = remaining.map(|left| (limit - left).max(0.0));
        metrics.push(UsageMetric {
            id: "key-limit".into(),
            label: "Key limit".into(),
            percent: used.and_then(|spent| percent_of(spent, limit)),
            used,
            limit: Some(limit),
            remaining,
            unit: "usd".into(),
            resets_at: None,
            resets_at_ms: None,
            status: None,
            detail: data
                .get("limit_reset")
                .and_then(serde_json::Value::as_str)
                .map(|reset| format!("resets {reset}")),
        });
    }

    let amount = |id: &str, label: &str, value: Option<f64>, out: &mut Vec<UsageMetric>| {
        if let Some(used) = value {
            out.push(UsageMetric {
                id: id.to_string(),
                label: label.to_string(),
                percent: None,
                used: Some(used),
                limit: None,
                remaining: None,
                unit: "usd".into(),
                resets_at: None,
                resets_at_ms: None,
                status: None,
                detail: None,
            });
        }
    };
    amount("month", "This month", monthly, &mut metrics);
    amount("week", "This week", weekly, &mut metrics);
    amount("total", "All time", total, &mut metrics);

    Ok(metrics)
}

// ------------------------------------------------------------------- DeepSeek

/// `{ "is_available": true, "balance_infos": [ { "currency", "total_balance",
/// "granted_balance", "topped_up_balance" } ] }` — amounts are decimal strings.
fn parse_deepseek(body: &str) -> Result<Vec<UsageMetric>> {
    let value = json(body, "DeepSeek")?;
    let available = value
        .get("is_available")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    let infos = value
        .get("balance_infos")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut metrics = Vec::new();
    for info in &infos {
        let currency = info
            .get("currency")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("USD")
            .to_uppercase();
        let Some(total) = info
            .get("total_balance")
            .and_then(serde_json::Value::as_str)
            .and_then(|raw| raw.parse::<f64>().ok())
        else {
            continue;
        };
        let granted = info
            .get("granted_balance")
            .and_then(serde_json::Value::as_str)
            .and_then(|raw| raw.parse::<f64>().ok());
        let topped = info
            .get("topped_up_balance")
            .and_then(serde_json::Value::as_str)
            .and_then(|raw| raw.parse::<f64>().ok());
        let detail = match (granted, topped) {
            (Some(granted), Some(topped)) => Some(format!(
                "granted {} · topped up {}",
                money(granted, &currency),
                money(topped, &currency)
            )),
            _ => None,
        };
        metrics.push(UsageMetric {
            id: format!("balance-{}", currency.to_lowercase()),
            label: format!("{currency} balance"),
            percent: None,
            used: None,
            limit: None,
            remaining: Some(total),
            unit: currency.to_lowercase(),
            resets_at: None,
            resets_at_ms: None,
            status: Some(if available { "ok" } else { "unavailable" }.into()),
            detail,
        });
    }

    if metrics.is_empty() {
        return Err(Error::Other(
            "DeepSeek balance response carried no usable amounts".into(),
        ));
    }
    Ok(metrics)
}

fn money(amount: f64, currency: &str) -> String {
    let symbol = match currency {
        "USD" => "$",
        "CNY" => "¥",
        _ => "",
    };
    format!("{symbol}{amount:.2}")
}

// ----------------------------------------------------------------------- Z.ai

/// `{ "success": true, "data": { "limits": [ { "type": "TOKENS_LIMIT",
/// "percentage", "currentValue", "usage", "nextResetTime" } ] } }` — an
/// internal coding-plan endpoint; pay-as-you-go keys report no limits.
fn parse_zai(body: &str) -> Result<Vec<UsageMetric>> {
    let value = json(body, "Z.ai")?;
    if value.get("success").and_then(serde_json::Value::as_bool) == Some(false) {
        let message = value
            .get("message")
            .or_else(|| value.get("msg"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("request rejected");
        return Err(Error::Other(format!(
            "Z.ai usage request was rejected: {message}"
        )));
    }

    let limits = value
        .get("data")
        .and_then(|data| data.get("limits"))
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut metrics = Vec::new();
    for limit in &limits {
        let kind = limit
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        let percent = number(limit.get("percentage"));
        let used = number(limit.get("currentValue"));
        let total = number(limit.get("usage"));
        let remaining = number(limit.get("remaining"));
        let resets_at_ms = limit
            .get("nextResetTime")
            .and_then(serde_json::Value::as_i64);

        let (id, label, unit) = match kind {
            "TOKENS_LIMIT" => {
                let window = match (
                    limit.get("unit").and_then(serde_json::Value::as_i64),
                    limit.get("number").and_then(serde_json::Value::as_i64),
                ) {
                    (Some(3), _) => "5 hour window",
                    (Some(6), _) => "Weekly window",
                    (_, Some(1)) => "Monthly window",
                    _ => "Token window",
                };
                ("tokens".to_string(), window.to_string(), "tokens")
            }
            "TIME_LIMIT" => (
                "tools".to_string(),
                "Monthly tool calls".to_string(),
                "calls",
            ),
            _ => continue,
        };

        metrics.push(UsageMetric {
            id,
            label,
            percent: percent.map(|value| value.clamp(0.0, 100.0)),
            used,
            limit: total,
            remaining,
            unit: unit.into(),
            resets_at: None,
            resets_at_ms,
            status: None,
            detail: None,
        });
    }

    if metrics.is_empty() {
        return Err(Error::Other(
            "Z.ai reported no coding-plan quota (pay-as-you-go keys have none).".into(),
        ));
    }
    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ProviderConfig, ProviderKind};

    fn provider(base_url: &str) -> ProviderConfig {
        ProviderConfig {
            name: "Test".into(),
            kind: ProviderKind::OpenaiCompatible,
            base_url: base_url.into(),
            ..Default::default()
        }
    }

    #[test]
    fn opencode_go_windows_parse_into_percent_metrics() {
        let body = r#"{
            "usage": {
                "rolling": { "status": "ok", "percent": 4, "resetsAt": "2026-08-13T16:27:38.287Z" },
                "weekly":  { "status": "ok", "percent": 3, "resetsAt": "2026-08-17T00:00:00.287Z" },
                "monthly": { "status": "ok", "percent": 1, "resetsAt": "2026-09-13T06:06:01.287Z" }
            }
        }"#;
        let metrics = parse_opencode_go(body).unwrap();
        assert_eq!(metrics.len(), 3);
        assert_eq!(metrics[0].id, "rolling");
        assert_eq!(metrics[0].percent, Some(4.0));
        assert_eq!(metrics[2].label, "Monthly window");
        assert_eq!(
            metrics[1].resets_at.as_deref(),
            Some("2026-08-17T00:00:00.287Z")
        );
    }

    #[test]
    fn opencode_go_skips_missing_windows_and_empty_bodies() {
        let partial = r#"{"usage":{"rolling":{"percent":50}}}"#;
        let metrics = parse_opencode_go(partial).unwrap();
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].percent, Some(50.0));

        assert!(parse_opencode_go(r#"{"usage":{}}"#).is_err());
        assert!(parse_opencode_go("not json").is_err());
    }

    #[test]
    fn openrouter_credits_become_a_limit_metric() {
        let body = r#"{
            "data": {
                "usage": 25.5, "usage_daily": 1.0, "usage_weekly": 4.0,
                "usage_monthly": 20.0, "limit": 100.0, "limit_remaining": 74.5,
                "limit_reset": "monthly"
            }
        }"#;
        let metrics = parse_openrouter(body).unwrap();
        let limit = &metrics[0];
        assert_eq!(limit.id, "key-limit");
        assert_eq!(limit.used, Some(25.5));
        assert_eq!(limit.remaining, Some(74.5));
        assert_eq!(limit.percent, Some(25.5));
        assert_eq!(limit.detail.as_deref(), Some("resets monthly"));

        // No key limit: plain spend figures, no fabricated percentage.
        let body =
            r#"{"data":{"usage":25.5,"usage_weekly":4.0,"usage_monthly":20.0,"limit":null}}"#;
        let metrics = parse_openrouter(body).unwrap();
        assert!(metrics.iter().all(|metric| metric.percent.is_none()));
        assert_eq!(metrics.len(), 3);
    }

    #[test]
    fn deepseek_balance_parses_string_amounts() {
        let body = r#"{
            "is_available": true,
            "balance_infos": [
                { "currency": "CNY", "total_balance": "10.00", "granted_balance": "10.00", "topped_up_balance": "0.00" },
                { "currency": "USD", "total_balance": "1.50", "granted_balance": "1.50", "topped_up_balance": "0.00" }
            ]
        }"#;
        let metrics = parse_deepseek(body).unwrap();
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics[1].unit, "usd");
        assert_eq!(metrics[1].remaining, Some(1.5));
        assert_eq!(metrics[1].status.as_deref(), Some("ok"));

        // Amounts that stop parsing are skipped; nothing usable is an error.
        let broken = r#"{"is_available":true,"balance_infos":[{"currency":"USD","total_balance":"n/a","granted_balance":"0","topped_up_balance":"0"}]}"#;
        assert!(parse_deepseek(broken).is_err());
    }

    #[test]
    fn zai_limits_parse_windows_and_tool_calls() {
        let body = r#"{
            "code": 200, "success": true,
            "data": { "limits": [
                { "type": "TOKENS_LIMIT", "unit": 3, "number": 5, "usage": 800000000,
                  "currentValue": 127694464, "remaining": 672305536, "percentage": 15,
                  "nextResetTime": 1770648402389 },
                { "type": "TIME_LIMIT", "unit": 5, "number": 1, "usage": 4000,
                  "currentValue": 1828, "remaining": 2172, "percentage": 45 }
            ] }
        }"#;
        let metrics = parse_zai(body).unwrap();
        assert_eq!(metrics[0].label, "5 hour window");
        assert_eq!(metrics[0].percent, Some(15.0));
        assert_eq!(metrics[0].resets_at_ms, Some(1770648402389));
        assert_eq!(metrics[1].id, "tools");
        assert_eq!(metrics[1].unit, "calls");
    }

    #[test]
    fn zai_rejections_and_pay_as_you_go_keys_surface_clearly() {
        let rejected = r#"{"success":false,"message":"invalid api key"}"#;
        assert!(parse_zai(rejected)
            .unwrap_err()
            .to_string()
            .contains("invalid api key"));

        let empty = r#"{"success":true,"data":{"limits":[]}}"#;
        let error = parse_zai(empty).unwrap_err().to_string();
        assert!(error.contains("pay-as-you-go"), "{error}");
    }

    #[test]
    fn sources_match_on_host_not_provider_id() {
        assert_eq!(
            source_for(&provider("https://opencode.ai/zen/go/v1"))
                .unwrap()
                .kind,
            SourceKind::OpencodeGo
        );
        // The Zen (pay-as-you-go) gateway has no usage endpoint.
        assert!(source_for(&provider("https://opencode.ai/zen/v1")).is_none());
        assert_eq!(
            source_for(&provider("https://openrouter.ai/api/v1"))
                .unwrap()
                .kind,
            SourceKind::OpenRouter
        );
        assert_eq!(
            source_for(&provider("https://api.deepseek.com/v1"))
                .unwrap()
                .kind,
            SourceKind::DeepSeek
        );
        assert_eq!(
            source_for(&provider("https://api.z.ai/api/paas/v4"))
                .unwrap()
                .kind,
            SourceKind::Zai
        );
        assert_eq!(
            source_for(&provider("https://open.bigmodel.cn/api/paas/v4"))
                .unwrap()
                .kind,
            SourceKind::Zai
        );
        assert!(source_for(&provider("https://api.openai.com/v1")).is_none());
        assert!(source_for(&provider("not a url")).is_none());
    }

    #[test]
    fn opencode_go_usage_url_keeps_the_gateway_path() {
        let source = source_for(&provider("https://opencode.ai/zen/go")).unwrap();
        assert_eq!(source.url, "https://opencode.ai/zen/go/v1/usage");
    }

    #[test]
    fn statuses_map_to_actionable_messages() {
        let entitlement = status_error(
            "OpenCode Go",
            403,
            r#"{"error":{"type":"EntitlementError"}}"#,
        );
        assert!(entitlement.to_string().contains("no active subscription"));

        let proxy = status_error("OpenCode Go", 403, "<html>denied</html>");
        assert!(proxy.to_string().contains("not available"));

        assert!(status_error("OpenRouter", 401, "")
            .to_string()
            .contains("rejected"));
        assert!(status_error("Z.ai", 429, "")
            .to_string()
            .contains("rate limited"));
    }
}
