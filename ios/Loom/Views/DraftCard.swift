import SwiftUI
import UIKit

/// A draft card in an agent's reply (PLAN.md, "Drafts"), as the desktop's draft_view.rs
/// draws it. Open, the whole of what goes out: a head with the service's mark, "Draft
/// email" and the thread a reply goes in; From, To, Cc and Subject as rows; the body; then
/// "The message it answers", folded, and Reject, Edit and Send, with Send later in the
/// arrow's menu. Edit makes the rows and the body editable in place, with the keyboard.
/// Once sent, one line: the mark, the subject and where it went, "Sent 14:02 ✓", and
/// Undo with its seconds left while it can be taken back.
struct DraftCard: View {
    @Environment(Session.self) private var session
    var card: Card

    @State private var editing = false
    @State private var rejecting = false
    @State private var answersOpen = false
    @State private var typed: [String: String] = [:]
    @State private var bodyText = ""
    @State private var note = ""

    var body: some View {
        if let d = card.draft {
            TimelineView(.periodic(from: .now, by: 1)) { t in
                switch d.shown(now: Int(t.date.timeIntervalSince1970 * 1000)) {
                case .open: open(d)
                case let .going(what, to, at, left):
                    line(d, what: what, to: to) {
                        sent(at)
                        Button("Undo · \(left)") { answer("undo") }.buttonStyle(Ghost())
                    }
                case let .sending(what, to):
                    line(d, what: what, to: to) { Text("Sending…").font(Fonts.caption).foregroundStyle(P.ink3) }
                case let .sent(what, to, at):
                    line(d, what: what, to: to) { sent(at) }
                case let .later(what, to, at):
                    line(d, what: what, to: to) {
                        Text("Sends \(laterWords(at))").font(Fonts.caption).foregroundStyle(P.ink2)
                        Button("Undo") { answer("undo") }.buttonStyle(Ghost())
                    }
                case let .failed(what, to, why):
                    line(d, what: what, to: to) {
                        Text("Not sent: \(why)").font(Fonts.caption).foregroundStyle(P.bad).lineLimit(1)
                        Button("Try again") { answer("retry") }.buttonStyle(CardButtonStyle())
                    }
                case let .rejected(what):
                    CardDone(badge: card.badge, name: d.service, icon: "xmark", how: "Rejected", what: what)
                }
            }
        }
    }

    // MARK: Open

