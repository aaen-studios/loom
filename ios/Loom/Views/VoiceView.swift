import SwiftUI

// Settings › Voice on the phone (PLAN.md, "Voice"): Dictation and Speech, each its engines
// as tiles in black and white with Use, Add key or Change key, and its rows under them;
// Live, live mode's rows; and the voice_setup card, the same tile and rows read only, with
// Add key while it waits on one. The model (Voice.swift) reads `voice.get` and says what
// to send.

/// Settings › Voice.
struct VoiceView: View {
    @Environment(Session.self) private var session
    @AppStorage(Prefs.holdToTalk) private var holdToTalk = false
    @State private var tab: String
    /// The tile whose key field is open, as `kind/engine`.
    @State private var editing: String?

    /// Opens on `kind`'s tab, or Speech with `-voiceSpeech` at launch and Live with
    /// `-voiceLive`, for their screenshots.
    init(kind: String = "dictation") {
        let args = ProcessInfo.processInfo.arguments
        _tab = State(initialValue: args.contains("-voiceSpeech") ? "speech" : args.contains("-voiceLive") ? "live" : kind)
    }

    var body: some View {
        let model = session.model
        let voice = model.voice
        List {
            Section {
                if !voice.came {
                    ProgressView().frame(maxWidth: .infinity)
                        .listRowBackground(Color.clear)
                }
                ForEach(voice.tiles(tab)) { t in
                    VStack(alignment: .leading, spacing: Tokens.Space.s) {
                        VoiceTileView(tile: t) { tapped(t) }
                        if editing == "\(tab)/\(t.id)" {
                            VoiceKeyField(adding: "Save", sending: voice.keySending.contains(t.id), cancel: { editing = nil }) { key in
                                session.send(model.voiceKey(t.id, key, use: t.inUse ? nil : tab))
                                editing = nil
                            }
                        }
                    }
                    .listRowInsets(EdgeInsets(top: Tokens.Space.xs, leading: Tokens.PhoneSpace.gutter, bottom: Tokens.Space.xs, trailing: Tokens.PhoneSpace.gutter))
                    .listRowBackground(Color.clear)
                    .listRowSeparator(.hidden)
                }
            }
            if voice.came {
                switch tab {
                case "speech": speechRows(voice)
                case "live": liveRows(voice)
                default: dictationRows(voice)
                }
            }
        }
        .tint(P.ink)
        .scrollContentBackground(.hidden)
        .background(P.side)
        .safeAreaInset(edge: .top, spacing: 0) {
            Picker("Voice", selection: $tab) {
                Text("Dictation").tag("dictation")
                Text("Speech").tag("speech")
                Text("Live").tag("live")
            }
            .pickerStyle(.segmented)
            .padding(.horizontal, Tokens.PhoneSpace.gutter)
            .padding(.vertical, Tokens.Space.s)
            .background(P.side)
        }
        .navigationTitle("Voice")
        .navigationBarTitleDisplayMode(.inline)
        .onChange(of: tab) { _, _ in editing = nil }
        .onAppear {
            session.send(model.loadVoice())
        }
        .task(id: "\(tab)/\(voice.settings.speechEngine)") {
            // The Voice row's picker, once Speech shows an engine.
            let engine = voice.settings.speechEngine
            if tab == "speech", !engine.isEmpty, model.voice.voices[engine] == nil {
                session.send(model.voiceVoices(engine))
            }
        }
    }

    private func tapped(_ t: VoiceTile) {
        switch t.button {
        case .use:
            session.send(session.model.voiceUse(tab, t.id))
        case .addKey, .changeKey:
            let id = "\(tab)/\(t.id)"
            editing = editing == id ? nil : id
        case nil:
            break
        }
    }

    // MARK: Dictation

    private func dictationRows(_ voice: VoiceState) -> some View {
        Section {
            Picker(selection: Binding(get: { voice.settings.language }, set: { put("dictation", ["language": JSON($0)]) })) {
                ForEach(voice.languages) { l in
                    Text(l.name).tag(l.code)
                }
            } label: {
                Text("Language").font(Fonts.body).foregroundStyle(P.ink)
            }
            .tint(P.ink3)
            Toggle(isOn: $holdToTalk) {
                Text("Hold to talk").font(Fonts.body).foregroundStyle(P.ink)
            }
        }
    }

    // MARK: Speech

