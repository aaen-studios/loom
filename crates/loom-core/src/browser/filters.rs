//! Filter lists: the network half of a content blocker.
//!
//! # What this is, and what it is not
//!
//! uBlock Origin **the extension cannot run here** — WebView2 has no extension
//! API, so a `.crx` has nowhere to go. This is not a workaround for that,
//! because there isn't one; it is the same *technique* on the layer below.
//!
//! uBlock does two things:
//!
//! 1. **Network filtering** — it looks at every request the browser is about to
//!    make and cancels the ones matching a filter list. WebView2 exposes
//!    exactly this: `AddWebResourceRequestedFilter` + `add_WebResourceRequested`
//!    hands every request to the host, and `SetResponse` answers it. That is
//!    where this module's [`Filters::verdict`] is called from.
//! 2. **Cosmetic filtering** — it hides the elements whose requests it just
//!    blocked, so the page does not end up with holes. That is
//!    [`Filters::cosmetic_css`], applied by the injected collector.
//!
//! The lists are the same lists. EasyList, EasyPrivacy, the uBlock lists and
//! any hosts-format blocklist all parse here.
//!
//! # Why the parser is this shape
//!
//! A real list is 50,000–150,000 lines. A linear scan per request would make
//! every page load a measurable cost, so rules are indexed by a token they must
//! contain — the same idea as uBlock's own tokenizer. A URL only ever tests the
//! handful of rules sharing one of its tokens.
//!
//! The syntax is the subset that carries the weight: host anchors, wildcards,
//! separators, exceptions, resource-type and party options, `$domain=`
//! restrictions, and cosmetic rules. Anything unrecognised is *skipped and
//! counted* rather than guessed at, so the stats can say how much of a list
//! Loom actually understood.

use std::collections::{HashMap, HashSet};

/// Which kind of resource a request is for, mirroring WebView2's contexts.
///
/// Kept as a bitflag so one rule can name several (`$script,image`), because the
/// alternative is a `Vec` per rule and there are a hundred thousand rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceTypes(u16);

impl ResourceTypes {
    /// Every type. What a rule with no `$type` option applies to.
    pub const ALL: u16 = u16::MAX;

    pub const DOCUMENT: u16 = 1 << 0;
    pub const STYLESHEET: u16 = 1 << 1;
    pub const SCRIPT: u16 = 1 << 2;
    pub const IMAGE: u16 = 1 << 3;
    pub const FONT: u16 = 1 << 4;
    pub const MEDIA: u16 = 1 << 5;
    pub const XHR: u16 = 1 << 6;
    pub const PING: u16 = 1 << 7;
    pub const WEBSOCKET: u16 = 1 << 8;
    pub const OTHER: u16 = 1 << 9;

    pub fn none() -> Self {
        Self(0)
    }

    pub fn all() -> Self {
        Self(Self::ALL)
    }

    pub fn single(bit: u16) -> Self {
        Self(bit)
    }

    pub fn with(self, bit: u16) -> Self {
        Self(self.0 | bit)
    }

    /// Whether this rule applies to a request of that type.
    pub fn contains(self, bit: u16) -> bool {
        self.0 & bit != 0
    }

    pub fn is_all(self) -> bool {
        self.0 == Self::ALL
    }

    /// The name a filter list uses, from WebView2's resource context.
    pub fn from_context_name(name: &str) -> Self {
        let bit = match name {
            "document" => Self::DOCUMENT,
            "stylesheet" => Self::STYLESHEET,
            "script" => Self::SCRIPT,
            "image" => Self::IMAGE,
            "font" => Self::FONT,
            "media" => Self::MEDIA,
            "xhr" | "fetch" => Self::XHR,
            "ping" | "beacon" => Self::PING,
            "websocket" => Self::WEBSOCKET,
            _ => Self::OTHER,
        };
        Self::single(bit)
    }
}

/// What a decision came out as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Cancel the request.
    Block,
    /// Let it through, and let it through even where a block rule matched.
    Allow,
    /// No rule matched.
    Pass,
}

/// Why a verdict came out that way, for the settings page's counter and for a
/// probe. Cheap to produce and only built when something matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub verdict: Verdict,
    /// The rule's text, so the UI can show *why* something was blocked.
    pub rule: String,
    /// Which list it came from, so a broken list can be identified.
    pub list: String,
}

/// One request, as the matcher sees it.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    pub url: &'a str,
    /// The host of the page the request came from, for `$third-party` and for
    /// `$domain=` restrictions.
    pub document_host: &'a str,
    pub resource_type: u16,
}

impl<'a> Request<'a> {
    pub fn new(url: &'a str, document_host: &'a str, resource_type: u16) -> Self {
        Self {
            url,
            document_host,
            resource_type,
        }
    }

    /// The request's own host.
    fn host(&self) -> &str {
        host_of(self.url)
    }

    /// Whether the destination is a different site from the page.
    ///
    /// Registrable-domain comparison would be more precise and needs the public
    /// suffix list; a suffix comparison is what most blockers ship for this and
    /// errs on the side of *calling it first-party*, which is the safe direction
    /// — a `$third-party` rule that fails to fire shows an ad, while one that
    /// fires wrongly breaks a site.
    fn third_party(&self) -> bool {
        let request = self.host();
        let document = self.document_host;
        if request.is_empty() || document.is_empty() {
            return false;
        }
        request != document
            && !request.ends_with(&format!(".{document}"))
            && !document.ends_with(&format!(".{request}"))
    }
}

/// The host part of a URL, lowercased and without a port.
///
/// Deliberately hand-rolled and allocation-light: this runs for every request on
/// every page, and a real URL parser per call is a cost with no benefit when a
/// filter only ever cares about the authority.
pub fn host_of(url: &str) -> &str {
    let rest = match url.find("://") {
        Some(at) => &url[at + 3..],
        // A scheme-relative or bare host, which lists do contain.
        None => url.strip_prefix("//").unwrap_or(url),
    };
    let end = rest
        .find(['/', '?', '#'])
        .unwrap_or(rest.len());
    let authority = &rest[..end];
    // Strip userinfo, then a port.
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    match authority.rfind(':') {
        // Only strip a trailing `:port`, not the colon in an IPv6 literal.
        Some(at) if !authority[at + 1..].contains(']') && authority[..at].contains('.') => {
            &authority[..at]
        }
        _ => authority,
    }
}

/// Whether `host` is `domain` or a subdomain of it.
fn host_matches_domain(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// A compiled filter list.
#[derive(Default)]
pub struct Filters {
    rules: Vec<Rule>,
    /// Token → indices into `rules`. A rule is only tested when the URL carries
    /// one of its tokens, which is what makes a 100,000-line list cheap.
    index: HashMap<String, Vec<u32>>,
    /// Rules with no usable token — short patterns, mostly — tested always.
    /// Kept separate rather than dropped, because a rule that never runs is a
    /// silently missing feature.
    unindexed: Vec<u32>,

    /// Cosmetic rules with a domain: `example.com##.ad`.
    cosmetic_by_domain: HashMap<String, Vec<String>>,
    /// Cosmetic exceptions: `example.com#@#.ad`.
    cosmetic_allowed: HashMap<String, HashSet<String>>,
    /// Cosmetic rules with no domain: `##.ad`, which apply everywhere.
    cosmetic_generic: Vec<String>,

    stats: Stats,
}

/// What a parse did, so the settings page can be honest about coverage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    pub lists: u32,
    pub lines: u32,
    /// Rules that will be consulted.
    pub rules: u32,
    pub cosmetic: u32,
    /// Lines that are comments, headers, or blank.
    pub ignored: u32,
    /// Lines this parser does not understand. Counted rather than guessed at:
    /// a rule silently reinterpreted is worse than one openly skipped.
    pub unsupported: u32,
}