    private func open(_ d: Draft) -> some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous)
        return VStack(alignment: .leading, spacing: 0) {
            // The head: the mark, what it is, and the thread a reply goes in.
            HStack(spacing: Tokens.Space.s) {
                mark(d)
                Text(card.title).font(Fonts.ui(Tokens.PhoneSize.small, .medium)).foregroundStyle(P.ink)
                if !d.thread.isEmpty {
                    Text("· replying in “\(d.thread)”").font(Fonts.caption).foregroundStyle(P.ink3).lineLimit(1)
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            Divider().overlay(P.line)
            // The rows, a label beside each value, or a box while it's edited.
            VStack(alignment: .leading, spacing: 0) {
                ForEach(d.rowsShown(editing: editing), id: \.key) { r in
                    HStack(spacing: 0) {
                        Text(r.label).foregroundStyle(P.ink3).frame(width: 64, alignment: .leading)
                        if editing && r.edit {
                            TextField(r.label, text: Binding(get: { typed[r.key] ?? r.value }, set: { typed[r.key] = $0 }))
                                .keyboardType(r.key == "subject" ? .default : .emailAddress)
                                .textInputAutocapitalization(r.key == "subject" ? .sentences : .never)
                                .autocorrectionDisabled(r.key != "subject")
                                .draftBox()
                        } else {
                            (Text(r.value).foregroundStyle(P.ink) + Text(r.key == "from" && !d.serviceName.isEmpty && r.value.lowercased() != d.serviceName.lowercased() ? " (\(d.serviceName))" : "").foregroundStyle(P.ink3))
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }
                        Spacer(minLength: 0)
                    }
                    .font(Fonts.small)
                    .frame(minHeight: 30)
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 6)
            Divider().overlay(P.line)
            // The body as it goes out, or its box.
            Group {
                if editing {
                    TextField("", text: $bodyText, axis: .vertical)
                        .lineLimit(3...16)
                        .padding(Tokens.Space.s)
                        .background(P.bg, in: RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous))
                        .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous).strokeBorder(P.lineStrong, lineWidth: 1))
                } else {
                    Text(d.body).fixedSize(horizontal: false, vertical: true)
                }
            }
            .font(Fonts.small)
            .lineSpacing(4)
            .foregroundStyle(P.ink)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            // The message it answers, once it's unfolded.
            if answersOpen && !d.answers.isEmpty {
                Text(d.answers)
                    .font(Fonts.caption)
                    .foregroundStyle(P.ink2)
                    .padding(.leading, Tokens.Space.m)
                    .overlay(alignment: .leading) { Rectangle().fill(P.lineStrong).frame(width: 2) }
                    .padding(.horizontal, 14)
                    .padding(.bottom, 10)
            }
            Divider().overlay(P.line)
            foot(d)
            if !card.error.isEmpty {
                Text(card.error).font(Fonts.caption).foregroundStyle(P.bad).padding(.horizontal, 14).padding(.bottom, 10)
            }
        }
        .background(shape.fill(P.bg))
        .overlay(shape.strokeBorder(P.line, lineWidth: 1))
        .clipShape(shape)
    }

    /// The foot: the message it answers, folded, and Reject, Edit, Send and Send later's
    /// menu; while it's edited, Cancel and Send; while it's rejected, its note, Cancel
    /// and Reject.
    @ViewBuilder private func foot(_ d: Draft) -> some View {
        if rejecting {
            HStack(spacing: Tokens.Space.s) {
                TextField("A note for the agent", text: $note).draftBox()
                Button("Cancel") { rejecting = false }.buttonStyle(Ghost())
                Button("Reject") { answer("reject") }.buttonStyle(CardButtonStyle(primary: true))
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
        } else {
            VStack(alignment: .leading, spacing: Tokens.Space.s) {
                if !d.answers.isEmpty {
                    Button {
                        answersOpen.toggle()
                    } label: {
                        Text("\(answersOpen ? "⌄" : "›") The message it answers").font(Fonts.caption).foregroundStyle(P.ink3)
                    }
                    .buttonStyle(.plain)
                }
                HStack(spacing: Tokens.Space.s) {
                    Spacer(minLength: 0)
                    if editing {
                        Button("Cancel") { editing = false }.buttonStyle(Ghost())
                    } else {
                        Button("Reject") { rejecting = true }.buttonStyle(Ghost())
                        Button("Edit") {
                            typed = [:]
                            bodyText = d.body
                            editing = true
                        }
                        .buttonStyle(CardButtonStyle())
                    }
                    HStack(spacing: 0) {
                        Button(card.sending ? "Sending…" : "Send") { answer("send") }
                            .buttonStyle(CardButtonStyle(primary: true, half: .leading))
                        Menu {
                            Section("Send later") {
                                ForEach(laterTimes(from: .now), id: \.at) { t in
                                    Button(t.words) { answer("later", at: t.at) }
                                }
                            }
                        } label: {
                            Image(systemName: "chevron.down").font(.system(size: 10, weight: .semibold))
                        }
                        .buttonStyle(CardButtonStyle(primary: true, half: .trailing))
                        .accessibilityLabel("Send later")
                    }
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
        }
    }

    // MARK: Parts

    /// The service's mark, in the quiet ink, or its badge where the app has none.
    @ViewBuilder private func mark(_ d: Draft) -> some View {
        if let glyph = d.mark.flatMap({ UIImage(named: "mark-\($0)") }) {
            Image(uiImage: glyph).renderingMode(.template).resizable().scaledToFit().frame(width: 14, height: 14).foregroundStyle(P.ink2)
        } else {
            BadgeView(badge: card.badge, name: d.service, size: 18)
        }
    }

    /// The one line a draft is once it's gone, going or couldn't go.
    private func line<Trailing: View>(_ d: Draft, what: String, to: String, @ViewBuilder trailing: () -> Trailing) -> some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous)
        return HStack(spacing: Tokens.Space.s + 2) {
            mark(d)
            (Text(what).foregroundStyle(P.ink) + Text(to.isEmpty ? "" : "  → \(to)").foregroundStyle(P.ink3))
                .font(Fonts.small)
                .lineLimit(1)
                .truncationMode(.tail)
            Spacer(minLength: 0)
            trailing()
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .background(shape.fill(P.bg))
        .overlay(shape.strokeBorder(P.line, lineWidth: 1))
    }

    private func sent(_ at: Int) -> some View {
        Text("Sent \(clock(at)) ✓").font(Fonts.caption).foregroundStyle(P.ok)
    }

    private func clock(_ at: Int) -> String {
        Date(timeIntervalSince1970: Double(at) / 1000).formatted(.dateTime.hour(.twoDigits(amPM: .omitted)).minute(.twoDigits))
    }

    /// When a draft set to go later goes: its time today, or tomorrow, or its date.
    private func laterWords(_ at: Int) -> String {
        let date = Date(timeIntervalSince1970: Double(at) / 1000)
        let cal = Calendar.current
        if cal.isDateInToday(date) { return clock(at) }
        if cal.isDateInTomorrow(date) { return "tomorrow \(clock(at))" }
        return date.formatted(.dateTime.day().month(.abbreviated).hour().minute())
    }

    /// Sends the person's answer: Send and Send later with what Edit changed, Reject with
    /// its note.
    private func answer(_ what: String, at: Int = 0) {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        var edits: [String: String]?
        if editing && (what == "send" || what == "later") {
            var e = typed
            e["body"] = bodyText
            edits = e
        }
        session.send(session.model.answerDraft(message: card.message, card: card.id, answer: what, edits: edits, note: what == "reject" ? note : "", at: at))
        if what == "send" || what == "later" || what == "reject" {
            editing = false
            rejecting = false
        }
    }
}

/// A button with no outline, quiet: Reject, Cancel, Undo.
private struct Ghost: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
            .lineLimit(1)
            .padding(.horizontal, Tokens.Space.m)
            .frame(minHeight: Tokens.Control.button)
            .foregroundStyle(P.ink2)
            .opacity(configuration.isPressed ? 0.6 : 1)
            .contentShape(Rectangle())
    }
}

private extension View {
    /// A field on a draft, in its box.
    func draftBox() -> some View {
        self
            .padding(.horizontal, Tokens.Space.s)
            .frame(minHeight: Tokens.Control.buttonSmall)
            .background(P.bg, in: RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous).strokeBorder(P.lineStrong, lineWidth: 1))
    }
}
