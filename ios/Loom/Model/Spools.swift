import Foundation

// Spools, the integrations catalog (PLAN.md, "The integrations catalog"), as
// desktop/src/model/spools.rs has it: its entries as `catalog.list` gives them; which of
// them Find shows for the pill picked and what's typed, with the MCP Registry's search;
// an entry's detail and the steps of connecting one, as the instance's `connect` frames
// move them; and the person's servers as Installed lists them, with how each stands. The
// screens only draw it.

/// The entries Find shows first, as two wide cards: loom's own.
let featuredEntries = ["acta", "t3"]

/// The category pills after All, in their order. A category the catalog has that isn't
/// one of them comes after them.
let spoolPills = ["Work", "Code", "Mail & calendar", "Files", "Home", "Fun"]

/// The entries whose sign-in refuses the iPhone's `loom:/callback` redirect, which the
/// person connects from a computer: T3 takes only loopback and https redirects (LOO-344).
let connectedFromAComputer = ["t3"]

/// How few letters the MCP Registry is searched for.
let registryShortest = 3

/// How many tools a detail shows before Show all.
let toolsShownMost = 8

/// How one of the person's MCP servers stands, as `mcp.list` says.
enum McpStatus: Equatable, Sendable {
    case connected, signIn, failing, untested
}

/// Whether one of the person's MCP servers signs in, and as whom once it has.
enum McpSignIn: Equatable, Sendable {
    case no, needed
    case signedIn(String)
}

/// One of the person's MCP servers, as `mcp.list` gives it.
struct McpServer: Equatable, Sendable, Identifiable {
    var name: String
    var url = ""
    var command = ""
    var signIn = McpSignIn.no
    var status = McpStatus.untested
    var why = ""
    /// The tools its last test found, while it's connected.
    var tools: [String] = []
    /// The ids of the person's agents that have it.
    var agents: [String] = []
    /// The id of the catalog entry it came from, or whose fixed address it has.
    var catalog = ""
    var badge: Badge?
    /// The name of its mark as the app has it built in (`mark-composio`): its entry's,
    /// or the one loom knows it by, by its host or its name.
    var mark = ""
    /// Its settings as its form shows them (`settingsText`), and the client of the
    /// person's own it signs in with, for one over HTTP.
    var settings = ""
    var clientId = ""

    var id: String { name }

    init(name: String, catalog: String = "", status: McpStatus = .untested) {
        self.name = name
        self.catalog = catalog
        self.status = status
    }

    init(_ s: JSON) {
        name = s["name"].text
        url = s["url"].text
        command = ([s["command"].text] + s["args"].array.compactMap(\.string)).filter { !$0.isEmpty }.joined(separator: " ")
        switch s["sign_in"].text {
        case "needed": signIn = .needed
        case "signed_in": signIn = .signedIn(s["signed_in_as"].text)
        default: signIn = .no
        }
        switch s["status"].text {
        case "connected": status = .connected
        case "sign_in": status = .signIn
        case "failing": status = .failing
        default: status = .untested
        }
        why = s["why"].text
        tools = s["tools"].array.compactMap(\.string)
        agents = s["agents"].array.compactMap(\.string)
        catalog = s["catalog"].text
        badge = Badge(s["badge"])
        mark = s["mark"].text
        settings = settingsText(s["env"].isNull ? s["headers"] : s["env"])
        clientId = s["client_id"].text
    }
}

/// A server's `env` or `headers` as its form shows them, as desktop/src/model.rs's
/// `settings_text`: one `NAME=value` or `NAME=secret:KEY` a line, a secret's prefix before
/// it, as `Authorization=Bearer secret:GITHUB_TOKEN`, by name.
func settingsText(_ pairs: JSON) -> String {
    guard case .object(let o) = pairs else { return "" }
    return o.keys.sorted().map { k in
        let v = o[k] ?? .null
        if let secret = v["secret"].string { return "\(k)=\(v["prefix"].text)secret:\(secret)" }
        return "\(k)=\(v.text)"
    }.joined(separator: "\n")
}

/// A setting's value as the form has it, as `setting_value` in desktop/src/model.rs:
/// `secret:KEY`, with any prefix before it ending in a space (`Bearer secret:KEY`), names
/// one of the person's secrets, whose name is letters, digits, `_`, `-` and `.`; anything
/// else is text.
func settingValue(_ v: String) -> JSON {
    if let at = v.range(of: "secret:", options: .backwards) {
        let prefix = String(v[..<at.lowerBound])
        let name = v[at.upperBound...].trimmingCharacters(in: .whitespaces)
        let named = !name.isEmpty && name.allSatisfy { $0.isASCII && ($0.isLetter || $0.isNumber || "_-.".contains($0)) }
        if named, prefix.isEmpty || prefix.last?.isWhitespace == true {
            return prefix.isEmpty ? ["secret": JSON(name)] : ["secret": JSON(name), "prefix": JSON(prefix)]
        }
    }
    return JSON(v)
}