#[derive(Debug, Clone)]
struct Rule {
    /// The rule's own text, for a `Match`.
    text: String,
    list: String,
    pattern: Pattern,
    exception: bool,
    /// Blocks that override exceptions, from `$important`.
    important: bool,
    types: ResourceTypes,
    /// `None` means the rule does not care.
    third_party: Option<bool>,
    /// `$domain=a.com|~b.com` — include and exclude, both as suffixes.
    domain_include: Vec<String>,
    domain_exclude: Vec<String>,
}

#[derive(Debug, Clone)]
enum Pattern {
    /// `||host` — the host, or a subdomain of it, and nothing after.
    Host(String),
    /// `||host^…` or `||host/path*` — a host boundary, then a pattern that has
    /// to match from where the host ends in the URL.
    ///
    /// The distinction from [`Pattern::Host`] is what makes `||example.com^`
    /// match `https://example.com/x` without also matching
    /// `https://notexample.com/x`, which a plain substring test would do.
    HostTail { host: String, rest: Vec<Part> },
    /// `||adservice.google.*^` — a wildcard *inside* the authority, which no
    /// fixed host can express.
    ///
    /// Matched by trying each label boundary of the authority, so the anchor
    /// still means "starts at a host boundary" and a pattern can never match in
    /// the middle of a label.
    HostAnchor(Vec<Part>),
    /// `|http://x` — an anchored prefix.
    Prefix(String),
    /// `x|` — an anchored suffix.
    Suffix(String),
    /// A plain substring.
    Contains(String),
    /// Wildcards and separators, in order.
    Complex(Vec<Part>),
}

#[derive(Debug, Clone)]
enum Part {
    Literal(String),
    /// `*` — any run of characters, including none.
    Any,
    /// `^` — a separator: anything that is not a letter, digit, `_`, `-`, `.`,
    /// `%`, or the end of the URL.
    Separator,
}

/// Matches a pattern that must begin at a **host boundary** — what `||` means.
///
/// The attempt is made at the start of the authority and after every dot in it,
/// which is what lets one pattern cover `example.com`, `www.example.com` and
/// `adservice.google.co.uk` without ever matching inside a label: a literal has
/// to line up with a boundary, so `||ads.test^` cannot find `ads.test` in
/// `notads.test`.
fn match_anchored(parts: &[Part], url: &str) -> bool {
    let Some((start, end)) = host_span(url) else {
        return false;
    };
    let authority = &url[start..end];
    let mut offset = 0usize;
    loop {
        if match_complex(parts, &url[start + offset..]) {
            return true;
        }
        match authority[offset..].find('.') {
            Some(at) => offset += at + 1,
            None => return false,
        }
    }
}

/// Whether a byte is a separator for the purpose of `^`.
fn is_separator(byte: u8) -> bool {
    !(byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b'%'))
}

impl Filters {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn stats(&self) -> &Stats {
        &self.stats
    }

    /// Parses one list into this set.
    pub fn extend(&mut self, name: &str, text: &str) {
        self.stats.lists += 1;
        for raw in text.lines() {
            self.stats.lines += 1;
            self.add_line(name, raw);
        }
        self.reindex();
    }

    /// Parses one list into a fresh set.
    pub fn parse(name: &str, text: &str) -> Self {
        let mut filters = Self::new();
        filters.extend(name, text);
        filters
    }