    private func speechRows(_ voice: VoiceState) -> some View {
        let s = voice.settings
        let options = voiceOptions(voice)
        let current = options.first { $0.id == s.voice }
        return Section {
            if !s.speechEngine.isEmpty {
                HStack(spacing: Tokens.Space.m) {
                    Picker(selection: Binding(get: { s.voice }, set: { id in
                        put("speech", ["voice": JSON(id), "voice_name": JSON(options.first { $0.id == id }?.name ?? id)])
                    })) {
                        ForEach(options) { o in
                            Text(o.name).tag(o.id)
                        }
                    } label: {
                        Text("Voice").font(Fonts.body).foregroundStyle(P.ink)
                    }
                    .tint(P.ink3)
                    Button {
                        session.send(session.model.voicePreview(engine: s.speechEngine, voice: s.voice, name: current?.name ?? s.voiceName))
                    } label: {
                        Image(systemName: "play")
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(P.ink2)
                            .frame(width: 30, height: 30)
                            .overlay(RoundedRectangle(cornerRadius: 7, style: .continuous).strokeBorder(P.line))
                    }
                    .buttonStyle(.borderless)
                    .accessibilityLabel("Play \(current?.name ?? "the voice")")
                }
                if let why = voice.voicesError[s.speechEngine] {
                    Text(why).font(Fonts.small).foregroundStyle(P.ink3)
                }
            }
            VStack(alignment: .leading, spacing: Tokens.Space.s) {
                Text("Speed").font(Fonts.body).foregroundStyle(P.ink)
                Picker("Speed", selection: Binding(get: { s.speed }, set: { put("speech", ["speed": .number($0)]) })) {
                    ForEach(voice.speeds, id: \.self) { v in
                        Text(voiceSpeed(v)).tag(v)
                    }
                }
                .pickerStyle(.segmented)
            }
            .padding(.vertical, Tokens.Space.xs)
            Picker(selection: Binding(get: { s.readAloud }, set: { put("speech", ["read_aloud": JSON($0)]) })) {
                ForEach(voice.readAloud, id: \.self) { r in
                    Text(readAloudWords(r)).tag(r)
                }
            } label: {
                Text("Read replies aloud").font(Fonts.body).foregroundStyle(P.ink)
            }
            .tint(P.ink3)
        }
    }

    // MARK: Live

    private func liveRows(_ voice: VoiceState) -> some View {
        let s = voice.settings
        return Section {
            Toggle(isOn: Binding(get: { s.bargeIn }, set: { live(["barge_in": JSON($0)]) })) {
                Text("Talk over a reply to stop it").font(Fonts.body).foregroundStyle(P.ink)
            }
            Picker(selection: Binding(get: { s.pause }, set: { live(["pause": .number($0)]) })) {
                ForEach(voice.pauses, id: \.self) { p in
                    Text(pauseWords(p)).tag(p)
                }
            } label: {
                Text("Send when I stop talking").font(Fonts.body).foregroundStyle(P.ink)
            }
            .tint(P.ink3)
            Toggle(isOn: Binding(get: { s.captions }, set: { live(["captions": JSON($0)]) })) {
                Text("Show what's said").font(Fonts.body).foregroundStyle(P.ink)
            }
        }
    }

    private func live(_ change: [String: JSON]) {
        session.send(session.model.voiceLive(change))
    }

    /// The engine's voices, with the one set first when the list doesn't have it (its
    /// first voice, before the core has the list).
    private func voiceOptions(_ voice: VoiceState) -> [VoiceOption] {
        let s = voice.settings
        var options = voice.voices[s.speechEngine] ?? []
        if !options.contains(where: { $0.id == s.voice }) {
            options.insert(VoiceOption(id: s.voice, name: s.voiceName.isEmpty ? (s.voice.isEmpty ? "Default" : s.voice) : s.voiceName), at: 0)
        }
        return options
    }

    private func put(_ kind: String, _ change: [String: JSON]) {
        session.send(session.model.voicePut(kind, change))
    }
}