/// The `mcp.put` arguments for a server added or changed by hand, as `server_args` and
/// `with_client` in desktop/src/model.rs: a command line, or an http(s) address, its
/// settings, one `NAME=value` or `NAME=secret:KEY` a line, and for one over HTTP a client
/// of the person's own, whose secret left empty stays as it was. `was` is the name it's
/// saved under, so a new name renames it.
func serverArgs(name: String, line: String, settings: String, clientId: String = "", clientSecret: String = "", was: String? = nil) -> [String: JSON] {
    var pairs: [String: JSON] = [:]
    for l in settings.split(whereSeparator: \.isNewline) {
        guard let eq = l.firstIndex(of: "=") else { continue }
        let k = l[..<eq].trimmingCharacters(in: .whitespaces)
        let v = l[l.index(after: eq)...].trimmingCharacters(in: .whitespaces)
        if k.isEmpty { continue }
        pairs[k] = settingValue(v)
    }
    let line = line.trimmingCharacters(in: .whitespaces)
    let name = name.trimmingCharacters(in: .whitespaces)
    var args: [String: JSON]
    if line.hasPrefix("http://") || line.hasPrefix("https://") {
        args = ["name": JSON(name), "url": JSON(line), "headers": .object(pairs)]
        let (id, secret) = (clientId.trimmingCharacters(in: .whitespaces), clientSecret.trimmingCharacters(in: .whitespaces))
        if !id.isEmpty {
            args["client_id"] = JSON(id)
            if !secret.isEmpty {
                args["client_secret"] = secret.hasPrefix("secret:")
                    ? ["secret": JSON(String(secret.dropFirst("secret:".count)).trimmingCharacters(in: .whitespaces))]
                    : JSON(secret)
            }
        }
    } else {
        let words = line.split(whereSeparator: \.isWhitespace).map(String.init)
        args = ["name": JSON(name), "command": JSON(words.first ?? ""), "args": JSON(Array(words.dropFirst())), "env": .object(pairs)]
    }
    if let was { args["was"] = JSON(was) }
    return args
}

/// A field an entry asks the person to fill in, such as T3's address. One from the
/// registry may have a default, used where it's left empty.
struct EntryField: Equatable, Sendable {
    var name: String
    var label: String
    var placeholder: String
    var defaultValue: String
}

/// A secret an entry asks for, and whether the person has one by its name already. One
/// from the registry may be optional.
struct EntryKey: Equatable, Sendable {
    var name: String
    var label: String
    var placeholder: String
    var have: Bool
    var optional: Bool
}

/// The first of `texts` that isn't empty.
private func firstOf(_ texts: [String]) -> String {
    texts.first { !$0.trimmingCharacters(in: .whitespaces).isEmpty } ?? ""
}

/// One of the catalog's entries, or one of the MCP Registry's from a search.
struct Entry: Equatable, Sendable, Identifiable {
    var id: String
    var name = ""
    var category = ""
    var description = ""
    var maker = ""
    var badge: Badge?
    /// Its mark, an SVG in currentColor.
    var iconSvg = ""
    /// Its mark's name, as the app has it built in (`mark`).
    var markName = ""
    /// The name its server is added as, unless the person gives another.
    var server = ""
    /// Reached over HTTP, or a program the instance starts.
    var remote = true
    var url = ""
    var command = ""
    var fields: [EntryField] = []
    /// `oauth`, `secret`, `unknown` or `none`.
    var signIn = ""
    var secrets: [EntryKey] = []
    /// One line, only where the person would go wrong without it.
    var note = ""
    /// Its tools, each with whether it changes something rather than only reading.
    var tools: [(name: String, changes: Bool)] = []
    var toolsChecked = false
    /// The person's servers made from it.
    var servers: [String] = []
    /// One of the MCP Registry's, by its name there and its version.
    var community = false
    var registry = ""
    var version = ""
    /// Why loom can't connect it, for one from the registry it can't.
    var unsupported = ""

    init(id: String) { self.id = id }

    init(_ v: JSON) {
        id = v["id"].text
        name = v["name"].text
        category = v["category"].text
        description = v["description"].text
        maker = v["maker"].text
        badge = Badge(v["badge"])
        iconSvg = v["icon_svg"].text
        server = v["server"].text
        remote = v["runs"].text != "instance"
        url = v["url"].text
        command = ([v["command"].text] + v["args"].array.compactMap(\.string)).filter { !$0.isEmpty }.joined(separator: " ")
        // A registry server's field or secret is labelled by its own name, so what it's
        // for, as the registry says, is its placeholder where it has none.
        fields = v["fields"].array.map { EntryField(name: $0["name"].text, label: $0["label"].text, placeholder: firstOf([$0["placeholder"].text, $0["default"].text, $0["description"].text]), defaultValue: $0["default"].text) }
        signIn = v["sign_in"].text
        secrets = v["secrets"].array.map { EntryKey(name: $0["name"].text, label: $0["label"].text, placeholder: firstOf([$0["placeholder"].text, $0["description"].text]), have: $0["have"].bool ?? false, optional: $0["optional"].bool ?? false) }
        note = v["note"].text
        tools = v["tools"].array.map { (name: $0["name"].text, changes: $0["kind"].text == "change") }
        toolsChecked = v["tools_checked"].bool ?? false
        servers = v["servers"].array.compactMap(\.string)
        community = v["community"].bool ?? false
        registry = v["registry"].text
        version = v["version"].text
        unsupported = v["unsupported"].text
        markName = v["mark"].text
    }