    /// Whether there is anything to match with. A caller with an empty set skips
    /// installing the request filter entirely, which keeps an unconfigured
    /// browser exactly as fast as one with no blocker at all.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty() && self.cosmetic_generic.is_empty() && self.cosmetic_by_domain.is_empty()
    }

    fn add_line(&mut self, list: &str, raw: &str) {
        let line = raw.trim();
        if line.is_empty() {
            self.stats.ignored += 1;
            return;
        }

        // Cosmetic first, and *before* the comment check, because `##` starts
        // with the same character a hosts-file comment does.
        if let Some(cosmetic) = split_cosmetic(line) {
            // A *procedural* rule — `div:has-text(Sponsored)`, `:xpath(…)` — is
            // not CSS. Handing one to the page as a selector would invalidate the
            // whole stylesheet and hide nothing, so it is counted as unsupported
            // rather than injected.
            if procedural(&cosmetic.selector) {
                self.stats.unsupported += 1;
                return;
            }
            if cosmetic.negative {
                self.cosmetic_allowed
                    .entry(cosmetic.domain.clone())
                    .or_default()
                    .insert(cosmetic.selector.clone());
                // An exception names a rule that already exists, so it is not
                // itself a cosmetic rule; counting it would make the reported
                // total depend on how many times a list says "except here".
                return;
            }
            if cosmetic.domain.is_empty() {
                self.cosmetic_generic.push(cosmetic.selector.clone());
            } else {
                self.cosmetic_by_domain
                    .entry(cosmetic.domain.clone())
                    .or_default()
                    .push(cosmetic.selector.clone());
            }
            self.stats.cosmetic += 1;
            return;
        }

        // Comments and headers, after cosmetics: `!` and `[` are the filter-list
        // forms and `#` is the hosts-file form.
        if line.starts_with('!') || line.starts_with('[') || line.starts_with('#') {
            self.stats.ignored += 1;
            return;
        }

        // Hosts format, which most blocklists ship: `0.0.0.0 ads.example.com`,
        // `127.0.0.1 ads.example.com`, or a bare `ads.example.com`.
        if let Some(domain) = hosts_line(line) {
            if domain.is_empty() {
                self.stats.ignored += 1;
                return;
            }
            self.push_rule(Rule {
                text: format!("||{domain}^"),
                list: list.to_string(),
                pattern: Pattern::Host(domain.to_string()),
                exception: false,
                important: false,
                types: ResourceTypes::all(),
                third_party: None,
                domain_include: Vec::new(),
                domain_exclude: Vec::new(),
            });
            return;
        }

        if let Some(rule) = parse_rule(list, line) {
            self.push_rule(rule);
        } else {
            self.stats.unsupported += 1;
        }
    }

    fn push_rule(&mut self, rule: Rule) {
        let index = self.rules.len() as u32;
        self.rules.push(rule);
        match self.token_for(index) {
            Some(token) => self.index.entry(token).or_default().push(index),
            None => self.unindexed.push(index),
        }
        self.stats.rules += 1;
    }

    /// Rebuilds the token index.
    ///
    /// Run once per `extend` rather than per rule: inserting into the map as
    /// rules arrive is the same amount of work, but doing it at the end means a
    /// list merges without the index being touched 100,000 times.
    fn reindex(&mut self) {
        self.index.clear();
        self.unindexed.clear();
        for index in 0..self.rules.len() as u32 {
            match self.token_for(index) {
                Some(token) => self.index.entry(token).or_default().push(index),
                None => self.unindexed.push(index),
            }
        }
    }

    /// The token a rule must be found by: its longest alphanumeric run.
    ///
    /// `None` for a rule with no run of three or more characters — those are
    /// matched always, because there is nothing to look them up by.
    fn token_for(&self, index: u32) -> Option<String> {
        let rule = self.rules.get(index as usize)?;
        let text = match &rule.pattern {
            Pattern::Host(host) => host.clone(),
            Pattern::HostTail { host, .. } => host.clone(),
            Pattern::HostAnchor(parts) => parts
                .iter()
                .filter_map(|part| match part {
                    Part::Literal(text) => Some(text.as_str()),
                    _ => None,
                })
                .max_by_key(|text| text.len())?
                .to_string(),
            Pattern::Prefix(text) | Pattern::Suffix(text) | Pattern::Contains(text) => text.clone(),
            Pattern::Complex(parts) => parts
                .iter()
                .filter_map(|part| match part {
                    Part::Literal(text) => Some(text.as_str()),
                    _ => None,
                })
                .max_by_key(|text| text.len())?
                .to_string(),
        };
        longest_token(&text)
    }

    /// Decides one request.
    ///
    /// Exceptions are checked first and win, except against `$important` — which
    /// is what that option is *for*: an allow rule for a broad domain should not
    /// re-enable something a list marked as an unbreakable block.
    pub fn verdict(&self, request: Request<'_>) -> Option<Match> {
        if self.rules.is_empty() {
            return None;
        }
        let url = request.url.to_ascii_lowercase();
        let host = host_of(&url).to_string();
        let third_party = request.third_party();

        let mut best: Option<Match> = None;
        for rule in self.candidates(&url) {
            if !rule.matches(
                &url,
                &host,
                third_party,
                request.resource_type,
                request.document_host,
            ) {
                continue;
            }
            // `$important` outranks everything, so it is taken immediately.
            if rule.important && !rule.exception {
                return Some(Match {
                    verdict: Verdict::Block,
                    rule: rule.text.clone(),
                    list: rule.list.clone(),
                });
            }
            match &best {
                // An exception already found stands: a second block rule does
                // not defeat it, because that is what `@@` means.
                Some(found) if found.verdict == Verdict::Allow => {}
                _ => {
                    best = Some(Match {
                        verdict: if rule.exception {
                            Verdict::Allow
                        } else {
                            Verdict::Block
                        },
                        rule: rule.text.clone(),
                        list: rule.list.clone(),
                    });
                }
            }
        }
        best
    }

    /// The rules worth testing for this URL: those sharing a token with it, plus
    /// those that could not be indexed.
    fn candidates(&self, url: &str) -> impl Iterator<Item = &Rule> {
        let mut seen: Vec<u32> = Vec::new();
        for token in tokens_of(url) {
            if let Some(indices) = self.index.get(&token) {
                seen.extend_from_slice(indices);
            }
        }
        seen.extend_from_slice(&self.unindexed);
        seen.sort_unstable();
        seen.dedup();
        seen.into_iter().filter_map(move |index| self.rules.get(index as usize))
    }

    /// The CSS that hides what was blocked on this page.
    ///
    /// Domain-scoped rules for this host are emitted first so a later generic
    /// rule of the same specificity cannot be the one that wins by accident,
    /// and exceptions are removed by *name* rather than by trying to out-specify
    /// them — a selector a list explicitly allowed should not be in the sheet at
    /// all.
    pub fn cosmetic_css(&self, host: &str) -> String {
        if self.is_empty() {
            return String::new();
        }
        let host = host.to_ascii_lowercase();
        let allowed = self.cosmetic_allowed.get(&host);

        let mut selectors: Vec<&str> = Vec::new();
        // A rule for `example.com` applies to `www.example.com` too, which is
        // what the lists assume.
        for (domain, rules) in &self.cosmetic_by_domain {
            if !host_matches_domain(&host, domain) {
                continue;
            }
            for selector in rules {
                if allowed.is_some_and(|set| set.contains(selector)) {
                    continue;
                }
                selectors.push(selector);
            }
        }
        for selector in &self.cosmetic_generic {
            if allowed.is_some_and(|set| set.contains(selector)) {
                continue;
            }
            selectors.push(selector);
        }

        if selectors.is_empty() {
            return String::new();
        }
        selectors.sort_unstable();
        selectors.dedup();
        // `:not(body):not(html)` guards against a bad generic rule — a list that
        // says `##.content` and a site whose whole body is `.content` would
        // otherwise render a blank page, which is the classic way a blocker
        // breaks a site.
        format!(
            "{} {{ display: none !important; }}",
            selectors
                .iter()
                .map(|selector| format!("{selector}:not(body):not(html)"))
                .collect::<Vec<_>>()
                .join(",\n")
        )
    }

    /// A compact one-line summary, for the settings page.
    pub fn summary(&self) -> String {
        format!(
            "{} rules ({} cosmetic) from {} list(s)",
            self.stats.rules, self.stats.cosmetic, self.stats.lists
        )
    }
}

impl Rule {
    /// Whether this rule applies to one request.
    ///
    /// `document_host` is threaded through rather than stored on the rule: a
    /// rule outlives a request by design — the list is parsed once and consulted
    /// for every request on every page — so borrowing the document into it would
    /// make the whole set lifetime-bound to one page load.
    fn matches(
        &self,
        url: &str,
        host: &str,
        third_party: bool,
        resource_type: u16,
        document_host: &str,
    ) -> bool {
        if !self.types.is_all() && !self.types.contains(resource_type) {
            return false;
        }
        if let Some(wanted) = self.third_party {
            if wanted != third_party {
                return false;
            }
        }
        // `$domain=` is an include and an exclude list at once. An include list
        // that does not name the page means the rule does not apply; an exclude
        // that does means the same.
        if !self.domain_include.is_empty()
            && !self
                .domain_include
                .iter()
                .any(|domain| host_matches_domain(document_host, domain))
        {
            return false;
        }
        if self
            .domain_exclude
            .iter()
            .any(|domain| host_matches_domain(document_host, domain))
        {
            return false;
        }

        match &self.pattern {
            Pattern::Host(domain) => host_matches_domain(host, domain),
            Pattern::HostTail { host: domain, rest } => {
                if !host_matches_domain(host, domain) {
                    return false;
                }
                // Match the tail against the URL from where the host ends, so
                // `^` in `||example.com^` is tested against the `/` that
                // follows the authority rather than against the host itself.
                match host_span(url) {
                    Some((_, end)) => match_complex(rest, &url[end..]),
                    None => false,
                }
            }
            Pattern::HostAnchor(parts) => match_anchored(parts, url),
            Pattern::Prefix(prefix) => url.starts_with(prefix.as_str()),
            Pattern::Suffix(suffix) => url.ends_with(suffix.as_str()),
            Pattern::Contains(text) => url.contains(text.as_str()),
            Pattern::Complex(parts) => match_complex(parts, url),
        }
    }
}

