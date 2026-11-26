import Foundation

// The card a routine leaves in the chat where it posts when something other than its
// person turns it off: five failures in a row, or its agent's pause_routine (PLAN.md,
// "Routines"), as desktop/src/model/routine_off.rs works it out. While it's off, its
// name, why on one line, and Turn on, which is `routine.put` with `on`. Once it's on
// again, from the card or anywhere else, or deleted, the instance says so on the card
// (`message.updated`), and it's one quiet line with no button.

/// The id a Turn on's command goes as, before the routine's id.
let routineOnAnswer = "app.routine_on:"

/// What a routine's card says now.
enum RoutineOffShown: Equatable {
    /// Off: why, whether Turn on is on its way, and what the instance said to one it
    /// refused.
    case off(why: String, sending: Bool, error: String)
    case on
    case deleted
}

/// Whether a stored message is a routine's card: a `loom` message with a `routine_off`
/// card.
func isRoutineOffLine(_ m: JSON) -> Bool {
    m["role"].text == "loom" && m["blocks"].array.contains { $0["kind"].text == "card" && $0["card"]["type"].text == "routine_off" }
}

extension Message {
    /// The routine_off card this message is, when it's one.
    var routineOff: Card? {
        guard role == "loom" else { return nil }
        return blocks.lazy.compactMap(\.card).first { $0.type == "routine_off" }
    }
}

extension Model {
    /// What routine card `c` says now, with the name of the agent that paused it.
    func routineOffShown(_ c: Card) -> RoutineOffShown {
        switch c.state {
        case "on": return .on
        case "deleted": return .deleted
        default:
            let who = c.agent.isEmpty ? "Failed \(c.failures) times in a row" : "\(agent(c.agent)?.name ?? "Its agent") paused it"
            let why = c.why.trimmingCharacters(in: .whitespacesAndNewlines)
            return .off(why: why.isEmpty ? who : "\(who): \(why)", sending: c.sending, error: c.error)
        }
    }

    /// Turn on, on routine `routine`'s card: it runs again with its schedule as it was.
    func turnRoutineOn(_ routine: String) -> JSON {
        routineOnErrors[routine] = nil
        routineOnSending.insert(routine)
        return ["t": "cmd", "id": JSON("\(routineOnAnswer)\(routine)"), "name": "routine.put",
                "args": ["id": JSON(routine), "on": true]]
    }
}