    /// The mark the app has for it, by the name the instance gives, as scripts/ios.calx
    /// bundles server/catalog's (`mark-composio`): its own, or the one loom knows it by,
    /// for a registry server too. One whose only mark is its own SVG isn't bundled, so it
    /// has its initial.
    var mark: String? {
        markName.isEmpty ? nil : markName
    }

    static func == (a: Entry, b: Entry) -> Bool {
        a.id == b.id && a.name == b.name && a.servers == b.servers && a.tools.map(\.name) == b.tools.map(\.name) && a.secrets == b.secrets && a.badge == b.badge && a.unsupported == b.unsupported
    }

    /// One of the two Find shows first, as a wide card.
    var featured: Bool { featuredEntries.contains(id) }

    var connected: Bool { !servers.isEmpty }

    /// Whether its tile on Find is filled in ink (LOO-436): once the person has a server
    /// from it, however that stands; outlined until then.
    var tileFilled: Bool { connected }

    /// Whether the person can connect more than one: a curated entry reached at an
    /// address they give, such as T3 on each of their computers.
    var several: Bool { !community && !fields.isEmpty }

    /// Whether connecting asks for something first: a field, or a secret the person
    /// doesn't have. Another of one they have always asks, for its address.
    func asks(another: Bool) -> Bool {
        another || !fields.isEmpty || secrets.contains { !$0.have }
    }

    /// Whether it can be connected from the phone: not one whose sign-in refuses the
    /// phone's redirect, and not one loom can't connect at all.
    var connectsHere: Bool { !connectedFromAComputer.contains(id) && unsupported.isEmpty }

    /// Where it runs, in words: the host it's reached at, the address the person gives,
    /// or the program it is on their server.
    var runsWords: String {
        if !remote {
            return "On your server, with \(command.split(separator: " ").first.map(String.init) ?? "its program")"
        }
        if url.contains("{") { return "At the address you give" }
        return hostOf(url)
    }

    /// How it signs in, in words.
    var signInWords: String {
        switch signIn {
        case "oauth": return "With your \(name) account"
        case "secret" where community: return "With a key"
        case "secret":
            guard let k = secrets.first, !k.label.isEmpty else { return "With a key" }
            let label = k.label.prefix(1).lowercased() + k.label.dropFirst()
            return "With \("aeiou".contains(label.prefix(1)) ? "an" : "a") \(label)"
        case "unknown": return "Known once it's connected"
        default: return "No sign-in"
        }
    }

    /// How `catalog.connect` names it: a curated entry by its id, one from the registry by
    /// its name there and its version.
    var connectArgs: [String: JSON] {
        community ? ["registry": JSON(registry), "version": JSON(version)] : ["id": JSON(id)]
    }
}

/// The host of an address, as `https://acta.aaenz.no/api/mcp` is `acta.aaenz.no`.
func hostOf(_ url: String) -> String {
    let rest = url.range(of: "://").map { url[$0.upperBound...] } ?? Substring(url)
    return String(rest.split(whereSeparator: { "/?#".contains($0) }).first ?? "")
}

/// A step of connecting one, as the instance's `connect` frames say it.
enum ConnectStep: Equatable, Sendable {
    case adding, signIn, agents, failed, done

    init(_ step: String) {
        switch step {
        case "sign_in": self = .signIn
        case "agents": self = .agents
        case "failed": self = .failed
        case "done": self = .done
        default: self = .adding
        }
    }
}

/// A connect under way: the entry, the server it's adding, its step, why it failed or a
/// sign-in didn't work, and the tools it found.
struct Connect: Equatable, Sendable {
    var entry: String
    var server: String
    var step: ConnectStep
    var said: String
    var tools: [String]

    init?(_ v: JSON) {
        server = v["server"].text
        guard !server.isEmpty else { return nil }
        entry = v["entry"].text
        step = ConnectStep(v["step"].text)
        said = v["said"].text
        tools = v["tools"].array.compactMap(\.string)
    }

    init(entry: String, server: String, step: ConnectStep) {
        self.entry = entry
        self.server = server
        self.step = step
        said = ""
        tools = []
    }
}