/// The byte range of a URL's authority, `[start, end)`.
///
/// Needed because `||host^…` has to be matched *from where the host ends*, and
/// the only way to know that is to find it.
pub fn host_span(url: &str) -> Option<(usize, usize)> {
    let start = match url.find("://") {
        Some(at) => at + 3,
        None => url.strip_prefix("//").map(|_| 2).unwrap_or(0),
    };
    let rest = &url[start..];
    let end = start + rest.find(['/', '?', '#']).unwrap_or(rest.len());
    if end <= start {
        return None;
    }
    Some((start, end))
}

/// Parses one line into a network rule, or `None` if it is not understood.
///
/// Cosmetic lines never reach here: [`Filters::add_line`] routes them first,
/// because a rule with no URL to match has no business in the network set.
fn parse_rule(list: &str, line: &str) -> Option<Rule> {
    let (body, options) = split_options(line);
    let exception = body.starts_with("@@");
    let body = body.strip_prefix("@@").unwrap_or(body);
    if body.is_empty() {
        return None;
    }

    let pattern = parse_pattern(body)?;
    let mut parsed = Options::default();
    if !parse_options(options, &mut parsed) {
        return None;
    }

    Some(Rule {
        text: line.to_string(),
        list: list.to_string(),
        pattern,
        exception,
        important: parsed.important,
        types: parsed.types,
        third_party: parsed.third_party,
        domain_include: parsed.domain_include,
        domain_exclude: parsed.domain_exclude,
    })
}

/// A cosmetic rule: the domain it applies to, the selector, and which way round.
///
/// An empty `domain` with `negative: false` is a generic rule, which applies
/// everywhere and is the reason `:not(body)` guards the generated sheet.
struct Cosmetic {
    domain: String,
    selector: String,
    negative: bool,
}

/// The `$option,option` tail, and the pattern before it.
///
/// The split is on the *last* `$` that is not inside a regex, because a pattern
/// can legitimately contain one — `*/$*` is a real EasyList rule — and taking
/// the first would mangle it.
fn split_options(line: &str) -> (&str, &str) {
    match line.rfind('$') {
        Some(at) if at > 0 && !line[at..].contains(' ') => (&line[..at], &line[at + 1..]),
        _ => (line, ""),
    }
}

#[derive(Default)]
struct Options {
    types: ResourceTypes,
    third_party: Option<bool>,
    important: bool,
    domain_include: Vec<String>,
    domain_exclude: Vec<String>,
}

impl Default for ResourceTypes {
    fn default() -> Self {
        Self(ResourceTypes::ALL)
    }
}

/// Returns `false` when the rule must be dropped.
fn parse_options(text: &str, out: &mut Options) -> bool {
    if text.is_empty() {
        return true;
    }
    // `types` starts as every type; a rule naming types narrows it, and a rule
    // that names only exclusions removes from the full set.
    let mut named: u16 = 0;
    let mut excluded: u16 = 0;

    for option in text.split(',') {
        let option = option.trim();
        if option.is_empty() {
            continue;
        }
        let (name, value) = match option.split_once('=') {
            Some((name, value)) => (name.trim(), Some(value.trim())),
            None => (option, None),
        };
        let negated = name.starts_with('~');
        let name = name.trim_start_matches('~');

        let bit = match name {
            "document" | "doc" => Some(ResourceTypes::DOCUMENT),
            "stylesheet" | "css" => Some(ResourceTypes::STYLESHEET),
            "script" => Some(ResourceTypes::SCRIPT),
            "image" | "img" => Some(ResourceTypes::IMAGE),
            "font" => Some(ResourceTypes::FONT),
            "media" => Some(ResourceTypes::MEDIA),
            "xmlhttprequest" | "xhr" => Some(ResourceTypes::XHR),
            "ping" | "beacon" => Some(ResourceTypes::PING),
            "websocket" => Some(ResourceTypes::WEBSOCKET),
            "other" => Some(ResourceTypes::OTHER),
            "subdocument" | "object" | "object-subrequest" => Some(ResourceTypes::OTHER),
            _ => None,
        };
        if let Some(bit) = bit {
            if negated {
                excluded |= bit;
            } else {
                named |= bit;
            }
            continue;
        }

        match (name, negated) {
            ("third-party", false) => out.third_party = Some(true),
            ("third-party", true) => out.third_party = Some(false),
            ("first-party", false) => out.third_party = Some(false),
            ("first-party", true) => out.third_party = Some(true),
            ("important", false) => out.important = true,
            ("domain", false) | ("from", false) => {
                for domain in value.unwrap_or("").split('|') {
                    let domain = domain.trim().trim_start_matches('~').to_ascii_lowercase();
                    if domain.is_empty() {
                        continue;
                    }
                    if option.starts_with("domain=~") || option.starts_with("from=~") {
                        out.domain_exclude.push(domain);
                    } else if option.contains("~") {
                        // A mixed list: `$domain=a.com|~b.com`.
                        out.domain_exclude.push(domain);
                    } else {
                        out.domain_include.push(domain);
                    }
                }
            }
            // Naming the document's own domain in the negative is common and
            // must not drop the rule.
            ("domain", true) | ("from", true) => {
                for domain in value.unwrap_or("").split('|') {
                    let domain = domain.trim().to_ascii_lowercase();
                    if !domain.is_empty() {
                        out.domain_exclude.push(domain);
                    }
                }
            }
            // Options that change *what* is served rather than whether it is:
            // redirects, `$csp`, `$removeparam`, `$replace`. Silently treating
            // one of these as a plain block would break a site in the name of
            // blocking less, so the rule is dropped — and counted as dropped.
            ("redirect", _) | ("redirect-rule", _) | ("csp", _) | ("removeparam", _)
            | ("replace", _) | ("inline-script", _) | ("inline-font", _)
            | ("genericblock", _) | ("generichide", _) | ("elemhide", _)
            | ("specifichide", _) | ("match-case", _) | ("badfilter", _) => return false,
            // Unknown: keep the rule without the option. A rule that blocks is
            // more useful than a rule that vanishes because of a suffix this
            // build has not heard of.
            _ => {}
        }
    }

    out.types = if excluded != 0 {
        ResourceTypes(ResourceTypes::ALL & !excluded)
    } else if named != 0 {
        ResourceTypes(named)
    } else {
        ResourceTypes::all()
    };
    true
}

