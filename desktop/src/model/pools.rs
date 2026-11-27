//! Usage pools (PLAN.md, "Usage pools"): an agent program whose settings send it to a
//! proxy that spreads its turns over several accounts, such as CLIProxyAPI over several
//! Claude logins. The instance reads the pool (`server/pools.calx`) and gives the owner
//! its reading with the program; this only takes it in and sends the owner's link.

use serde_json::{json, Value};

use super::{s, Model};

/// The pool a program draws from: its kind, address and the name of the secret with
/// its management key; the newest reading, or why there's none.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pool {
    pub kind: String,
    pub address: String,
    pub secret: String,
    pub error: Option<String>,
    pub reading: Option<Reading>,
}

/// What the pool said when it was last read (Unix ms).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
    pub at: i64,
    pub accounts: Vec<Account>,
    pub total: Total,
}

/// One account in the pool: its name, how it stands, until when, its requests, and the
/// limits it reports, which differ between accounts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Account {
    pub name: String,
    pub state: State,
    pub why: String,
    pub until: i64,
    pub ok: i64,
    pub failed: i64,
    pub limits: Vec<Limit>,
}

/// How an account stands at the pool.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum State {
    #[default]
    Ready,
    /// A short wait.
    Cooling,
    /// Out of its quota.
    Over,
    SignedOut,
    Off,
    Error,
}

/// One window of an account's limits, such as "5 hours" or "Week · Fable": how much of it
/// is used, in percent, and when it resets (Unix ms, 0 when the pool didn't say).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Limit {
    pub window: String,
    pub label: String,
    pub used: i64,
    pub resets: i64,
}

/// The pool as a whole: its accounts, how many are ready, the requests, and each
/// window's used across the accounts that report it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Total {
    pub accounts: i64,
    pub ready: i64,
    pub ok: i64,
    pub failed: i64,
    pub limits: Vec<Limit>,
}

fn int(v: &Value, k: &str) -> i64 {
    v.get(k).and_then(Value::as_i64).unwrap_or(0)
}

fn limits(v: Option<&Value>) -> Vec<Limit> {
    v.and_then(Value::as_array)
        .map(|ls| {
            ls.iter()
                .map(|l| Limit {
                    window: s(l, "window"),
                    label: s(l, "label"),
                    used: int(l, "used").clamp(0, 100),
                    resets: int(l, "resets"),
                })
                .collect()
        })
        .unwrap_or_default()
}

impl Pool {
    /// A program's `pool`, as acp.list and pool.updated give it, or none.
    pub fn of(v: &Value) -> Option<Pool> {
        if !v.is_object() {
            return None;
        }
        let reading = v.get("reading").filter(|r| r.is_object()).map(|r| {
            let total = r.get("total").cloned().unwrap_or(Value::Null);
            Reading {
                at: int(r, "at"),
                accounts: r
                    .get("accounts")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().map(Account::of).collect())
                    .unwrap_or_default(),
                total: Total {
                    accounts: int(&total, "accounts"),
                    ready: int(&total, "ready"),
                    ok: int(&total, "ok"),
                    failed: int(&total, "failed"),
                    limits: limits(total.get("limits")),
                },
            }
        });
        Some(Pool {
            kind: s(v, "kind"),
            address: s(v, "address"),
            secret: s(v, "secret"),
            error: v
                .get("error")
                .and_then(Value::as_str)
                .filter(|e| !e.is_empty())
                .map(str::to_string),
            reading,
        })
    }

    /// The pool's kind as people know it.
    pub fn kind_name(&self) -> &str {
        match self.kind.as_str() {
            "cliproxyapi" => "CLIProxyAPI",
            other => other,
        }
    }
}

impl Account {
    fn of(v: &Value) -> Account {
        Account {
            name: s(v, "name"),
            state: match s(v, "state").as_str() {
                "cooling" => State::Cooling,
                "over" => State::Over,
                "signed_out" => State::SignedOut,
                "off" => State::Off,
                "error" => State::Error,
                _ => State::Ready,
            },
            why: s(v, "why"),
            until: int(v, "until"),
            ok: int(v, "ok"),
            failed: int(v, "failed"),
            limits: limits(v.get("limits")),
        }
    }
}