/// Where an entry's detail is.
enum SpoolStage: Equatable, Sendable {
    /// What it is, with Connect, or how its servers stand once it has some.
    case about
    /// Its fields and secrets, to fill in before it's connected.
    case form
    /// Connecting, at this step, as the instance last said.
    case going(ConnectStep)
}

/// The entry whose detail sheet is open, and how far connecting it is.
struct SpoolFlow: Equatable, Sendable, Identifiable {
    var entry: String
    /// Another server from an entry the person has one of already.
    var another = false
    /// The server it adds, once the instance has said.
    var server = ""
    var stage = SpoolStage.about
    /// What went wrong, in one line: a field left out, or the instance's why.
    var said = ""
    /// The agents picked to get it, at its agents step.
    var agents: [String] = []
    /// Every tool shown, after Show all.
    var allTools = false

    var id: String { entry }
}

/// The tools a detail shows, by their place in `tools`: every one after Show all, or for
/// an entry with more, the first `toolsShownMost`, as many that change something as that
/// only read where it has enough of both, in the catalog's order.
func toolsShown(_ tools: [(name: String, changes: Bool)], all: Bool) -> [Int] {
    guard !all, tools.count > toolsShownMost else { return Array(tools.indices) }
    let reads = tools.indices.filter { !tools[$0].changes }
    let changes = tools.indices.filter { tools[$0].changes }
    let half = toolsShownMost / 2
    let r = changes.count < half ? toolsShownMost - changes.count : min(half, reads.count)
    return (Array(reads.prefix(r)) + Array(changes.prefix(min(toolsShownMost - r, changes.count)))).sorted()
}

/// What can be done with one of the person's servers, from Installed.
enum ServerAct: Equatable, Sendable {
    case signIn
    /// Tries one not tried yet (`mcp.test`).
    case tryIt
    case reconnect, disconnect, remove

    var label: String {
        switch self {
        case .signIn: "Sign in"
        case .tryIt: "Try it"
        case .reconnect: "Reconnect"
        case .disconnect: "Disconnect"
        case .remove: "Remove"
        }
    }
}

/// The dot of how a server stands.
enum StandingDot: Equatable, Sendable {
    case ok, warn, bad, quiet
}

/// One of the person's servers as Installed lists it: the server, the entry it came from,
/// its name, the line under it, and its dot.
struct InstalledRow: Equatable, Sendable, Identifiable {
    var server: McpServer
    var entry: Entry?
    var title: String
    var line: String
    var dot: StandingDot

    var id: String { server.name }

    /// Its mark, as the app has it built in: the server's, else its entry's.
    var mark: String? {
        server.mark.isEmpty ? entry?.mark : server.mark
    }

    /// Whether its logo's box is filled in ink, as it is once it's connected (LOO-436).
    var filled: Bool { dot == .ok }
}

/// A search of the MCP Registry (`catalog.search`): what it's for, the command asked, and
/// once answered, the registry's servers, the curated entries it found, or why the
/// registry couldn't say.
struct RegistrySearch: Equatable, Sendable {
    var query = ""
    fileprivate var asked = ""
    var answered = false
    var entries: [Entry] = []
    var curated: [String] = []
    var error: String?
}

/// What Find shows: loom's own that match as cards, the rest of the catalog's that match
/// and then the registry's as tiles, and under them the search's state.
struct SpoolsShown: Equatable, Sendable {
    var cards: [Entry] = []
    var tiles: [Entry] = []
    /// Searching…, or why the registry couldn't be searched.
    var line: String?
    /// Nothing matches, the registry included.
    var nothing = false
}

/// Whether every word of `query` is in `text`, in any case.
func finds(_ query: String, _ text: String) -> Bool {
    let text = text.lowercased()
    return query.lowercased().split(whereSeparator: \.isWhitespace).allSatisfy { text.contains($0) }
}

/// The pills, All aside: the decided ones, then any other category the catalog has.
func pillsOf(_ categories: [String]) -> [String] {
    spoolPills + categories.filter { !spoolPills.contains($0) }
}

/// What Find shows of the catalog for the pill picked (none is All) and what's typed: the
/// featured entries that match, as cards, and the rest that match, as tiles. `also` are
/// the ids of those the registry's search found too.
func found(_ entries: [Entry], pill: String?, query: String, also: [String]) -> (cards: [Entry], tiles: [Entry]) {
    var cards: [Entry] = [], tiles: [Entry] = []
    for e in entries {
        if let pill, pill != e.category { continue }
        guard finds(query, "\(e.name) \(e.description) \(e.category) \(e.maker)") || also.contains(e.id) else { continue }
        if e.featured { cards.append(e) } else { tiles.append(e) }
    }
    return (cards, tiles)
}

/// How one of the person's servers stands, in words.
func statusWords(_ m: McpServer) -> String {
    switch m.status {
    case .connected:
        if case .signedIn(let who) = m.signIn, !who.isEmpty { return "Connected as \(who)" }
        return "Connected"
    case .signIn: return "Needs you to sign in"
    case .failing: return m.why.isEmpty ? "Not working" : m.why
    case .untested: return "Not tried yet"
    }
}

