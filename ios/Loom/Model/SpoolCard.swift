import Foundation

// The card an agent's spool_connect puts in its reply (PLAN.md, "An agent connects a
// spool"), as desktop/src/model/spool_card.rs works it out: Spools' card for the entry,
// loom's knot tied to its logo, its name, address and the agents it's for, with Sign in.
// Sign in is `spool.connect`; the instance moves the card along in place, and once it
// waits for the sign-in, the phone that sent Sign in opens the sign-in page. T3 refuses
// the phone's `loom:` return address, so its card says to finish on a computer.

/// The id a Sign in's command goes as, before the card's id.
let spoolAnswer = "app.spool_connect:"

/// A key the entry takes, which the person types on the card.
struct SpoolKey: Equatable, Sendable {
    var name: String
    var label: String
    var placeholder: String
}

/// What the foot of a spool card says, beside its button.
enum SpoolFoot: Equatable {
    case forAgents(String)
    case connecting
    case signingIn
    case connected(Int)
    case failed(String)
    /// T3 on the phone: finished from loom on a computer.
    case computer
}

/// The foot of spool card `c`, as the desktop works it out.
func spoolFoot(_ c: Card) -> SpoolFoot {
    switch c.state {
    case "connecting": return .connecting
    case "sign_in" where c.said.isEmpty: return .signingIn
    case "sign_in", "failed": return .failed(c.said)
    case "connected": return .connected(c.tools.count)
    default:
        if connectedFromAComputer.contains(c.entry) { return .computer }
        if !c.error.isEmpty { return .failed(c.error) }
        return .forAgents("For \(c.agentNames.joined(separator: ", "))")
    }
}

/// "Connected · 12 tools", as the card says it.
func spoolConnectedLine(_ tools: Int) -> String {
    switch tools {
    case 0: return "Connected"
    case 1: return "Connected · 1 tool"
    default: return "Connected · \(tools) tools"
    }
}

/// The address as the card shows it: without `https://` and the path loom adds.
func spoolShortAddress(_ address: String) -> String {
    var bare = address
    for scheme in ["https://", "http://"] where bare.hasPrefix(scheme) { bare = String(bare.dropFirst(scheme.count)) }
    if bare.hasSuffix("/mcp") { bare = String(bare.dropLast(4)) }
    return bare
}

extension Model {
    /// Sign in, or Try again, on spool card `card` in reply `message`, for its server
    /// `server`, with the keys typed on it.
    func spoolSignIn(message: String, card: String, server: String, secrets: [String: String]) -> JSON {
        spoolErrors[card] = nil
        spoolSending.insert(card)
        spoolWaiting[card] = server
        var keys: [String: JSON] = [:]
        for (name, value) in secrets { keys[name] = JSON(value) }
        return ["t": "cmd", "id": JSON("\(spoolAnswer)\(card)"), "name": "spool.connect",
                "args": ["message": JSON(message), "card": JSON(card), "secrets": JSON.object(keys)]]
    }

    /// A spool card as the instance sent it: once one this phone signed in from waits for
    /// the sign-in, its sign-in page opens, once.
    func spoolSeen(_ c: JSON) {
        guard c["type"].text == "spool_connect" else { return }
        let id = c["id"].text
        switch c["state"].text {
        case "sign_in":
            if let server = spoolWaiting.removeValue(forKey: id), let f = signIn(server, redirect: SignInPage.redirect) {
                pending.append(f)
            }
        case "connected", "failed":
            spoolWaiting[id] = nil
            mcpStale = true
        default:
            break
        }
    }

    /// Sign in's answer: the card's own changes say how it goes, and a refusal shows on
    /// the card.
    func spoolAnswered(_ id: String, error: String?) {
        let card = String(id.dropFirst(spoolAnswer.count))
        spoolSending.remove(card)
        if let error {
            spoolWaiting[card] = nil
            spoolErrors[card] = error
        }
    }
}
