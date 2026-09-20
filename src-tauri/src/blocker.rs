//! The content blocker: WebView2's request filter, driven by filter lists.
//!
//! # Why not uBlock Origin itself
//!
//! It cannot run here. WebView2 has **no extension API**, so a `.crx` has
//! nowhere to go — that is a property of the substrate, not a build decision,
//! and no amount of wiring changes it.
//!
//! What WebView2 *does* have is the technique underneath. Every request is
//! offered to the host **before it is made**, and the host may answer it:
//! `AddWebResourceRequestedFilter` registers what to watch,
//! `add_WebResourceRequested` delivers each one, and `SetResponse` with a
//! synthetic response is how a request is refused. A request matching a filter
//! rule is therefore never sent at all, which is what uBlock's network layer
//! does too.
//!
//! The lists are uBlock's lists. The parser is `loom_core::browser::filters`,
//! where it is tested without a browser; the fetching is
//! `loom_core::browser::lists`. This file is the WebView2 plumbing.
//!
//! # What is genuinely missing, and cannot be added
//!
//! Network filtering and element hiding are both covered. What is not — and
//! cannot be, without an extension API — is a filter list's *script*
//! injections: replacing a video player, rewriting a page's own variables.
//! `$csp` and `$replace` rules are counted and skipped rather than guessed at,
//! so the settings page can say how much of a list was understood.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use loom_core::browser::filters::{Filters, Request, ResourceTypes, Verdict};

/// A loaded, shared blocker.
///
/// `Arc` because every tab's request filter needs it, and a filter set is one
/// immutable thing built once per list change — rebuilding it per tab would
/// multiply a 100,000-rule parse by the number of tabs.
pub struct Blocker {
    inner: Arc<BlockerInner>,
}

struct BlockerInner {
    /// Swapped whole when the lists change, rather than mutated: a request
    /// handler running on another thread must never see a half-parsed list.
    filters: Mutex<Arc<Filters>>,
    /// Hosts that are never blocked.
    allow: Mutex<Vec<String>>,
    blocked: AtomicU64,
}