/// How it stands, as its detail says it: with how many tools it offers, once connected.
func statusLine(_ m: McpServer) -> String {
    let words = statusWords(m)
    guard m.status == .connected, !m.tools.isEmpty else { return words }
    return m.tools.count == 1 ? "\(words) · 1 tool" : "\(words) · \(m.tools.count) tools"
}

/// The dot of how it stands: ok, warn for one that needs a sign-in, bad for one failing,
/// and quiet before it's been tried.
func statusDot(_ m: McpServer) -> StandingDot {
    switch m.status {
    case .connected: .ok
    case .signIn: .warn
    case .failing: .bad
    case .untested: .quiet
    }
}

/// How an entry's card stands: none until the person has a server from it, then the
/// worst of how its servers stand: failing, needing a sign-in, not tried yet, or else
/// connected, with how many when there are several. Its thread is taut only while
/// it's connected.
func cardStatus(_ entry: Entry, _ servers: [McpServer]) -> (words: String, dot: StandingDot)? {
    let mine = servers.filter { entry.servers.contains($0.name) }
    guard !mine.isEmpty else { return nil }
    for worst in [McpStatus.failing, .signIn, .untested] {
        if let m = mine.first(where: { $0.status == worst }) { return (statusWords(m), statusDot(m)) }
    }
    // One there can be several of says how many, such as two T3s.
    if mine.count > 1 { return ("\(mine.count) connected", .ok) }
    return ("Connected", .ok)
}

/// What can be done with it: Sign in where it needs one, Try it before it's been tried
/// and Reconnect once it has, Disconnect where it's signed in, and Remove.
func serverActs(_ m: McpServer) -> [ServerAct] {
    var out: [ServerAct] = []
    if m.status == .signIn || m.signIn == .needed { out.append(.signIn) }
    out.append(m.status == .untested ? .tryIt : .reconnect)
    if case .signedIn = m.signIn { out.append(.disconnect) }
    out.append(.remove)
    return out
}

/// The person's servers as Installed lists them: those from the catalog in its order,
/// each entry's by name, then the rest by name. Each is its entry's name, or its own; the
/// line under it is the host, for an entry with several (T3 on each computer), or else how
/// it stands; a connect under way says so.
func installed(_ servers: [McpServer], _ entries: [Entry], _ connects: [String: Connect]) -> [InstalledRow] {
    let rows = servers.map { m -> (Int, InstalledRow) in
        let at = m.catalog.isEmpty ? nil : entries.firstIndex { $0.id == m.catalog }
        let e = at.map { entries[$0] }
        let going = connects[m.name]?.step == .adding
        let line = going ? "Connecting…" : e?.several == true ? hostOf(m.url) : statusWords(m)
        return (at ?? .max, InstalledRow(server: m, entry: e, title: e?.name ?? m.name, line: line, dot: going ? .quiet : statusDot(m)))
    }
    return rows.sorted { ($0.0, $0.1.server.name) < ($1.0, $1.1.server.name) }.map(\.1)
}

/// The line under a server's title in its detail: its own name only where it isn't the
/// one its entry gives (`t3-vps-2`, not `acta`), and its host where the person gave it
/// (T3's) or added it by hand.
func installedUnder(_ m: McpServer, _ entry: Entry?) -> String {
    var parts: [String] = []
    if let e = entry {
        if m.name != e.server { parts.append(m.name) }
        if e.several { parts.append(hostOf(m.url)) }
    } else {
        parts.append(hostOf(m.url))
    }
    return parts.filter { !$0.isEmpty }.joined(separator: " · ")
}

/// The name of another server from `entry`, reached at `address`, for an entry the
/// person has one of already: its server's name and the first part of the host, as
/// `t3-vps` for `https://vps.tail1234.ts.net`, with a number after it where that's taken.
func anotherName(_ entry: Entry, address: String, taken: [String]) -> String {
    let first = hostOf(address.trimmingCharacters(in: .whitespaces)).split(whereSeparator: { $0 == "." || $0 == ":" }).first.map(String.init)?.lowercased() ?? ""
    let label = String(first.map { $0.isASCII && ($0.isLetter || $0.isNumber) ? $0 : "-" }).trimmingCharacters(in: CharacterSet(charactersIn: "-"))
    let base = label.isEmpty ? "\(entry.server)-2" : "\(entry.server)-\(label)"
    var name = base
    var n = 2
    while taken.contains(name) {
        name = "\(base)-\(n)"
        n += 1
    }
    return name
}

