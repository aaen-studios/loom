import Foundation

/// How this phone shows loom, as the person set it in Settings, kept on the phone as the
/// desktop keeps its `prefs.json`: the keys `@AppStorage` keeps them under.
enum Prefs {
    /// How an agent's answer shows: `bubbles` (the default) or `plain`.
    static let replies = "loom.replies"
    static let bubbles = "bubbles"
    static let plain = "plain"
    /// Whether the person's bubbles are the agent's colour itself or a breath of it (the
    /// default). `@AppStorage` keeps it only once the person sets it, so a phone that never
    /// did takes the default, and one whose person picked Vibrant keeps it.
    static let vibrant = "loom.vibrant_bubbles"
}