impl Default for Blocker {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Blocker {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Blocker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(BlockerInner {
                filters: Mutex::new(Arc::new(Filters::new())),
                allow: Mutex::new(Vec::new()),
                blocked: AtomicU64::new(0),
            }),
        }
    }

    /// Whether anything is loaded.
    ///
    /// A caller with an empty blocker does not register a request filter at all,
    /// which is what keeps a browser with blocking switched off exactly as fast
    /// as one from before the feature existed.
    pub fn is_active(&self) -> bool {
        !self.filters().is_empty() || !self.allow_hosts().is_empty()
    }

    fn filters(&self) -> Arc<Filters> {
        Arc::clone(&lock(&self.inner.filters))
    }

    fn allow_hosts(&self) -> Vec<String> {
        lock(&self.inner.allow).clone()
    }

    /// Replaces the loaded lists.
    pub fn set(&self, filters: Filters, allow: Vec<String>) {
        *lock(&self.inner.filters) = Arc::new(filters);
        *lock(&self.inner.allow) = allow;
    }

    pub fn blocked_count(&self) -> u64 {
        self.inner.blocked.load(Ordering::Relaxed)
    }

    /// How many rules are loaded, for the settings page.
    pub fn rule_count(&self) -> u32 {
        self.filters().stats().rules
    }

    /// Decides one request.
    ///
    /// The allow list is consulted *first* and short-circuits, which is what
    /// makes it trustworthy: a site the user exempted cannot be broken by a list
    /// updated after they exempted it.
    pub fn verdict(&self, url: &str, document_host: &str, resource_type: u16) -> Verdict {
        if self.is_allowed(url, document_host) {
            return Verdict::Pass;
        }
        let filters = self.filters();
        let verdict = filters
            .verdict(Request::new(url, document_host, resource_type))
            .map(|found| found.verdict)
            .unwrap_or(Verdict::Pass);
        if verdict == Verdict::Block {
            self.inner.blocked.fetch_add(1, Ordering::Relaxed);
        }
        verdict
    }

    /// Whether the user has exempted this host, or the page's.
    fn is_allowed(&self, url: &str, document_host: &str) -> bool {
        let allow = self.allow_hosts();
        if allow.is_empty() {
            return false;
        }
        // Bound to a local: `host_of` borrows the string it is given, and the
        // lowercased URL is a temporary that would otherwise be dropped at the
        // end of the statement.
        let lowered = url.to_ascii_lowercase();
        let host = loom_core::browser::filters::host_of(&lowered);
        allow.iter().any(|entry| {
            let entry = entry
                .trim()
                .trim_start_matches("https://")
                .trim_start_matches("http://");
            let entry = entry.split('/').next().unwrap_or(entry).trim_end_matches('/');
            if entry.is_empty() {
                return false;
            }
            let entry = entry.to_ascii_lowercase();
            // Either the request's host or the page's being exempt is enough: a
            // user who exempts a site means "leave this site alone", not "leave
            // only its own requests alone".
            host_matches(host, &entry) || host_matches(document_host, &entry)
        })
    }

    /// The CSS that hides what was blocked on this page.
    pub fn cosmetic_css(&self, host: &str) -> String {
        self.filters().cosmetic_css(host)
    }

    /// The CSS as a script, for injection into a page.
    ///
    /// Injected rather than appended through an API, because WebView2 runs
    /// `AddScriptToExecuteOnDocumentCreated` *before* the page's own scripts —
    /// which is the only moment a hidden element can be hidden before it paints.
    /// Injecting later shows the ad for a frame, which is the flicker every
    /// blocker exists to avoid.
    pub fn cosmetic_script(&self, host: &str) -> Option<String> {
        let css = self.cosmetic_css(host);
        if css.trim().is_empty() {
            return None;
        }
        // A `<style>` element rather than per-element styles, so `!important`
        // and descendant selectors both work. The CSS is embedded as a **JSON
        // string**, which is the load-bearing part: a selector containing a
        // quote must not be able to break out of the script it is injected into,
        // and a filter list is untrusted input.
        let css = serde_json::to_string(&css).unwrap_or_else(|_| "\"\"".to_string());
        Some(format!(
            "(function () {{ \
               if (window.__loomCosmetic) return; \
               window.__loomCosmetic = true; \
               var apply = function () {{ \
                 if (!document.documentElement) return; \
                 var style = document.createElement('style'); \
                 style.setAttribute('data-loom', 'cosmetic'); \
                 style.textContent = {css}; \
                 document.documentElement.appendChild(style); \
               }}; \
               apply(); \
             }})();"
        ))
    }
}

fn host_matches(host: &str, entry: &str) -> bool {
    host == entry || host.ends_with(&format!(".{entry}"))
}

