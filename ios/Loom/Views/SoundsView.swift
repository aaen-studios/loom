import SwiftUI

/// Settings → Sounds, the same rows as the desktop's App & notifications (PLAN.md, "Sounds"):
/// loom, System or Off, the volume, each cue with its play button and switch, silent while
/// the chat is in front, and quiet hours. Each change goes to the core (`app.sounds`).
struct SoundsView: View {
    @Environment(Session.self) private var session
    @State private var prefs = SoundPrefs.load()

    var body: some View {
        List {
            Section {
                Picker(selection: $prefs.mode) {
                    Text("loom").tag("loom")
                    Text("System").tag("system")
                    Text("Off").tag("off")
                } label: {
                    Text("Sounds").font(Fonts.body).foregroundStyle(P.ink)
                }
                .tint(P.ink3)
                HStack(spacing: Tokens.Space.m) {
                    Text("Volume").font(Fonts.body).foregroundStyle(P.ink)
                    Slider(value: $prefs.volume, in: 0...1, step: 0.05)
                        .tint(P.ink)
                }
            }
            Section {
                ForEach(SoundPrefs.cues, id: \.0) { cue, label in
                    HStack(spacing: Tokens.Space.m) {
                        Text(label).font(Fonts.body).foregroundStyle(P.ink)
                        Spacer()
                        Button {
                            session.send(["t": "sound.try", "cue": JSON(cue)])
                        } label: {
                            Image(systemName: "play")
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(P.ink2)
                                .frame(width: 30, height: 30)
                                .overlay(RoundedRectangle(cornerRadius: 7, style: .continuous).strokeBorder(P.line))
                        }
                        .buttonStyle(.borderless)
                        .accessibilityLabel("Play \(label)")
                        Toggle(label, isOn: Binding(get: { prefs.on[cue] ?? false }, set: { prefs.on[cue] = $0 }))
                            .labelsHidden()
                    }
                }
            }
            Section {
                Toggle(isOn: $prefs.inFront) {
                    Text("Silent while the chat is in front").font(Fonts.body).foregroundStyle(P.ink)
                }
                Toggle(isOn: $prefs.quiet) {
                    Text("Quiet hours").font(Fonts.body).foregroundStyle(P.ink)
                }
                DatePicker(selection: time(\.from), displayedComponents: .hourAndMinute) {
                    Text("From").font(Fonts.body).foregroundStyle(P.ink)
                }
                DatePicker(selection: time(\.to), displayedComponents: .hourAndMinute) {
                    Text("To").font(Fonts.body).foregroundStyle(P.ink)
                }
            }
        }
        .tint(P.ink)
        .scrollContentBackground(.hidden)
        .background(P.side)
        .navigationTitle("Sounds")
        .navigationBarTitleDisplayMode(.inline)
        .onChange(of: prefs) { _, now in
            now.save()
            session.send(now.frame)
        }
    }

    /// A quiet hour, `HH:MM`, as the time picker shows it.
    private func time(_ key: WritableKeyPath<SoundPrefs, String>) -> Binding<Date> {
        Binding(
            get: {
                let parts = prefs[keyPath: key].split(separator: ":").compactMap { Int($0) }
                var d = DateComponents()
                d.hour = parts.first ?? 0
                d.minute = parts.count > 1 ? parts[1] : 0
                return Calendar.current.date(from: d) ?? .now
            },
            set: { date in
                let c = Calendar.current.dateComponents([.hour, .minute], from: date)
                prefs[keyPath: key] = String(format: "%02d:%02d", c.hour ?? 0, c.minute ?? 0)
            }
        )
    }
}