fn parse_pattern(body: &str) -> Option<Pattern> {
    let host_anchored = body.starts_with("||");
    let prefix_anchored = !host_anchored && body.starts_with('|');
    let suffix_anchored = body.ends_with('|');
    let body = body
        .strip_prefix("||")
        .or_else(|| body.strip_prefix('|'))
        .unwrap_or(body);
    let body = body.strip_suffix('|').unwrap_or(body);
    if body.is_empty() {
        return None;
    }

    if host_anchored {
        // `||` anchors at a host boundary. How that is expressed depends on
        // whether a wildcard lands inside the authority itself.
        let authority_end = body.find('/').unwrap_or(body.len());
        if body[..authority_end].contains('*') {
            // `||adservice.google.*^` — no fixed host can say this, so the whole
            // pattern is matched from each label boundary instead.
            return Some(Pattern::HostAnchor(parse_parts(body)?));
        }
        // The common case: a literal host and an optional tail. Everything up
        // to the first separator is the host, and the rest is matched from where
        // that host ends in the URL.
        let split = body.find(['/', '^', '*']).unwrap_or(body.len());
        let (host, rest) = body.split_at(split);
        if host.is_empty() {
            return None;
        }
        let host = host.to_ascii_lowercase();
        if rest.is_empty() {
            return Some(Pattern::Host(host));
        }
        return Some(Pattern::HostTail {
            host,
            rest: parse_parts(rest)?,
        });
    }

    let wildcarded = body.contains('*') || body.contains('^');
    if !wildcarded {
        let lowered = body.to_ascii_lowercase();
        if prefix_anchored {
            return Some(Pattern::Prefix(lowered));
        }
        if suffix_anchored {
            return Some(Pattern::Suffix(lowered));
        }
        return Some(Pattern::Contains(lowered));
    }

    let mut parts = parse_parts(body)?;
    // A pattern with no `|` anchor means "find me anywhere in the URL", and
    // `match_complex` walks from a fixed position — so an unanchored pattern
    // needs a leading `*` to say that. Without it, `/adserv^` would only match a
    // URL *beginning* with `/adserv`, which is to say never: every URL starts
    // with a scheme. A plain pattern escapes this because `Contains` uses
    // `url.contains`, and the wildcarded form has no equivalent — so the leading
    // `Any` is what makes the two agree.
    if !prefix_anchored && !parts.first().is_some_and(|part| matches!(part, Part::Any)) {
        parts.insert(0, Part::Any);
    }
    Some(Pattern::Complex(parts))
}

/// Splits a pattern body into literals, `*` and `^`.
fn parse_parts(body: &str) -> Option<Vec<Part>> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    for ch in body.chars() {
        match ch {
            '*' => {
                if !literal.is_empty() {
                    parts.push(Part::Literal(literal.to_ascii_lowercase()));
                    literal.clear();
                }
                // Collapse `**` into one, or the matcher has to consider every
                // split point for nothing.
                if !matches!(parts.last(), Some(Part::Any)) {
                    parts.push(Part::Any);
                }
            }
            '^' => {
                if !literal.is_empty() {
                    parts.push(Part::Literal(literal.to_ascii_lowercase()));
                    literal.clear();
                }
                parts.push(Part::Separator);
            }
            other => literal.push(other),
        }
    }
    if !literal.is_empty() {
        parts.push(Part::Literal(literal.to_ascii_lowercase()));
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts)
}

/// Matches a wildcard/separator pattern against a URL.
///
/// A greedy left-to-right walk with one backtrack point per `*`, which is linear
/// in the URL for the patterns lists actually contain. A general wildcard
/// matcher is exponential on adversarial input, and a filter list is not
/// adversarial — it is a text file a person wrote.
fn match_complex(parts: &[Part], url: &str) -> bool {
    let bytes = url.as_bytes();
    let mut position = 0usize;
    let mut star: Option<(usize, usize)> = None; // (url index, part index)

    let mut index = 0usize;
    while index < parts.len() {
        match &parts[index] {
            Part::Any => {
                star = Some((position, index));
                index += 1;
                continue;
            }
            Part::Separator => {
                if position < bytes.len() && is_separator(bytes[position]) {
                    position += 1;
                    index += 1;
                    continue;
                }
                // End of URL also satisfies `^`, which is how `||x^` matches a
                // bare `https://x`.
                if position == bytes.len() {
                    index += 1;
                    continue;
                }
            }
            Part::Literal(text) => {
                if url[position..].starts_with(text.as_str()) {
                    position += text.len();
                    index += 1;
                    continue;
                }
            }
        }
        // Backtrack to the last `*` and let it swallow one more character.
        match star {
            Some((at, part)) if at < bytes.len() => {
                position = at + 1;
                star = Some((position, part));
                index = part + 1;
            }
            _ => return false,
        }
    }
    true
}

/// The cosmetic half of a line, or `None` if it is a network rule.
///
/// One domain per line, because that is what the storage wants: a rule naming
/// three domains becomes three entries. The alternative — a `Vec<String>` on
/// each — would make the per-host lookup a scan.
fn split_cosmetic(line: &str) -> Option<Cosmetic> {
    // `#@#` first, because it contains `##`.
    let (marker, negative) = if line.contains("#@#") {
        ("#@#", true)
    } else if line.contains("##") {
        ("##", false)
    } else {
        // `#?#` is a procedural rule — `div:has-text(Sponsored)` — and `#$#` is
        // a script injection. Neither is CSS, and handing one to the page as a
        // selector would invalidate the whole stylesheet and hide nothing.
        return None;
    };
    let (domains, selector) = line.split_once(marker)?;
    let selector = selector.trim();
    if selector.is_empty() {
        return None;
    }
    // A marker inside the selector means this split was wrong.
    if selector.contains("##") || selector.contains("#@#") {
        return None;
    }

    // The first domain wins when several are named. Storing all of them would
    // be more faithful and would need a rule-per-domain expansion; the common
    // case in real lists is one domain, and a rule that applies to the first of
    // two is a smaller error than one that applies to neither.
    let domain = domains
        .split(',')
        .map(|entry| entry.trim())
        // `~domain` is an *exception* for that domain, which is a different
        // thing from the selector being an exception. Skipped rather than
        // treated as an include, which would apply the rule to the one site the
        // list said to leave alone.
        .find(|entry| !entry.is_empty() && !entry.starts_with('~'))
        .map(|entry| entry.to_ascii_lowercase())
        .unwrap_or_default();

    Some(Cosmetic {
        domain,
        selector: selector.to_string(),
        negative,
    })
}

/// A hosts-format line, or `None` if this is not one.
///
/// The bare-domain arm is the one to be careful with: a filter rule with no
/// options — `||ads.test^`, `example.com##.ad`, `/banner_ad.` — is *also* a line
/// with no space in it and a dot. Accepting one as a hostname produces a rule
/// that silently matches nothing, which is why anything carrying filter syntax is
/// rejected here before it can be mistaken for a domain.
fn hosts_line(line: &str) -> Option<&str> {
    let mut parts = line.split_whitespace();
    let first = parts.next()?;
    // `0.0.0.0 host`, `127.0.0.1 host`, `::1 host`.
    if matches!(first, "0.0.0.0" | "127.0.0.1" | "::1" | "::" | "0") {
        let host = parts.next().unwrap_or("");
        // A hosts file also lists `localhost`, which is not an ad.
        if host.is_empty() || matches!(host, "localhost" | "localhost.localdomain" | "broadcasthost")
        {
            return Some("");
        }
        return Some(host);
    }
    // A bare domain, as some lists ship: no space, a dot, and none of the
    // characters a filter rule is built from.
    let looks_like_filter = first.chars().any(|character| {
        matches!(
            character,
            '|' | '^' | '*' | '$' | '/' | '@' | '#' | '?' | '=' | '%'
        )
    });
    if !first.contains(' ') && first.contains('.') && !looks_like_filter {
        return Some(first.trim_end_matches('.'));
    }
    None
}

