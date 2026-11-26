import PhotosUI
import SwiftUI
import UIKit

// The emoji on screen (PLAN.md, "Emoji"): the row over the composer while a name is
// typed after a colon, the picker the smiley opens, and Settings → Emoji.

/// An emoji `side` points square: Twemoji's picture, or one of the person's own once its
/// picture is here, else its name.
struct EmojiPicture: View {
    var emoji: String
    var side: CGFloat
    var paths: [String: String]

    var body: some View {
        if let image = Emoji.picture(of: emoji, side: side, paths: paths) {
            Image(uiImage: image)
                .accessibilityLabel(Emoji.asNames(emoji))
        } else {
            Text(Emoji.asNames(emoji)).font(Fonts.caption).foregroundStyle(P.ink3).lineLimit(1)
                .frame(width: side, height: side)
        }
    }
}

/// The suggestion row over the composer: up to eight emoji whose names start as typed,
/// the person's own first, each with its `:name:`. A tap takes one.
struct EmojiRowView: View {
    var items: [EmojiItem]
    var paths: [String: String]
    var take: (EmojiItem) -> Void

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous)
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: Tokens.Space.xs) {
                ForEach(Array(items.prefix(8).enumerated()), id: \.offset) { i, item in
                    Button {
                        take(item)
                    } label: {
                        HStack(spacing: Tokens.Space.xs) {
                            EmojiPicture(emoji: item.emoji, side: 22, paths: paths)
                            Text(":\(item.name):").font(Fonts.mono(Tokens.PhoneSize.caption)).foregroundStyle(P.ink2).lineLimit(1)
                        }
                        .padding(.horizontal, Tokens.Space.s)
                        .frame(height: 36)
                        .background(i == 0 ? P.hover : .clear, in: RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous))
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel(":\(item.name):")
                }
            }
            .padding(Tokens.Space.xs)
        }
        .background(P.raised, in: shape)
        .overlay(shape.strokeBorder(P.lineStrong, lineWidth: 1))
        .shadow(color: .black.opacity(0.08), radius: 12, y: 4)
    }
}

/// The emoji picker, a sheet: **Find an emoji**, the skin tone, a tab for each group,
/// and the grid under them with a heading for each, the person's own with a dashed +
/// that opens Settings → Emoji. A tap puts an emoji in and the sheet stays up, for the
/// next; Recently used keeps its order until it opens again (`Emojis.openPicker`).
struct EmojiPicker: View {
    @Environment(Session.self) private var session
    var pick: (EmojiItem) -> Void
    @State private var query = ""
    @State private var settingsOpen = false
    @AppStorage(Prefs.skinTone) private var skinTone = 0

    private static let columns = Array(repeating: GridItem(.flexible(), spacing: 0), count: 8)