/// What's missing from a form before it can connect: the label of the first field
/// without a default, or of a secret the person doesn't have and isn't optional, left
/// empty.
func missingInForm(_ entry: Entry, fields: [String: String], secrets: [String: String]) -> String? {
    func given(_ list: [String: String], _ name: String) -> Bool {
        !(list[name] ?? "").trimmingCharacters(in: .whitespaces).isEmpty
    }
    if let f = entry.fields.first(where: { $0.defaultValue.isEmpty && !given(fields, $0.name) }) { return "Fill in \(f.label)" }
    if let k = entry.secrets.first(where: { !$0.have && !$0.optional && !given(secrets, $0.name) }) { return "Fill in \(k.label)" }
    return nil
}

/// The agents a server goes to with `agent` turned on or off.
func toggled(_ agents: [String], _ agent: String) -> [String] {
    var out = agents
    if let i = out.firstIndex(of: agent) { out.remove(at: i) } else { out.append(agent) }
    return out
}

/// Spools as the model keeps it.
struct SpoolsState {
    var entries: [Entry] = []
    var categories: [String] = []
    /// Whether the catalog has come, and why it couldn't, from an instance without it.
    var came = false
    var error: String?
    /// The category pill picked; none is All.
    var pill: String?
    /// The entry whose detail sheet is open.
    var flow: SpoolFlow?
    /// Each connect under way, by the server it adds.
    var connects: [String: Connect] = [:]
    /// Servers connected again by Reconnect: their agents step is answered with the
    /// agents they had.
    fileprivate var reconnecting: Set<String> = []
    /// The servers being tried, until the instance answers.
    var trying: Set<String> = []
    /// The search of the MCP Registry for what's typed, once it's long enough.
    var search = RegistrySearch()
}

extension Model {
    /// Spools opens: asks for the catalog and the person's servers, which are kept up to
    /// date from then on.
    func openSpools() -> [JSON] {
        spools.flow = nil
        mcpStale = true
        return loadCatalog()
    }

    /// Asks for the catalog, and the registry's search again where there is one, as the
    /// person's servers from both change.
    func loadCatalog() -> [JSON] {
        var out: [JSON] = [["t": "cmd", "id": JSON(id("app.spools_list")), "name": "catalog.list", "args": [:]]]
        if !spools.search.query.isEmpty { out.append(askSearch(spools.search.query)) }
        return out
    }

    /// An entry by its id: the catalog's, or one the registry's search found.
    func entry(_ id: String) -> Entry? {
        spools.entries.first { $0.id == id } ?? spools.search.entries.first { $0.id == id }
    }

    /// What's typed in Find's search, once typing pauses: the MCP Registry is searched for
    /// it from `registryShortest` letters, and fewer forget the last search.
    func spoolSearch(_ query: String) -> JSON? {
        let q = query.trimmingCharacters(in: .whitespaces)
        guard q.count >= registryShortest else {
            spools.search = RegistrySearch()
            return nil
        }
        guard spools.search.query != q else { return nil }
        spools.search = RegistrySearch(query: q)
        return askSearch(q)
    }

    /// `catalog.search` for `query`, whose answer is the only one taken.
    private func askSearch(_ query: String) -> JSON {
        let id = id("app.spools_search")
        spools.search.asked = id
        return ["t": "cmd", "id": JSON(id), "name": "catalog.search", "args": ["query": JSON(query)]]
    }

    /// What Find shows for what's typed, `query`, with the pill picked: the catalog's that
    /// match, or that the registry found, then the registry's servers, which have no
    /// category, under All; and while the registry hasn't answered for it, or couldn't, a
    /// line that says so.
    func spoolsShown(_ query: String) -> SpoolsShown {
        let q = query.trimmingCharacters(in: .whitespaces)
        let search = q.count >= registryShortest && spools.search.query == q ? spools.search : nil
        let (cards, tiles) = found(spools.entries, pill: spools.pill, query: q, also: search?.curated ?? [])
        var shown = SpoolsShown(cards: cards, tiles: tiles)
        if let search {
            if spools.pill == nil { shown.tiles += search.entries }
            shown.line = search.answered ? search.error : "Searching…"
        }
        shown.nothing = shown.cards.isEmpty && shown.tiles.isEmpty && shown.line == nil
        return shown
    }

    /// The person's server `name`.
    func mcpServer(_ name: String) -> McpServer? {
        mcpServers.first { $0.name == name }
    }

    /// What Installed lists, in its order.
    var installedRows: [InstalledRow] {
        installed(mcpServers, spools.entries, spools.connects)
    }

    /// How an entry's card or tile stands.
    func entryStatus(_ e: Entry) -> (words: String, dot: StandingDot)? {
        cardStatus(e, mcpServers)
    }