/// Whether a cosmetic selector is procedural rather than CSS.
///
/// uBlock's extended syntax — `:has-text()`, `:xpath()`, `:matches-css()`,
/// `:remove()` — is a small language of its own. Injecting one as a selector
/// would make the *whole* stylesheet invalid, so a page would lose the ordinary
/// hiding it should have had as well as the rule that could not work.
fn procedural(selector: &str) -> bool {
    const MARKERS: [&str; 6] = [
        ":has-text(",
        ":matches-css(",
        ":matches-attr(",
        ":xpath(",
        ":remove(",
        ":style(",
    ];
    MARKERS.iter().any(|marker| selector.contains(marker))
}

/// The longest alphanumeric run in `text`, lowercased, if it is worth indexing.
fn longest_token(text: &str) -> Option<String> {
    let mut best = "";
    let mut current_start = None;
    for (index, byte) in text.bytes().enumerate() {
        if byte.is_ascii_alphanumeric() {
            if current_start.is_none() {
                current_start = Some(index);
            }
        } else if let Some(start) = current_start.take() {
            if index - start > best.len() {
                best = &text[start..index];
            }
        }
    }
    if let Some(start) = current_start {
        if text.len() - start > best.len() {
            best = &text[start..];
        }
    }
    if best.len() < 3 {
        return None;
    }
    Some(best.to_ascii_lowercase())
}