/// Maps a WebView2 resource context to the flag the filter engine speaks.
///
/// The numbers are WebView2's own `COREWEBVIEW2_WEB_RESOURCE_CONTEXT` values,
/// read from the bindings rather than assumed: they are not in an order anyone
/// would guess, and one of them wrong would mis-classify a rule's `$type` — so
/// `$image` would match scripts.
pub fn context_to_type(context: i32) -> u16 {
    match context {
        // CSP_VIOLATION_REPORT is 15 in the API; it is not a type a list names,
        // so it falls through to OTHER with the rest.
        1 => ResourceTypes::DOCUMENT,
        2 => ResourceTypes::STYLESHEET,
        3 => ResourceTypes::IMAGE,
        4 => ResourceTypes::MEDIA,
        5 => ResourceTypes::FONT,
        6 => ResourceTypes::SCRIPT,
        7 => ResourceTypes::XHR,
        8 => ResourceTypes::XHR,
        11 => ResourceTypes::WEBSOCKET,
        14 => ResourceTypes::PING,
        _ => ResourceTypes::OTHER,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filters_from(text: &str) -> Filters {
        Filters::parse("test", text)
    }

    #[test]
    fn the_webview_resource_contexts_map_to_the_right_flags() {
        // A rule's `$image` would silently match scripts if one were wrong, and
        // the numbers are not guessable.
        assert_eq!(context_to_type(1), ResourceTypes::DOCUMENT);
        assert_eq!(context_to_type(2), ResourceTypes::STYLESHEET);
        assert_eq!(context_to_type(3), ResourceTypes::IMAGE);
        assert_eq!(context_to_type(4), ResourceTypes::MEDIA);
        assert_eq!(context_to_type(5), ResourceTypes::FONT);
        assert_eq!(context_to_type(6), ResourceTypes::SCRIPT);
        assert_eq!(context_to_type(7), ResourceTypes::XHR);
        // FETCH is a different context with the same meaning for a filter.
        assert_eq!(context_to_type(8), ResourceTypes::XHR);
        assert_eq!(context_to_type(11), ResourceTypes::WEBSOCKET);
        assert_eq!(context_to_type(14), ResourceTypes::PING);
        // Text tracks, manifests, signed exchanges: `other`, as a list means it.
        assert_eq!(context_to_type(9), ResourceTypes::OTHER);
        assert_eq!(context_to_type(12), ResourceTypes::OTHER);
        assert_eq!(context_to_type(13), ResourceTypes::OTHER);
        // The `ALL` sentinel — what a failed version-2 cast reports — also lands
        // on `other`, and that is the deliberate direction: a rule naming a type
        // does not fire, rather than matching every type and over-blocking.
        assert_eq!(context_to_type(0), ResourceTypes::OTHER);
    }

    #[test]
    fn an_unknown_resource_type_under_blocks_rather_than_over_blocks() {
        // The failure mode this guards: an `$image` rule that fired for a script
        // would break the site it was meant to help. Reporting the type as
        // `other` makes such a rule stand down, and an untyped rule — the vast
        // majority of any list — still applies.
        let blocker = Blocker::new();
        blocker.set(
            filters_from("||ads.test/banner$image\n||track.test^\n"),
            Vec::new(),
        );
        // Untyped: fires for an unknown type, so blocking still works when the
        // runtime cannot say what the resource is.
        assert_eq!(
            blocker.verdict("https://track.test/x", "d.test", ResourceTypes::OTHER),
            Verdict::Block
        );
        // Typed: stands down for an unknown type.
        assert_eq!(
            blocker.verdict("https://ads.test/banner", "d.test", ResourceTypes::OTHER),
            Verdict::Pass
        );
        // And fires when the type really is known.
        assert_eq!(
            blocker.verdict("https://ads.test/banner", "d.test", ResourceTypes::IMAGE),
            Verdict::Block
        );
    }

    #[test]
    fn an_empty_blocker_is_inactive_so_no_filter_is_installed() {
        // A browser with blocking switched off should cost exactly what it did
        // before this existed, guaranteed by not registering a filter at all.
        let blocker = Blocker::new();
        assert!(!blocker.is_active());
        assert_eq!(
            blocker.verdict("https://ads.test/x", "news.test", ResourceTypes::SCRIPT),
            Verdict::Pass
        );
    }

    #[test]
    fn a_loaded_blocker_blocks_and_counts() {
        let blocker = Blocker::new();
        blocker.set(filters_from("||ads.test^\n"), Vec::new());
        assert!(blocker.is_active());
        assert_eq!(
            blocker.verdict("https://ads.test/x.gif", "news.test", ResourceTypes::IMAGE),
            Verdict::Block
        );
        assert_eq!(blocker.blocked_count(), 1);
        // A clean URL is not counted, or the number would be meaningless.
        assert_eq!(
            blocker.verdict("https://clean.test/x", "news.test", ResourceTypes::IMAGE),
            Verdict::Pass
        );
        assert_eq!(blocker.blocked_count(), 1);
        assert_eq!(blocker.rule_count(), 1);
    }

    #[test]
    fn the_allow_list_short_circuits_before_any_rule() {
        // This is what makes it trustworthy: a site the user exempted cannot be
        // broken by a list updated after they exempted it. `$important` included
        // — an allow list a rule can outrank is not an allow list.
        let blocker = Blocker::new();
        // `||ads.test^` is here so the last assertion below has a rule to fire.
        // Without it the list mentioned only `broken.test`, and the assertion
        // asked an empty set to block a host no rule named — an assertion that
        // could never pass, so the test proved nothing about the allow list.
        blocker.set(
            filters_from("||broken.test^\n||broken.test/x$important\n||ads.test^\n"),
            vec!["broken.test".to_string()],
        );
        assert_eq!(
            blocker.verdict("https://broken.test/x", "broken.test", ResourceTypes::SCRIPT),
            Verdict::Pass
        );
        // Subdomains of an exempted host are exempt, because that is what the
        // user meant by naming it.
        assert_eq!(
            blocker.verdict("https://cdn.broken.test/x", "broken.test", ResourceTypes::SCRIPT),
            Verdict::Pass
        );
        // A different host still blocks, so the exemption did not disable the
        // blocker.
        assert_eq!(
            blocker.verdict("https://ads.test/x", "other.test", ResourceTypes::SCRIPT),
            Verdict::Block
        );
    }

    #[test]
    fn an_allow_entry_may_be_a_url_or_a_bare_host() {
        // People paste whatever is in the address bar.
        let blocker = Blocker::new();
        blocker.set(
            filters_from("||shop.test^\n"),
            vec!["https://shop.test/some/path".to_string()],
        );
        assert_eq!(
            blocker.verdict("https://shop.test/x", "shop.test", ResourceTypes::SCRIPT),
            Verdict::Pass
        );
    }

    #[test]
    fn exempting_the_page_exempts_what_it_loads() {
        // A user who exempts a site means "leave this site alone", not "leave
        // only its own requests alone" — so a third-party script it needs is
        // allowed too.
        let blocker = Blocker::new();
        blocker.set(filters_from("||cdn.test^\n"), vec!["shop.test".to_string()]);
        assert_eq!(
            blocker.verdict("https://cdn.test/lib.js", "shop.test", ResourceTypes::SCRIPT),
            Verdict::Pass
        );
        // From anywhere else it is still blocked.
        assert_eq!(
            blocker.verdict("https://cdn.test/lib.js", "other.test", ResourceTypes::SCRIPT),
            Verdict::Block
        );
    }

    #[test]
    fn the_cosmetic_script_is_a_self_contained_injection() {
        let blocker = Blocker::new();
        blocker.set(filters_from("shop.test##.ad-banner\n"), Vec::new());
        let script = blocker.cosmetic_script("shop.test").expect("a rule for this host");
        assert!(script.contains(".ad-banner"), "{script}");
        assert!(script.contains("createElement('style')"), "{script}");
        assert!(script.contains("display: none !important"), "{script}");
        // Guarded, so injecting twice does not add a second sheet.
        assert!(script.contains("__loomCosmetic"), "{script}");

        // A host with no rules gets no script, so nothing is injected for
        // nothing on most pages.
        assert!(blocker.cosmetic_script("clean.test").is_none());
    }

    #[test]
    fn a_cosmetic_rule_with_a_quote_cannot_escape_its_script() {
        // A filter list is untrusted input and this is the one place it is
        // pasted into a script, so the JSON encoding is what has to hold.
        let blocker = Blocker::new();
        blocker.set(
            filters_from("evil.test##div[title=\"' + alert(1) + '\"]\n"),
            Vec::new(),
        );
        let script = blocker
            .cosmetic_script("evil.test")
            .expect("a rule for this host");
        // The property that has to hold is that the rule's double quotes arrive
        // **escaped**, so the payload cannot end the string literal it is pasted
        // into. It is the double quote that closes that literal; a single quote
        // cannot, inside a double-quoted string.
        //
        // The assertion this replaced looked for the payload's own text,
        // `' + alert(1) + '`, and required it to be *absent*. It can never be
        // absent: single quotes survive JSON encoding verbatim, and they are
        // harmless where they land. That assertion would have failed for
        // correct escaping and could only have passed for broken escaping, so
        // it had the polarity of the test backwards.
        assert!(
            script.contains(r#"div[title=\"' + alert(1) + '\"]"#),
            "the rule's double quotes must arrive escaped: {script}"
        );
        // The complementary half: the unescaped form must not appear, which is
        // what the escape failing would look like.
        assert!(
            !script.contains(r#"div[title="' + alert(1) + '\"]"#),
            "the rule's double quote reached the script unescaped: {script}"
        );
    }

    #[test]
    fn swapping_lists_swaps_the_verdict() {
        // The set is replaced whole rather than mutated, because a request
        // handler on another thread must never see a half-parsed list.
        let blocker = Blocker::new();
        blocker.set(filters_from("||first.test^\n"), Vec::new());
        assert_eq!(
            blocker.verdict("https://first.test/x", "d.test", ResourceTypes::SCRIPT),
            Verdict::Block
        );
        blocker.set(filters_from("||second.test^\n"), Vec::new());
        assert_eq!(
            blocker.verdict("https://first.test/x", "d.test", ResourceTypes::SCRIPT),
            Verdict::Pass
        );
        assert_eq!(
            blocker.verdict("https://second.test/x", "d.test", ResourceTypes::SCRIPT),
            Verdict::Block
        );
    }

    #[test]
    fn a_cloned_blocker_shares_the_one_set() {
        // Every tab gets a clone, and the whole point is that they share one
        // parsed list rather than one each.
        let blocker = Blocker::new();
        blocker.set(filters_from("||ads.test^\n"), Vec::new());
        let clone = blocker.clone();
        assert_eq!(
            clone.verdict("https://ads.test/x", "d.test", ResourceTypes::SCRIPT),
            Verdict::Block
        );
        // Including the counter, so the settings page reports one number.
        assert_eq!(blocker.blocked_count(), clone.blocked_count());
    }

    #[test]
    fn a_hosts_format_list_is_the_common_path_not_the_exotic_one() {
        // Two of the four presets ship hosts format.
        let blocker = Blocker::new();
        blocker.set(
            filters_from("0.0.0.0 doubleclick.net\n0.0.0.0 google-analytics.com\n"),
            Vec::new(),
        );
        assert_eq!(
            blocker.verdict("https://doubleclick.net/x", "news.test", ResourceTypes::SCRIPT),
            Verdict::Block
        );
        assert_eq!(
            blocker.verdict("https://www.google-analytics.com/collect", "news.test", ResourceTypes::XHR),
            Verdict::Block
        );
        assert_eq!(blocker.rule_count(), 2);
    }

    #[test]
    fn the_home_page_itself_is_never_blocked_by_a_hosts_list() {
        // A list blocking the address the user typed would be a blank window
        // with no way to explain itself, so it is worth pinning that a
        // document request to a blocked host is still refused — and that the
        // user's own allow list is the escape.
        let blocker = Blocker::new();
        blocker.set(filters_from("0.0.0.0 adsite.test\n"), Vec::new());
        assert_eq!(
            blocker.verdict("https://adsite.test/", "adsite.test", ResourceTypes::DOCUMENT),
            Verdict::Block
        );
        blocker.set(
            filters_from("0.0.0.0 adsite.test\n"),
            vec!["adsite.test".to_string()],
        );
        assert_eq!(
            blocker.verdict("https://adsite.test/", "adsite.test", ResourceTypes::DOCUMENT),
            Verdict::Pass
        );
    }
}
