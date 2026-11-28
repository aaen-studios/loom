import Foundation

// Loom's card under a reply whose chat its agent program moved to another model after a
// model's safeguards flagged a request (PLAN.md, "A model fallback"), as
// desktop/src/model/model_fallback.rs works it out. While the chat is there, its title,
// Switch back (`conversation.model_back`) and New chat (`conversation.create` with
// `from`). Once either is used, or the person picks a model, the instance says what
// happened on the card (`message.updated`), and it's one quiet line with no buttons. The
// reply's own line says so too, as a `notice` block, which the chat draws already.

/// The ids Switch back's and New chat's commands go as, before the chat's id.
let modelBackAnswer = "app.model_back:"
let fallbackChatAnswer = "app.fallback_chat:"

/// Which of the card's buttons is on its way.
enum FallbackGoing: Equatable {
    case back
    case newChat
}

/// What a fallback card says now.
enum FallbackShown: Equatable {
    /// On the fallback: its title, the model it was on, which button is on its way, and
    /// what the instance said to one it refused.
    case on(title: String, from: String, going: FallbackGoing?, error: String)
    /// Done with: what happened, in one line.
    case done(String)
}

/// Whether a stored message is a fallback's card: a `loom` message with a
/// `model_fallback` card.
func isFallbackLine(_ m: JSON) -> Bool {
    m["role"].text == "loom" && m["blocks"].array.contains { $0["kind"].text == "card" && $0["card"]["type"].text == "model_fallback" }
}

extension Message {
    /// The model_fallback card this message is, when it's one.
    var modelFallback: Card? {
        guard role == "loom" else { return nil }
        return blocks.lazy.compactMap(\.card).first { $0.type == "model_fallback" }
    }
}

extension Model {
    /// What fallback card `c` says now.
    func fallbackShown(_ c: Card) -> FallbackShown {
        guard c.state == "fallback" else { return .done(c.said) }
        let key = conversation ?? ""
        let going: FallbackGoing? = modelBackSending.contains(key) ? .back : fallbackChatSending.contains(key) ? .newChat : nil
        return .on(title: c.title, from: c.fromName, going: going, error: fallbackErrors[key] ?? "")
    }

    /// Switch back, on chat `conversation`'s card: its session goes back to the model it
    /// was on.
    func switchModelBack(_ conversation: String) -> JSON {
        fallbackErrors[conversation] = nil
        modelBackSending.insert(conversation)
        return ["t": "cmd", "id": JSON("\(modelBackAnswer)\(conversation)"), "name": "conversation.model_back",
                "args": ["conversation": JSON(conversation)]]
    }

    /// New chat, on chat `conversation`'s card: a new chat with its agent, on the agent's
    /// own pick, which the card says took over.
    func newChatFrom(_ conversation: String, agent: String) -> JSON {
        fallbackErrors[conversation] = nil
        fallbackChatSending.insert(conversation)
        return ["t": "cmd", "id": JSON("\(fallbackChatAnswer)\(conversation)"), "name": "conversation.create",
                "args": ["agent": JSON(agent), "from": JSON(conversation)]]
    }

    /// Either button's answer: the card's own message.updated says what happened, and a
    /// refusal shows on the card, which keeps its buttons. Whether `id` was one.
    func takeFallbackAnswer(_ id: String, error: String?) -> Bool {
        let conversation: String
        if id.hasPrefix(modelBackAnswer) {
            conversation = String(id.dropFirst(modelBackAnswer.count))
        } else if id.hasPrefix(fallbackChatAnswer) {
            conversation = String(id.dropFirst(fallbackChatAnswer.count))
        } else {
            return false
        }
        modelBackSending.remove(conversation)
        fallbackChatSending.remove(conversation)
        if let error { fallbackErrors[conversation] = error }
        return true
    }
}