/// An engine's tile, in black and white: its maker's mark on a tile in ink, its name and one quiet
/// line, and its button, or none on a card. The engine in use is outlined in ink, with a
/// check and In use.
struct VoiceTileView: View {
    var tile: VoiceTile
    /// Its button's action, or none for a tile that's only shown.
    var act: (() -> Void)?

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous)
        HStack(spacing: Tokens.Space.m) {
            RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous)
                .fill(P.button)
                .frame(width: Tokens.Control.iconLarge, height: Tokens.Control.iconLarge)
                .overlay {
                    // Its maker's mark in the ink on it, as Spools draws an entry's; else
                    // the cloud or the laptop.
                    if let glyph = tile.mark.flatMap({ UIImage(named: "voice-\($0)") }) {
                        Image(uiImage: glyph)
                            .renderingMode(.template)
                            .resizable()
                            .scaledToFit()
                            .frame(width: (Tokens.Control.iconLarge * 0.56).rounded(), height: (Tokens.Control.iconLarge * 0.56).rounded())
                            .foregroundStyle(P.buttonInk)
                    } else {
                        Image(systemName: tile.icon == "laptop" ? "laptopcomputer" : "cloud")
                            .font(.system(size: 15, weight: .medium))
                            .foregroundStyle(P.buttonInk)
                    }
                }
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 2) {
                Text(tile.name).font(Fonts.ui(Tokens.PhoneSize.body, .semibold)).foregroundStyle(P.ink).lineLimit(1)
                Text(tile.line).font(Fonts.small).foregroundStyle(P.ink3).lineLimit(2)
                if let why = tile.why {
                    Text(why).font(Fonts.small).foregroundStyle(P.ink2).lineLimit(2)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            VStack(alignment: .trailing, spacing: Tokens.Space.xs + 2) {
                if tile.inUse {
                    HStack(spacing: 3) {
                        Image(systemName: "checkmark").font(.system(size: 10, weight: .bold))
                        Text("In use")
                    }
                    .font(Fonts.ui(Tokens.PhoneSize.caption, .medium))
                    .foregroundStyle(P.ink)
                }
                if let act, let b = tile.button {
                    Button(b.words, action: act)
                        .buttonStyle(CardButtonStyle(primary: b == .use))
                }
            }
            .fixedSize()
        }
        .padding(Tokens.Space.m)
        .background(P.bg, in: shape)
        .overlay(shape.strokeBorder(tile.inUse ? P.ink : P.line, lineWidth: tile.inUse ? 1.5 : 1))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(tile.inUse ? "\(tile.name), in use" : tile.name)
    }
}

/// A field for a service's key: masked, with an eye to show it, and its button, with
/// Cancel beside it on Settings › Voice.
struct VoiceKeyField: View {
    var adding: String
    var sending = false
    var cancel: (() -> Void)?
    var save: (String) -> Void
    @State private var key = ""
    @State private var shown = false

    var body: some View {
        let typed = key.trimmingCharacters(in: .whitespacesAndNewlines)
        VStack(alignment: cancel == nil ? .leading : .trailing, spacing: Tokens.Space.s) {
            HStack(spacing: Tokens.Space.xs) {
                Group {
                    if shown {
                        TextField("API key", text: $key)
                    } else {
                        SecureField("API key", text: $key)
                    }
                }
                .font(Fonts.code)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.done)
                .onSubmit { if !typed.isEmpty { save(typed) } }
                Button {
                    shown.toggle()
                } label: {
                    Image(systemName: shown ? "eye.slash" : "eye").foregroundStyle(P.ink3)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(shown ? "Hide" : "Show")
            }
            .boxed()
            HStack(spacing: Tokens.Space.s) {
                if let cancel {
                    Button("Cancel", action: cancel).buttonStyle(CardButtonStyle())
                }
                Button(sending ? "Adding…" : adding) { save(typed) }
                    .buttonStyle(CardButtonStyle(primary: true))
                    .disabled(typed.isEmpty || sending)
            }
        }
    }
}

/// loom's own card for an agent's voice_setup (PLAN.md, "Voice"): the engine's tile and
/// the rows as the agent set them, read only, as Settings › Voice draws them; while it
/// waits on a key, its field and Add; and Voice settings, which opens the page.
struct VoiceSetupCard: View {
    @Environment(Session.self) private var session
    var card: Card
    @State private var settingsOpen = false

    var body: some View {
        let model = session.model
        if let v = card.voice {
            CardFrame(card.title) {
                VoiceTileView(tile: v.tile, act: nil)
                CardLines(lines: v.rows(model.voice).map { Shown.Line(label: $0.0, value: $0.1) })
                if card.state == "asking" {
                    VoiceKeyField(adding: "Add", sending: card.sending) { key in
                        UIImpactFeedbackGenerator(style: .light).impactOccurred()
                        session.send(model.voiceCardAdd(message: card.message, card: card.id, key: key))
                    }
                    if !card.error.isEmpty {
                        Text(card.error).font(Fonts.small).foregroundStyle(P.bad)
                    }
                }
                Button {
                    settingsOpen = true
                } label: {
                    HStack(spacing: Tokens.Space.xs) {
                        Text("Voice settings")
                        Image(systemName: "arrow.right").font(.system(size: 11, weight: .semibold))
                    }
                    .font(Fonts.ui(Tokens.PhoneSize.small, .medium))
                    .foregroundStyle(P.ink)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
            .frame(maxWidth: 420, alignment: .leading)
            .sheet(isPresented: $settingsOpen) {
                NavigationStack { VoiceView(kind: v.kind) }
            }
            .onAppear {
                // For the language's name.
                if !model.voice.came { session.send(model.loadVoice()) }
            }
        }
    }
}
