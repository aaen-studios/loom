import SwiftUI
import UIKit

// loom's look on the phone, from the design tokens (design/tokens.toml, generated into
// Tokens.swift). Each colour follows the system's light or dark appearance by itself.

extension UIColor {
    convenience init(loom hex: UInt32) {
        self.init(red: CGFloat((hex >> 16) & 0xff) / 255, green: CGFloat((hex >> 8) & 0xff) / 255, blue: CGFloat(hex & 0xff) / 255, alpha: 1)
    }
}

extension Color {
    /// A colour with a value for light and one for dark.
    static func loom(_ light: UInt32, _ dark: UInt32) -> Color {
        Color(UIColor { $0.userInterfaceStyle == .dark ? UIColor(loom: dark) : UIColor(loom: light) })
    }
}

enum P {
    typealias L = Tokens.ColorLight
    typealias D = Tokens.ColorDark
    static let bg = Color.loom(L.bg, D.bg)
    static let side = Color.loom(L.side, D.side)
    static let raised = Color.loom(L.raised, D.raised)
    static let raised2 = Color.loom(L.raised2, D.raised2)
    static let hover = Color.loom(L.hover, D.hover)
    static let active = Color.loom(L.active, D.active)
    static let line = Color.loom(L.line, D.line)
    static let lineStrong = Color.loom(L.lineStrong, D.lineStrong)
    static let ink = Color.loom(L.ink, D.ink)
    static let ink2 = Color.loom(L.ink2, D.ink2)
    static let ink3 = Color.loom(L.ink3, D.ink3)
    static let button = Color.loom(L.button, D.button)
    static let buttonInk = Color.loom(L.buttonInk, D.buttonInk)
    static let bubble = Color.loom(L.bubble, D.bubble)
    static let ok = Color.loom(L.ok, D.ok)
    static let bad = Color.loom(L.bad, D.bad)
    /// A call that can change or run something, in its pill on a card.
    static let warn = Color.loom(L.warn, D.warn)
    /// A card in the chat, as the desktop's `card_fill`: the page's colour in light,
    /// raised in dark; what's set into it the other way round; and its edge.
    static let cardFill = Color.loom(L.bg, D.raised)
    static let cardInset = Color.loom(L.raised, D.bg)
    static let cardEdge = Color.loom(L.lineStrong, D.line)

    /// An agent's thread colour, by the name the instance gives it, or its own as
    /// #rrggbb, kept to its hue and made light or dark enough for the appearance.
    /// Kept per name: a new dynamic colour never equals the last, so a thread given one
    /// would be drawn again whenever the view around it is.
    static func thread(_ name: String) -> Color {
        if let kept = threads[name] { return kept }
        let c = makeThread(name)
        threads[name] = c
        return c
    }

    nonisolated(unsafe) private static var threads: [String: Color] = [:]    // drawn on the main thread only

    /// The person's bubble in an agent's chat: the bubble grey with a breath of the
    /// agent's thread colour, 7% in light and 12% in dark. With Vibrant bubbles it's the
    /// thread colour itself, as saturated as it is, at a lightness of 80% in light and 28%
    /// in dark, where the words read on every colour, as the desktop's `person_bubble`.
    /// Kept per colour, as `thread` is.
    static func personBubble(_ name: String, vibrant: Bool = false) -> Color {
        let key = vibrant ? "vibrant:\(name)" : name
        if let kept = personBubbles[key] { return kept }
        let tint = UIColor(thread(name))
        let light = UIColor(loom: L.bubble), dark = UIColor(loom: D.bubble)
        let c = Color(UIColor { traits in
            let isDark = traits.userInterfaceStyle == .dark
            let t = tint.resolvedColor(with: traits)
            if vibrant { return lightness(t, isDark ? 0.28 : 0.8) }
            return mix(isDark ? dark : light, t, isDark ? 0.12 : 0.07)
        })
        personBubbles[key] = c
        return c
    }

    nonisolated(unsafe) private static var personBubbles: [String: Color] = [:]    // drawn on the main thread only

    /// `c` with its HSL lightness set to `l`, keeping its hue and saturation.
    private static func lightness(_ c: UIColor, _ l: CGFloat) -> UIColor {
        var (h, sv, v, a): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
        c.getHue(&h, saturation: &sv, brightness: &v, alpha: &a)
        // HSB to HSL's saturation, then back to HSB at the new lightness.
        let l0 = v * (1 - sv / 2)
        let room0 = min(l0, 1 - l0)
        let s = room0 > 0 ? (v - l0) / room0 : 0
        let v1 = l + s * min(l, 1 - l)
        let sv1 = v1 > 0 ? 2 * (1 - l / v1) : 0
        return UIColor(hue: h, saturation: sv1, brightness: v1, alpha: a)
    }