    var body: some View {
        let emoji = session.model.emoji
        ScrollViewReader { proxy in
            VStack(spacing: 0) {
                HStack(spacing: Tokens.Space.s) {
                    HStack(spacing: Tokens.Space.s) {
                        Image(systemName: "magnifyingglass").foregroundStyle(P.ink3)
                        TextField("Find an emoji", text: $query)
                            .font(Fonts.ui(Tokens.PhoneSize.small))
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                    }
                    .padding(.horizontal, Tokens.Space.m)
                    .frame(height: 40)
                    .background(P.bg, in: RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous))
                    ToneMenu(tone: $skinTone)
                }
                .padding(.horizontal, Tokens.Space.m)
                .padding(.bottom, Tokens.Space.s)
                if query.isEmpty {
                    HStack(spacing: 0) {
                        ForEach(emoji.groups) { g in
                            Button {
                                withAnimation { proxy.scrollTo(g.id, anchor: .top) }
                            } label: {
                                Image(systemName: Self.icon(g.id))
                                    .font(.system(size: 15))
                                    .foregroundStyle(P.ink3)
                                    .frame(maxWidth: .infinity, minHeight: 32)
                                    .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel(g.name)
                        }
                    }
                    .padding(.horizontal, Tokens.Space.s)
                    Rectangle().fill(P.line).frame(height: 1)
                }
                ScrollView {
                    LazyVGrid(columns: Self.columns, spacing: 0, pinnedViews: [.sectionHeaders]) {
                        if !query.isEmpty {
                            let found = emoji.found?.query == query ? emoji.found?.items ?? [] : []
                            Section {
                                ForEach(found.indices.map { ("found/\($0)", found[$0]) }, id: \.0) { cell($0.1, paths: emoji.paths) }
                            }
                        } else {
                            ForEach(emoji.groups) { g in
                                Section {
                                    // An emoji can be in Recently used and its own group too:
                                    // each cell's id is its group and its place.
                                    ForEach(g.emoji.indices.map { ("\(g.id)/\($0)", g.emoji[$0]) }, id: \.0) { cell($0.1, paths: emoji.paths) }
                                    if g.id == "custom" { addCell }
                                } header: {
                                    Text(g.name)
                                        .font(Fonts.ui(Tokens.PhoneSize.caption, .medium, relativeTo: .caption))
                                        .foregroundStyle(P.ink3)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                        .padding(.horizontal, Tokens.Space.s)
                                        .padding(.vertical, Tokens.Space.xs)
                                        .background(P.raised)
                                        .id(g.id)
                                }
                            }
                        }
                    }
                    .padding(.horizontal, Tokens.Space.s)
                    .padding(.bottom, Tokens.Space.m)
                }
            }
        }
        .padding(.top, Tokens.Space.m)
        .background(P.raised)
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .presentationCornerRadius(Tokens.Radius.card * 2)
        .onChange(of: query) { _, now in
            let q = now.trimmingCharacters(in: .whitespaces)
            if !q.isEmpty { session.send(session.model.emoji.find(q)) }
        }
        .onChange(of: skinTone) { _, now in
            let model = session.model
            model.emoji.tone = now
            session.send(model.emoji.picker())
            if !query.isEmpty { session.send(model.emoji.find(query)) }
        }
        .sheet(isPresented: $settingsOpen) {
            NavigationStack { EmojiSettingsView() }
        }
    }