    /// The answers to Spools' commands. Gives whether `f` was one.
    func spoolsResult(_ f: JSON) -> Bool {
        let id = f["id"].text
        let error = f["error"].isNull ? nil : (f["error"]["message"].string ?? f["error"]["code"].text)
        if id.hasPrefix("app.spools_list") {
            if let error { spools.error = error } else { catalogCame(f["value"]) }
            return true
        }
        if id.hasPrefix("app.spools_search") {
            // An answer to a search typed over since is left.
            guard spools.search.asked == id else { return true }
            let v = f["value"]
            spools.search.answered = true
            spools.search.entries = v["entries"].array.map(Entry.init)
            spools.search.curated = v["curated"].array.compactMap(\.string)
            spools.search.error = error ?? v["error"].string
            return true
        }
        if id.hasPrefix("app.spools_connect") {
            if let error {
                if var flow = spools.flow, flow.stage == .going(.adding) {
                    flow.stage = .going(.failed)
                    flow.said = error
                    spools.flow = flow
                } else {
                    notice = error
                }
            } else if let c = Connect(f["value"]["connect"]) {
                // The steps after it may have come first, as frames.
                let later = spools.connects[c.server].map { $0.step != .adding && $0.entry == c.entry } ?? false
                if !later { connectCame(c) }
            }
            mcpStale = true
            return true
        }
        if id.hasPrefix("app.spools_agents") {
            if let error {
                notice = error
            } else if var flow = spools.flow, flow.server == f["value"]["server"].text, flow.stage == .going(.agents) {
                flow.stage = .going(.done)
                spools.flow = flow
            }
            mcpStale = true
            return true
        }
        if id.hasPrefix("app.spools_try:") {
            spools.trying.remove(String(id.dropFirst("app.spools_try:".count)))
            if let error { notice = error }
            mcpStale = true
            return true
        }
        if id.hasPrefix("app.spools_server") {
            if let error { notice = error }
            mcpStale = true
            return true
        }
        return false
    }

    /// The catalog came: its entries, its categories, and the connects under way.
    private func catalogCame(_ v: JSON) {
        spools.entries = v["entries"].array.map(Entry.init)
        spools.categories = v["categories"].array.compactMap(\.string)
        spools.came = true
        spools.error = nil
        for e in v["entries"].array {
            if let c = Connect(e["connecting"]), spools.connects[c.server] == nil { spools.connects[c.server] = c }
        }
    }

    /// A `connect` frame: a server being connected moved to its next step, on this device
    /// or another of the person's.
    func connectFrame(_ f: JSON) {
        guard let c = Connect(f["connect"]) else { return }
        connectCame(c)
        // The servers and the catalog change with it.
        mcpStale = true
        if c.step == .done || c.step == .failed { pending += loadCatalog() }
    }

    private func connectCame(_ c: Connect) {
        // Reconnect waits on no one at its agents step: they're as they were.
        if c.step == .agents, spools.reconnecting.remove(c.server) != nil {
            let agents = mcpServer(c.server)?.agents ?? []
            pending.append(["t": "cmd", "id": JSON(id("app.spools_agents")), "name": "mcp.agents", "args": ["name": JSON(c.server), "agents": JSON(agents)]])
        }
        if c.step == .failed || c.step == .done { spools.reconnecting.remove(c.server) }
        if var flow = spools.flow, flow.entry == c.entry, flow.server == c.server || flow.server.isEmpty, case .going = flow.stage {
            flow.server = c.server
            flow.stage = .going(c.step)
            flow.said = c.said
            spools.flow = flow
        }
        if c.step == .done { spools.connects[c.server] = nil } else { spools.connects[c.server] = c }
    }

    /// Opens the detail of the entry `id`: where a connect of it is under way, at its step.
    func spoolOpen(_ id: String) {
        var flow = SpoolFlow(entry: id)
        if let c = spools.connects.values.first(where: { $0.entry == id && $0.step != .done }) {
            flow.server = c.server
            flow.stage = .going(c.step)
            flow.said = c.said
        }
        spools.flow = flow
    }

    func spoolClose() {
        spools.flow = nil
    }

    /// Connect, or Connect another, in the open detail: its form, where it asks for
    /// something first, or else connecting at once.
    func spoolConnect(another: Bool = false) -> JSON? {
        guard var flow = spools.flow, let entry = entry(flow.entry), entry.connectsHere else { return nil }
        flow.another = another
        flow.said = ""
        flow.agents = []
        if entry.asks(another: another) {
            flow.stage = .form
            spools.flow = flow
            return nil
        }
        flow.server = entry.server
        flow.stage = .going(.adding)
        spools.flow = flow
        return ["t": "cmd", "id": JSON(id("app.spools_connect")), "name": "catalog.connect", "args": .object(entry.connectArgs)]
    }