    /// `a` moved `k` of the way to `b`, channel by channel in sRGB.
    private static func mix(_ a: UIColor, _ b: UIColor, _ k: CGFloat) -> UIColor {
        var (ar, ag, ab, aa): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
        var (br, bg, bb, ba): (CGFloat, CGFloat, CGFloat, CGFloat) = (0, 0, 0, 0)
        a.getRed(&ar, green: &ag, blue: &ab, alpha: &aa)
        b.getRed(&br, green: &bg, blue: &bb, alpha: &ba)
        let l = { (x: CGFloat, y: CGFloat) in x + (y - x) * k }
        return UIColor(red: l(ar, br), green: l(ag, bg), blue: l(ab, bb), alpha: l(aa, ba))
    }

    private static func makeThread(_ name: String) -> Color {
        typealias TL = Tokens.ThreadColorLight
        typealias TD = Tokens.ThreadColorDark
        if name.count == 7, name.hasPrefix("#"), let rgb = UInt32(name.dropFirst(), radix: 16) {
            let base = UIColor(loom: rgb)
            return Color(UIColor { traits in
                var h: CGFloat = 0, s: CGFloat = 0, l: CGFloat = 0, a: CGFloat = 0
                base.getHue(&h, saturation: &s, brightness: &l, alpha: &a)
                let dark = traits.userInterfaceStyle == .dark
                return UIColor(hue: h, saturation: dark ? min(s, 0.55) : max(s, 0.5), brightness: dark ? max(l, 0.85) : min(max(l, 0.45), 0.75), alpha: 1)
            })
        }
        switch name {
        case "coral": return .loom(TL.coral, TD.coral)
        case "moss": return .loom(TL.moss, TD.moss)
        case "teal": return .loom(TL.teal, TD.teal)
        case "plum": return .loom(TL.plum, TD.plum)
        case "mint": return .loom(TL.mint, TD.mint)
        case "apricot": return .loom(TL.apricot, TD.apricot)
        case "pink": return .loom(TL.pink, TD.pink)
        case "sky": return .loom(TL.sky, TD.sky)
        case "gold": return .loom(TL.gold, TD.gold)
        case "lavender": return .loom(TL.lavender, TD.lavender)
        case "grey": return .loom(TL.grey, TD.grey)
        default: return .loom(TL.periwinkle, TD.periwinkle)
        }
    }
}

/// The type: Geist and Geist Mono at the phone's sizes, scaled with Dynamic Type.
enum Fonts {
    typealias S = Tokens.PhoneSize

    static func ui(_ size: CGFloat, _ weight: Font.Weight = .regular, relativeTo style: Font.TextStyle = .body) -> Font {
        .custom(Tokens.Font.ui, size: size, relativeTo: style).weight(weight)
    }

    static func mono(_ size: CGFloat, relativeTo style: Font.TextStyle = .body) -> Font {
        .custom(Tokens.Font.mono, size: size, relativeTo: style)
    }

    static let body = ui(S.body)
    static let reply = ui(S.reply)
    static let small = ui(S.small, relativeTo: .subheadline)
    static let caption = ui(S.caption, relativeTo: .caption)
    static let heading = ui(S.heading, .semibold, relativeTo: .title)
    static let hero = ui(S.hero, .semibold, relativeTo: .largeTitle)
    static let code = mono(S.small - 0.5, relativeTo: .subheadline)
}

/// A plain button in loom's style: filled for the main action, outlined otherwise.
struct LoomButtonStyle: ButtonStyle {
    var primary = true
    var wide = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
            .padding(.horizontal, Tokens.Space.l)
            .frame(maxWidth: wide ? .infinity : nil, minHeight: wide ? Tokens.Control.buttonWide : 44)
            .foregroundStyle(primary ? P.buttonInk : P.ink)
            .background {
                RoundedRectangle(cornerRadius: Tokens.Radius.button, style: .continuous)
                    .fill(primary ? P.button : Color.clear)
                    .strokeBorder(primary ? Color.clear : P.lineStrong, lineWidth: 1)
            }
            .opacity(configuration.isPressed ? 0.7 : 1)
            .contentShape(Rectangle())
    }
}

extension ButtonStyle where Self == LoomButtonStyle {
    static var loom: LoomButtonStyle { LoomButtonStyle() }
    static var loomWide: LoomButtonStyle { LoomButtonStyle(wide: true) }
    static var loomQuiet: LoomButtonStyle { LoomButtonStyle(primary: false) }
}

/// Times as the chat says them.
enum When {
    static func time(_ ms: Int) -> String {
        guard ms > 0, ms != .max else { return "" }
        let date = Date(timeIntervalSince1970: Double(ms) / 1000)
        if Calendar.current.isDateInToday(date) {
            return date.formatted(date: .omitted, time: .shortened)
        }
        if Calendar.current.isDateInYesterday(date) {
            return "Yesterday " + date.formatted(date: .omitted, time: .shortened)
        }
        return date.formatted(.dateTime.day().month(.abbreviated).hour().minute())
    }

    static func size(_ bytes: Int) -> String {
        switch bytes {
        case ..<1000: return "\(bytes) B"
        case ..<1_000_000: return "\(bytes / 1000) KB"
        default: return String(format: "%.1f MB", Double(bytes) / 1_000_000)
        }
    }
}