    private func cell(_ item: EmojiItem, paths: [String: String]) -> some View {
        Button {
            pick(item)
        } label: {
            EmojiPicture(emoji: item.emoji, side: 30, paths: paths)
                .frame(maxWidth: .infinity, minHeight: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(item.names.first.map { ":\($0):" } ?? item.emoji)
    }

    private var addCell: some View {
        Button {
            settingsOpen = true
        } label: {
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .strokeBorder(P.lineStrong, style: StrokeStyle(lineWidth: 1, dash: [3, 3]))
                .frame(width: 30, height: 30)
                .overlay(Image(systemName: "plus").font(.system(size: 13, weight: .medium)).foregroundStyle(P.ink3))
                .frame(maxWidth: .infinity, minHeight: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Add emoji")
    }

    /// Each group's tab, as the desktop's.
    static func icon(_ group: String) -> String {
        switch group {
        case "recent": "clock"
        case "custom": "star"
        case "smileys": "face.smiling"
        case "people": "hand.raised"
        case "nature": "leaf"
        case "food": "fork.knife"
        case "travel": "airplane"
        case "activities": "volleyball"
        case "objects": "lightbulb"
        case "symbols": "heart"
        default: "flag"
        }
    }
}

/// The six hands of the skin tones: none, then light to dark.
private let hands = ["270b", "270b-1f3fb", "270b-1f3fc", "270b-1f3fd", "270b-1f3fe", "270b-1f3ff"]

/// The skin tone as a hand, which opens the six.
private struct ToneMenu: View {
    @Binding var tone: Int

    var body: some View {
        Menu {
            ForEach(0..<6, id: \.self) { t in
                Button {
                    tone = t
                } label: {
                    if let image = Emoji.picture(hands[t], side: 22) { Image(uiImage: image) }
                }
            }
        } label: {
            if let image = Emoji.picture(hands[min(max(tone, 0), 5)], side: 26) {
                Image(uiImage: image).frame(width: 40, height: 40)
            }
        }
        .accessibilityLabel("Skin tone")
    }
}

/// Settings → Emoji: the person's own as tiles, an Add tile first and how many there
/// are, each one renamed or deleted from its menu, with Undo; then the skin tone, the
/// emoticons and the smiley in the composer.
struct EmojiSettingsView: View {
    @Environment(Session.self) private var session
    @AppStorage(Prefs.skinTone) private var skinTone = 0
    @AppStorage(Prefs.emoticons) private var emoticons = false
    @AppStorage(Prefs.emojiButton) private var emojiButton = true
    @State private var choosing = false
    @State private var photo: PhotosPickerItem?
    /// A picture ready to go up, waiting for its name.
    @State private var adding: String?
    @State private var name = ""
    @State private var renaming: CustomEmoji?

    var body: some View {
        let emoji = session.model.emoji
        List {
            Section {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 72), spacing: Tokens.Space.s)], spacing: Tokens.Space.m) {
                    Button {
                        choosing = true
                    } label: {
                        tile {
                            RoundedRectangle(cornerRadius: 8, style: .continuous)
                                .strokeBorder(P.lineStrong, style: StrokeStyle(lineWidth: 1, dash: [3, 3]))
                                .frame(width: 40, height: 40)
                                .overlay(Image(systemName: emoji.uploading == nil ? "plus" : "arrow.up").foregroundStyle(P.ink3))
                        } name: {
                            Text(emoji.uploading.map { ":\($0):" } ?? "Add").foregroundStyle(P.ink3)
                        }
                    }
                    .buttonStyle(.plain)
                    .disabled(emoji.uploading != nil || emoji.own.count >= emoji.most)
                    ForEach(emoji.own) { e in
                        tile {
                            EmojiPicture(emoji: e.token, side: 40, paths: emoji.paths)
                                .overlay(alignment: .bottomTrailing) {
                                    if e.animated {
                                        Text("GIF").font(.system(size: 8, weight: .bold)).foregroundStyle(P.ink2)
                                            .padding(.horizontal, 2).background(P.raised2, in: RoundedRectangle(cornerRadius: 2))
                                            .offset(x: 4, y: 4)
                                    }
                                }
                        } name: {
                            Text(":\(e.name):").foregroundStyle(P.ink2)
                        }
                        .contextMenu {
                            Button("Rename", systemImage: "pencil") {
                                name = e.name
                                renaming = e
                            }
                            Button("Delete", systemImage: "trash", role: .destructive) {
                                session.send(session.model.emoji.delete(e.id))
                            }
                        }
                    }
                }
                .padding(.vertical, Tokens.Space.s)
                if let gone = emoji.deleted {
                    HStack {
                        Text("Deleted :\(gone.name):").font(Fonts.small).foregroundStyle(P.ink2)
                        Spacer()
                        Button("Undo") { session.send(session.model.emoji.restore()) }.font(Fonts.small)
                    }
                }
                if let error = emoji.error {
                    Text(error).font(Fonts.small).foregroundStyle(P.bad)
                }
            } header: {
                HStack {
                    Text("Your emoji")
                    Spacer()
                    Text("\(emoji.own.count) of \(emoji.most)")
                }
            }
            Section {
                HStack {
                    Text("Skin tone").font(Fonts.body).foregroundStyle(P.ink)
                    Spacer()
                    ForEach(0..<6, id: \.self) { t in
                        Button {
                            skinTone = t
                        } label: {
                            if let image = Emoji.picture(hands[t], side: 22) {
                                Image(uiImage: image)
                                    .padding(3)
                                    .background(skinTone == t ? P.hover : .clear, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                            }
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(t == 0 ? "No skin tone" : "Skin tone \(t)")
                    }
                }
                Toggle(isOn: $emoticons) {
                    turnInto.font(Fonts.body).foregroundStyle(P.ink)
                }
                Toggle(isOn: $emojiButton) {
                    Text("Show the emoji button in the composer").font(Fonts.body).foregroundStyle(P.ink)
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(P.side)
        .navigationTitle("Emoji")
        .navigationBarTitleDisplayMode(.inline)
        .photosPicker(isPresented: $choosing, selection: $photo, matching: .images)
        .onChange(of: photo) { _, item in
            photo = nil
            guard let item else { return }
            Task { await prepare(item) }
        }
        .onChange(of: skinTone) { _, now in session.model.emoji.tone = now }
        .alert("Name this emoji", isPresented: Binding(get: { adding != nil }, set: { if !$0 { adding = nil } })) {
            TextField("party_cat", text: $name)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            Button("Add") {
                if let path = adding { session.send(session.model.emoji.upload(path: path, name: emojiName(name))) }
                adding = nil
            }
            Button("Cancel", role: .cancel) { adding = nil }
        } message: {
            Text("2 to 32 of a–z, 0–9 and _")
        }
        .alert("Rename", isPresented: Binding(get: { renaming != nil }, set: { if !$0 { renaming = nil } })) {
            TextField("party_cat", text: $name)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            Button("Rename") {
                if let e = renaming { session.send(session.model.emoji.rename(e.id, to: emojiName(name))) }
                renaming = nil
            }
            Button("Cancel", role: .cancel) { renaming = nil }
        }
        .onAppear { session.model.emoji.tone = skinTone }
    }

    /// "Turn :) into 🙂", its 🙂 drawn with Twemoji.
    private var turnInto: Text {
        let side = Emoji.side(Tokens.PhoneSize.body)
        guard let image = Emoji.picture("1f642", side: side) else { return Text("Turn :) into 🙂") }
        return Text("Turn :) into \(Emoji.inline(image, side: side, label: "🙂"))")
    }

    private func tile<Picture: View, Name: View>(@ViewBuilder _ picture: () -> Picture, @ViewBuilder name: () -> Name) -> some View {
        VStack(spacing: Tokens.Space.xs) {
            picture().frame(width: 40, height: 40)
            name().font(Fonts.mono(Tokens.PhoneSize.caption - 1.5)).lineLimit(1).truncationMode(.middle)
        }
        .frame(maxWidth: .infinity)
        .contentShape(Rectangle())
    }

    /// A photo made 128 points square, its shape kept, as a PNG to go up, then asked a
    /// name for.
    private func prepare(_ item: PhotosPickerItem) async {
        guard let data = try? await item.loadTransferable(type: Data.self) else {
            session.model.emoji.error = "That photo couldn't be read."
            return
        }
        let url = Composer.outbox().appendingPathComponent("emoji-\(UUID().uuidString).png")
        let made = await Task.detached(priority: .userInitiated) { EmojiSettingsView.square(data, to: url) }.value
        guard made else {
            session.model.emoji.error = "That photo couldn't be read."
            return
        }
        name = ""
        adding = url.path
    }

    nonisolated static func square(_ data: Data, to url: URL) -> Bool {
        guard let image = UIImage(data: data) else { return false }
        let side: CGFloat = 128
        let format = UIGraphicsImageRendererFormat()
        format.scale = 1
        format.opaque = false
        let k = min(side / max(image.size.width, 1), side / max(image.size.height, 1))
        let w = image.size.width * k, h = image.size.height * k
        let png = UIGraphicsImageRenderer(size: CGSize(width: side, height: side), format: format).pngData { _ in
            image.draw(in: CGRect(x: (side - w) / 2, y: (side - h) / 2, width: w, height: h))
        }
        return (try? png.write(to: url)) != nil
    }
}

/// Settings' credits: Twemoji, which draws loom's emoji, under its licence.
struct EmojiCredits: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Link(destination: URL(string: "https://github.com/jdecked/twemoji")!) {
                Text("Emoji by Twemoji").font(Fonts.body).foregroundStyle(P.ink)
            }
            Text("© Twitter, Jason Sofonia and Justine De Caires").font(Fonts.caption).foregroundStyle(P.ink3)
            Link(destination: URL(string: "https://creativecommons.org/licenses/by/4.0/")!) {
                Text("CC-BY 4.0").font(Fonts.caption).underline()
            }
            .foregroundStyle(P.ink3)
        }
    }
}