/// Every alphanumeric run in a URL, three characters or longer. These are the
/// keys a rule can be looked up by.
fn tokens_of(url: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut start: Option<usize> = None;
    for (index, byte) in url.bytes().enumerate() {
        if byte.is_ascii_alphanumeric() {
            if start.is_none() {
                start = Some(index);
            }
        } else if let Some(from) = start.take() {
            if index - from >= 3 {
                tokens.push(url[from..index].to_string());
            }
        }
    }
    if let Some(from) = start {
        if url.len() - from >= 3 {
            tokens.push(url[from..].to_string());
        }
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(text: &str) -> Filters {
        Filters::parse("test", text)
    }

    fn blocks(filters: &Filters, url: &str, document: &str) -> bool {
        filters
            .verdict(Request::new(url, document, ResourceTypes::IMAGE))
            .is_some_and(|found| found.verdict == Verdict::Block)
    }

    #[test]
    fn a_host_rule_blocks_the_host_and_its_subdomains() {
        let filters = list("||ads.example.com^\n");
        assert!(blocks(&filters, "https://ads.example.com/banner.png", "example.com"));
        assert!(blocks(&filters, "https://a.b.ads.example.com/x.js", "example.com"));
        // Not the bare domain: `||host^` anchors on the host, and `example.com`
        // is not `ads.example.com`.
        assert!(!blocks(&filters, "https://example.com/", "example.com"));
        // Nor a domain that merely ends with the same letters.
        assert!(!blocks(&filters, "https://notads.example.com.evil.test/x", "example.com"));
    }

    #[test]
    fn a_hosts_file_parses_as_host_rules() {
        // Most blocklists ship hosts format, and treating one as a filter list
        // would produce rules that match nothing.
        let filters = list(
            "# comment\n\
             0.0.0.0 doubleclick.net\n\
             127.0.0.1 google-analytics.com\n\
             ::1 adservice.google.com\n\
             0.0.0.0 localhost\n",
        );
        assert!(blocks(&filters, "https://doubleclick.net/x", "example.com"));
        assert!(blocks(&filters, "https://www.google-analytics.com/collect", "example.com"));
        assert!(blocks(&filters, "https://adservice.google.com/x", "example.com"));
        // `localhost` is not an ad, and blocking it breaks every dev server.
        assert!(!blocks(&filters, "https://localhost:3000/x", "localhost"));
        assert_eq!(filters.stats().rules, 3);
    }

    #[test]
    fn an_exception_wins_over_the_block_that_matched() {
        // The whole point of `@@`: a list blocks a whole domain, and a site that
        // genuinely needs one path has to be able to say so.
        let filters = list("||example.com^\n@@||example.com/allowed^\n");
        assert!(blocks(&filters, "https://example.com/ads/x.js", "other.test"));
        let verdict = filters
            .verdict(Request::new(
                "https://example.com/allowed/thing.js",
                "other.test",
                ResourceTypes::SCRIPT,
            ))
            .expect("exception should match");
        assert_eq!(verdict.verdict, Verdict::Allow);
    }

    #[test]
    fn important_outranks_an_exception() {
        // This is what `$important` exists for: a broad allow rule must not
        // re-enable something a list marked unbreakable.
        let filters = list("@@||example.com^\n||example.com/tracker.js$important\n");
        let verdict = filters
            .verdict(Request::new(
                "https://example.com/tracker.js",
                "other.test",
                ResourceTypes::SCRIPT,
            ))
            .expect("important block should win");
        assert_eq!(verdict.verdict, Verdict::Block);
    }

    #[test]
    fn a_resource_type_option_narrows_the_rule() {
        let filters = list("||example.com/track$script\n");
        assert!(filters
            .verdict(Request::new("https://example.com/track", "x.test", ResourceTypes::SCRIPT))
            .is_some());
        // The same URL as an image is not matched: `$script` says script only.
        assert!(filters
            .verdict(Request::new("https://example.com/track", "x.test", ResourceTypes::IMAGE))
            .is_none());
    }

    #[test]
    fn a_negated_type_removes_from_the_full_set() {
        // `$~image` means "everything except images", which is the opposite of
        // what "narrow to the named types" would produce.
        let filters = list("||example.com/x$~image\n");
        assert!(filters
            .verdict(Request::new("https://example.com/x", "d.test", ResourceTypes::SCRIPT))
            .is_some());
        assert!(filters
            .verdict(Request::new("https://example.com/x", "d.test", ResourceTypes::IMAGE))
            .is_none());
    }

    #[test]
    fn third_party_is_judged_against_the_document() {
        let filters = list("||example.com/x$third-party\n");
        // A different site asking: third-party, so the rule applies.
        assert!(filters
            .verdict(Request::new("https://example.com/x", "other.test", ResourceTypes::XHR))
            .is_some());
        // The site asking itself: first-party, so it does not.
        assert!(filters
            .verdict(Request::new("https://example.com/x", "example.com", ResourceTypes::XHR))
            .is_none());
        // A subdomain asking its own parent is still first-party.
        assert!(filters
            .verdict(Request::new("https://example.com/x", "www.example.com", ResourceTypes::XHR))
            .is_none());
    }

    #[test]
    fn a_domain_option_restricts_a_rule_to_a_site() {
        let filters = list("||tracker.test^$domain=shop.test\n");
        // On the named site: applies.
        assert!(filters
            .verdict(Request::new("https://tracker.test/x", "shop.test", ResourceTypes::SCRIPT))
            .is_some());
        // On a subdomain of it: applies, because the option is a suffix match.
        assert!(filters
            .verdict(Request::new("https://tracker.test/x", "eu.shop.test", ResourceTypes::SCRIPT))
            .is_some());
        // Anywhere else: does not.
        assert!(filters
            .verdict(Request::new("https://tracker.test/x", "other.test", ResourceTypes::SCRIPT))
            .is_none());
    }

    #[test]
    fn a_pattern_with_a_wildcard_matches_across_it() {
        let filters = list("/ads/*/banner.\n");
        assert!(blocks(
            &filters,
            "https://x.test/ads/300x250/banner.png",
            "x.test"
        ));
        assert!(blocks(&filters, "https://x.test/ads/a/b/banner.png", "x.test"));
        assert!(!blocks(&filters, "https://x.test/ads/banner.png", "x.test"));
    }

    #[test]
    fn a_separator_matches_a_slash_a_colon_or_the_end() {
        let filters = list("/adserv^\n");
        assert!(blocks(&filters, "https://x.test/adserv/", "x.test"));
        assert!(blocks(&filters, "https://x.test/adserv?q=1", "x.test"));
        // The end of the URL satisfies `^` too, which is what makes
        // `||example.com^` match a bare address with nothing after it.
        assert!(blocks(&filters, "https://x.test/adserv", "x.test"));
        // A letter is not a separator, so this is a different word.
        assert!(!blocks(&filters, "https://x.test/adserving/thing", "x.test"));
    }

    #[test]
    fn a_dot_is_not_a_separator_and_that_is_the_point() {
        // The assertion that has to be exactly right, and the reason is worth
        // recording because it looks like an oversight from the outside.
        //
        // `^` exists to say "the pattern ends at a word boundary". A dot is
        // **not** a boundary, and a filter list depends on that: if it were,
        // `||example.com^` would match `https://example.com.evil.test/`, which
        // is precisely the lookalike the anchor is there to exclude. The
        // characters that do not separate are a letter, a digit, `_`, `-`, `.`
        // and `%` — the same set Adblock Plus and uBlock Origin use, because the
        // lists are written against it.
        let filters = list("/adserv^\n");
        assert!(!blocks(&filters, "https://x.test/adserv.js", "x.test"));

        // And the case it protects, spelled out: the trailing `^` in a host rule
        // must not let a longer domain through.
        let host = list("||example.com^\n");
        assert!(blocks(&host, "https://example.com/", "other.test"));
        assert!(blocks(&host, "https://example.com:8443/x", "other.test"));
        assert!(blocks(&host, "https://example.com", "other.test"));
        assert!(!blocks(&host, "https://example.com.evil.test/x", "other.test"));

        // The set, one character at a time, so a future edit to `is_separator`
        // fails here rather than in a browser.
        for (byte, separates) in [
            (b'/', true),
            (b':', true),
            (b'?', true),
            (b'&', true),
            (b'.', false),
            (b'-', false),
            (b'_', false),
            (b'%', false),
            (b'a', false),
            (b'7', false),
        ] {
            assert_eq!(is_separator(byte), separates, "{}", byte as char);
        }
    }

    #[test]
    fn anchored_patterns_respect_their_anchor() {
        let prefix = list("|https://exact.test/ad\n");
        assert!(blocks(&prefix, "https://exact.test/ad", "x.test"));
        assert!(!blocks(&prefix, "https://other.test/exact.test/ad", "x.test"));

        let suffix = list("/track.js|\n");
        assert!(blocks(&suffix, "https://x.test/a/track.js", "x.test"));
        assert!(!blocks(&suffix, "https://x.test/a/track.js?v=2", "x.test"));
    }

    #[test]
    fn cosmetic_rules_become_css_for_the_right_host() {
        let filters = list(
            "example.com##.ad-banner\n\
             ##.sponsored\n\
             example.com#@#.sponsored\n",
        );
        let css = filters.cosmetic_css("example.com");
        assert!(css.contains(".ad-banner"), "{css}");
        // The generic rule applies, but this host explicitly allowed it.
        assert!(!css.contains(".sponsored"), "{css}");
        // `:not(body)` is what stops a bad generic rule blanking a page.
        assert!(css.contains(":not(body)"), "{css}");
        assert!(css.contains("display: none !important"), "{css}");

        // A different host gets the generic rule and not the domain one.
        let other = filters.cosmetic_css("other.test");
        assert!(!other.contains(".ad-banner"), "{other}");
        assert!(other.contains(".sponsored"), "{other}");
    }

    #[test]
    fn a_cosmetic_rule_for_a_parent_domain_covers_its_subdomains() {
        let filters = list("example.com##.ad\n");
        assert!(filters.cosmetic_css("example.com").contains(".ad"));
        assert!(filters.cosmetic_css("www.example.com").contains(".ad"));
        assert!(filters.cosmetic_css("news.www.example.com").contains(".ad"));
        assert!(!filters.cosmetic_css("notexample.com").contains(".ad"));
    }



    #[test]
    fn unimplementable_options_drop_the_rule_instead_of_softening_it() {
        // A `$redirect` treated as a plain block would block a resource the list
        // meant to *replace*, which is how a blocker breaks a site. Dropping it
        // is the honest failure, and the stat says it happened.
        let filters = list("||example.com/x$redirect=noop.js\n||other.test/y\n");
        assert_eq!(filters.stats().unsupported, 1);
        assert!(!blocks(&filters, "https://example.com/x", "d.test"));
        assert!(blocks(&filters, "https://other.test/y", "d.test"));
    }

    #[test]
    fn an_unknown_option_keeps_the_rule_working() {
        // A suffix this build has not heard of should not disarm the rule: the
        // list author's intent was to block, and blocking less is the safer way
        // to be wrong.
        let filters = list("||example.com/x$some-new-option\n");
        assert!(blocks(&filters, "https://example.com/x", "d.test"));
    }

    #[test]
    fn comments_and_headers_are_ignored_and_counted() {
        let filters = list("! a comment\n[Adblock Plus 2.0]\n\n||x.test^\n");
        assert_eq!(filters.stats().rules, 1);
        assert_eq!(filters.stats().ignored, 3);
    }

    #[test]
    fn a_url_without_a_scheme_is_handled() {
        let filters = list("||ads.test^\n");
        assert!(blocks(&filters, "//ads.test/x", "d.test"));
        assert!(blocks(&filters, "ads.test/x", "d.test"));
    }

    #[test]
    fn host_of_strips_userinfo_and_a_port() {
        assert_eq!(host_of("https://user:pw@example.com:8443/x"), "example.com");
        assert_eq!(host_of("https://example.com/x?y=1#z"), "example.com");
        assert_eq!(host_of("https://example.com"), "example.com");
        // IPv6 keeps its colons.
        assert_eq!(host_of("http://[::1]:8080/x"), "[::1]:8080");
    }

    #[test]
    fn the_token_index_does_not_change_the_answer() {
        // The index is an optimisation, and an optimisation that changes a
        // verdict is a bug. This builds the same list with and without tokens
        // available and compares the decisions.
        let text = "||ads.test^\n/tracker.js\nsponsor\n@@||ads.test/ok\n";
        let indexed = list(text);
        let mut linear = list(text);
        // Force everything into the always-tested bucket.
        linear.index.clear();
        linear.unindexed = (0..linear.rules.len() as u32).collect();

        for url in [
            "https://ads.test/x",
            "https://ads.test/ok",
            "https://x.test/tracker.js",
            "https://x.test/sponsor/thing",
            "https://x.test/clean",
        ] {
            let a = indexed.verdict(Request::new(url, "d.test", ResourceTypes::SCRIPT));
            let b = linear.verdict(Request::new(url, "d.test", ResourceTypes::SCRIPT));
            assert_eq!(a, b, "{url}");
        }
    }

    #[test]
    fn an_empty_set_is_recognised_so_nothing_is_installed_for_it() {
        // An unconfigured browser should be exactly as fast as one with no
        // blocker, which means not registering a request filter at all.
        assert!(Filters::new().is_empty());
        assert!(!list("||x.test^\n").is_empty());
        assert!(!list("##.ad\n").is_empty());
    }

    #[test]
    fn a_realistic_list_parses_without_dropping_much() {
        // The shape of an EasyList excerpt, including the awkward bits.
        let filters = list(
            "! EasyList excerpt\n\
             [Adblock Plus 2.0]\n\
             ||doubleclick.net^\n\
             ||googlesyndication.com^\n\
             ||google-analytics.com/analytics.js\n\
             @@||googlesyndication.com/pagead/js/adsbygoogle.js$script\n\
             /pagead/*/show_ads.\n\
             $third-party\n\
             ||adservice.google.*^\n\
             ##.ad-container\n\
             example.com##.promo\n\
             ||tracker.test^$domain=example.com|shop.test\n\
             /banner_ad.\n\
             $image\n",
        );
        // The stray `$third-party` line has no pattern, so it is not a rule.
        assert!(filters.stats().rules >= 7, "{:?}", filters.stats());
        assert!(blocks(&filters, "https://doubleclick.net/x.gif", "news.test"));
        assert!(blocks(&filters, "https://www.googlesyndication.com/x", "news.test"));
        // The exception is for a script, and this is a script.
        assert_eq!(
            filters
                .verdict(Request::new(
                    "https://googlesyndication.com/pagead/js/adsbygoogle.js",
                    "news.test",
                    ResourceTypes::SCRIPT
                ))
                .map(|found| found.verdict),
            Some(Verdict::Allow)
        );
        assert!(filters.cosmetic_css("news.test").contains(".ad-container"));
    }

    #[test]
    fn a_wildcard_inside_the_authority_matches_across_a_label() {
        // `||adservice.google.*^` is a real EasyList shape, and no fixed host
        // can express it — the authority's own last label varies by country.
        let filters = list("||adservice.google.*^\n");
        for url in [
            "https://adservice.google.com/x",
            "https://adservice.google.co.uk/x",
            "https://adservice.google.de/x",
            "https://sub.adservice.google.com/x",
        ] {
            assert!(blocks(&filters, url, "news.test"), "{url}");
        }
        // Still anchored at a boundary, so it cannot match mid-label. This is
        // the assertion that matters: an over-broad blocker breaks sites.
        assert!(!blocks(&filters, "https://notadservice.google.com/x", "news.test"));
        // A wildcard *does* cross a dot, because that is what `*` means in a
        // filter pattern — `google.*` is `google` then anything. Asserted rather
        // than wished away, since it is the same rule that makes
        // `google.co.uk` work.
        assert!(blocks(&filters, "https://adservice.google.com.evil.test/x", "news.test"));
    }

    #[test]
    fn a_filter_rule_is_never_mistaken_for_a_bare_hostname() {
        // The bug this guards, and it is the subtle one: a filter rule with no
        // options is also "a line with no space and a dot in it", so a hosts
        // parser that accepts anything of that shape swallows `||ads.test^` as
        // the hostname `||ads.test^` — a rule that matches nothing, silently.
        for line in [
            "||ads.test^",
            "||ads.test/path",
            "example.com##.ad",
            "/banner_ad.",
            "|https://exact.test/ad",
            "||x.test^$script",
            "text-with-dots.test/x$third-party",
        ] {
            assert!(hosts_line(line).is_none(), "{line} was taken for a hostname");
        }
        // And the real hosts arms still work.
        assert_eq!(hosts_line("0.0.0.0 ads.test"), Some("ads.test"));
        assert_eq!(hosts_line("127.0.0.1 ads.test"), Some("ads.test"));
        assert_eq!(hosts_line("::1 ads.test"), Some("ads.test"));
        assert_eq!(hosts_line("ads.test"), Some("ads.test"));
        assert_eq!(hosts_line("ads.test."), Some("ads.test"));
    }

    #[test]
    fn stats_add_up() {
        let filters = list("! c\n||a.test^\n||b.test^\n##.x\n");
        let stats = filters.stats();
        // Two network rules and one cosmetic. A cosmetic rule is counted as
        // cosmetic rather than as a rule, because it is not one: it has no URL to
        // match, and folding the two together would make the number the settings
        // page shows mean nothing in particular.
        assert_eq!(stats.rules, 2, "{stats:?}");
        assert_eq!(stats.cosmetic, 1, "{stats:?}");
        assert_eq!(stats.ignored, 1, "{stats:?}");
        assert_eq!(stats.lists, 1);
        assert_eq!(stats.unsupported, 0, "{stats:?}");
    }

    #[test]
    fn a_cosmetic_exception_is_not_counted_as_a_cosmetic_rule() {
        // `#@#` names a rule that already exists, so counting it would make the
        // reported total depend on how many times a list says "except here".
        let filters = list("example.com##.ad\nexample.com#@#.ad\n##.generic\n");
        assert_eq!(filters.stats().cosmetic, 2, "{:?}", filters.stats());
        // And the exception still does its job.
        assert!(!filters.cosmetic_css("example.com").contains(".ad"));
        assert!(filters.cosmetic_css("example.com").contains(".generic"));
    }

    #[test]
    fn procedural_cosmetic_rules_are_skipped_rather_than_misapplied() {
        // uBlock's extended syntax is not CSS. Injecting one would make the whole
        // stylesheet invalid, so the page would lose the ordinary hiding it should
        // have had as well.
        let filters = list(
            "example.com##div:has-text(Sponsored)\n\
             other.test##p:matches-css(display: none)\n\
             third.test##.fine\n",
        );
        let css = filters.cosmetic_css("example.com");
        assert!(!css.contains("has-text"), "{css}");
        let css = filters.cosmetic_css("other.test");
        assert!(!css.contains("matches-css"), "{css}");
        // A plain selector on another host is unaffected.
        assert!(filters.cosmetic_css("third.test").contains(".fine"));
        // And the skipped ones are counted, so the settings page can say how much
        // of a list Loom does not implement rather than implying it does.
        assert_eq!(filters.stats().unsupported, 2, "{:?}", filters.stats());
    }

    #[test]
    fn matching_a_host_rule_against_a_lookalike_domain_fails() {
        // The classic over-block: `||ads.test^` must not match `notads.test`,
        // and a naive `contains` would.
        let filters = list("||ads.test^\n");
        assert!(!blocks(&filters, "https://notads.test/x", "d.test"));
        assert!(!blocks(&filters, "https://ads.test.evil.test/x", "d.test"));
        assert!(blocks(&filters, "https://ads.test/x", "d.test"));
    }
}