impl Account {
    /// How it stands, in a few words: "Ready", "Cooling down · 4 min", "Over its quota
    /// until 15:30" (a weekday past a day), or why it can't be used.
    pub fn words<Tz: chrono::TimeZone>(&self, now: chrono::DateTime<Tz>) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        let left = self.until - now.timestamp_millis();
        let when = || -> Option<String> {
            if left <= 0 {
                return None;
            }
            if left < 60 * 60 * 1000 {
                return Some(format!("{} min", (left + 59_999) / 60_000));
            }
            let at = now.timezone().timestamp_millis_opt(self.until).single()?;
            Some(if left < 24 * 60 * 60 * 1000 {
                at.format("%H:%M").to_string()
            } else {
                at.format("%a").to_string()
            })
        };
        match self.state {
            State::Ready => "Ready".into(),
            State::Cooling => match when() {
                Some(w) => format!("Cooling down · {w}"),
                None => "Cooling down".into(),
            },
            State::Over => match when() {
                Some(w) if left < 60 * 60 * 1000 => format!("Over its quota · {w}"),
                Some(w) => format!("Over its quota until {w}"),
                None => "Over its quota".into(),
            },
            State::SignedOut | State::Off | State::Error => self.why.clone(),
        }
    }
}

impl Model {
    /// A program's pool, as pool.updated says it is now.
    pub fn pool_updated(&mut self, f: &Value) {
        let name = s(f, "program");
        let pool = f.get("pool").and_then(Pool::of);
        match self.installed.iter_mut().find(|p| p.name == name) {
            Some(p) => p.pool = pool,
            None => self.programs_stale = true,
        }
    }

    /// Links `program` to its usage pool: CLIProxyAPI at `address` (its settings' base
    /// address when empty), with the management key in the secret named `secret`.
    pub fn link_pool(&mut self, program: &str, address: &str, secret: &str) -> Value {
        let id = self.id("app.program");
        json!({"t": "cmd", "id": id, "name": "acp.pool", "args": {"name": program, "kind": "cliproxyapi", "address": address.trim(), "secret": secret.trim()}})
    }

    /// Unlinks `program` from its pool.
    pub fn unlink_pool(&mut self, program: &str) -> Value {
        let id = self.id("app.program");
        json!({"t": "cmd", "id": id, "name": "acp.pool", "args": {"name": program, "kind": ""}})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reading_keeps_each_account_s_own_windows() {
        let p = Pool::of(&json!({
            "kind": "cliproxyapi", "address": "http://127.0.0.1:8317", "secret": "pool-key",
            "reading": {"at": 5, "accounts": [
                {"name": "a@x", "state": "ready", "ok": 3, "failed": 0,
                 "limits": [{"window": "5h", "label": "5 hours", "used": 40, "resets": 9}, {"window": "7d", "label": "Week", "used": 20}]},
                {"name": "b@x", "state": "over", "until": 77, "why": "Over its quota",
                 "limits": [{"window": "7d:fable", "label": "Week · fable", "used": 100}]}
            ], "total": {"accounts": 2, "ready": 1, "ok": 3, "failed": 0, "limits": [{"window": "7d", "label": "Week", "used": 20}]}}
        }))
        .unwrap();
        assert_eq!(p.kind_name(), "CLIProxyAPI");
        let r = p.reading.unwrap();
        assert_eq!(r.accounts.len(), 2);
        assert_eq!(r.accounts[0].limits.len(), 2);
        assert_eq!(r.accounts[1].limits[0].label, "Week · fable");
        assert_eq!(r.accounts[1].state, State::Over);
        assert_eq!(r.total.ready, 1);
        assert!(Pool::of(&Value::Null).is_none());
        let refused = Pool::of(
            &json!({"kind": "cliproxyapi", "error": "The pool didn't take the management key."}),
        )
        .unwrap();
        assert!(refused.reading.is_none() && refused.error.is_some());
    }
}