    /// The form's Connect, with what was typed in its fields and secrets: what's missing,
    /// in one line, or the connect.
    func spoolSubmit(fields: [String: String], secrets: [String: String]) -> JSON? {
        guard var flow = spools.flow, let entry = entry(flow.entry) else { return nil }
        if let said = missingInForm(entry, fields: fields, secrets: secrets) {
            flow.said = said
            spools.flow = flow
            return nil
        }
        func picked(_ list: [String: String]) -> JSON {
            .object(list.filter { !$0.value.trimmingCharacters(in: .whitespaces).isEmpty }.mapValues { JSON($0.trimmingCharacters(in: .whitespaces)) })
        }
        var args = entry.connectArgs
        args["fields"] = picked(fields)
        args["secrets"] = picked(secrets)
        var name = entry.server
        if flow.another {
            let address = entry.fields.first.flatMap { fields[$0.name] } ?? ""
            name = anotherName(entry, address: address, taken: mcpServers.map(\.name))
            args["name"] = JSON(name)
        }
        flow.server = name
        flow.said = ""
        flow.stage = .going(.adding)
        spools.flow = flow
        return ["t": "cmd", "id": JSON(id("app.spools_connect")), "name": "catalog.connect", "args": .object(args)]
    }

    /// Back from a connect that failed, or from the form: to the form again, for one that
    /// asks, or to the entry.
    func spoolBack() {
        guard var flow = spools.flow else { return }
        let asks = entry(flow.entry)?.asks(another: flow.another) ?? false
        flow.stage = flow.stage == .going(.failed) && asks ? .form : .about
        flow.said = ""
        spools.flow = flow
    }

    /// An agent's chip at the agents step, turned on or off.
    func spoolPickAgent(_ agent: String) {
        guard var flow = spools.flow else { return }
        flow.agents = toggled(flow.agents, agent)
        spools.flow = flow
    }

    /// Done at the agents step: the server goes to the agents picked.
    func spoolGive() -> JSON? {
        guard let flow = spools.flow, flow.stage == .going(.agents) else { return nil }
        return ["t": "cmd", "id": JSON(id("app.spools_agents")), "name": "mcp.agents", "args": ["name": JSON(flow.server), "agents": JSON(flow.agents)]]
    }

    /// An agent's chip on one of the person's servers, turned on or off, as it shows at
    /// once.
    func spoolToggleAgent(server: String, agent: String) -> JSON? {
        guard let i = mcpServers.firstIndex(where: { $0.name == server }) else { return nil }
        mcpServers[i].agents = toggled(mcpServers[i].agents, agent)
        return ["t": "cmd", "id": JSON(id("app.spools_agents")), "name": "mcp.agents", "args": ["name": JSON(server), "agents": JSON(mcpServers[i].agents)]]
    }

    /// How one of the person's servers stands in its detail, in words and a dot: being
    /// connected or tried just now says so, or else how its last test found it.
    func serverStanding(_ m: McpServer) -> (words: String, dot: StandingDot) {
        if spools.connects[m.name]?.step == .adding { return ("Connecting…", .quiet) }
        if spools.trying.contains(m.name) { return ("Trying…", .quiet) }
        return (statusLine(m), statusDot(m))
    }

    /// Save on Installed's form, for a server added by hand (`was` none) or changed: the
    /// `mcp.put` that keeps it, whose answer reads the servers again; none while it has no
    /// name, or no command or address.
    func putServer(name: String, line: String, settings: String, clientId: String, clientSecret: String, was: String?) -> JSON? {
        if name.trimmingCharacters(in: .whitespaces).isEmpty || line.trimmingCharacters(in: .whitespaces).isEmpty { return nil }
        let args = serverArgs(name: name, line: line, settings: settings, clientId: clientId, clientSecret: clientSecret, was: was)
        return ["t": "cmd", "id": JSON(id("app.spools_server")), "name": "mcp.put", "args": .object(args)]
    }

    /// Try it: one not tried yet is tested.
    func spoolTry(_ server: String) -> JSON {
        spools.trying.insert(server)
        return ["t": "cmd", "id": JSON("app.spools_try:\(server)"), "name": "mcp.test", "args": ["name": JSON(server)]]
    }

    /// Reconnect: one from the catalog is connected again from its entry, as it is,
    /// keeping its agents; one added by hand is tried again.
    func spoolReconnect(_ server: String) -> JSON? {
        guard let m = mcpServer(server) else { return nil }
        guard !m.catalog.isEmpty, spools.entries.contains(where: { $0.id == m.catalog }) else { return spoolTry(server) }
        spools.reconnecting.insert(server)
        spools.connects[server] = Connect(entry: m.catalog, server: server, step: .adding)
        return ["t": "cmd", "id": JSON(id("app.spools_connect")), "name": "catalog.connect", "args": ["id": JSON(m.catalog), "name": JSON(server)]]
    }

    /// Disconnect: its sign-in is dropped.
    func spoolDisconnect(_ server: String) -> JSON {
        ["t": "cmd", "id": JSON(id("app.spools_server")), "name": "mcp.sign_out", "args": ["name": JSON(server)]]
    }

    /// Remove: the server goes.
    func spoolRemove(_ server: String) -> JSON {
        spools.connects[server] = nil
        mcpServers.removeAll { $0.name == server }
        return ["t": "cmd", "id": JSON(id("app.spools_server")), "name": "mcp.delete", "args": ["name": JSON(server)]]
    }
}
