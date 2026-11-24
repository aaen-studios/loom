import Photos
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// Where the person writes: text, photos and files, and Send, or Stop while the agent
/// answers.
struct Composer: View {
    @Environment(Session.self) private var session
    var agentName: String
    var sent: () -> Void
    @State private var text = ""
    /// The mentions the @ picker put in the text, which go as links when it's sent.
    @State private var mentions: [Mention] = []
    @State private var photos: [PhotosPickerItem] = []
    @State private var choosingPhotos = false
    @State private var choosingFiles = false
    /// Whether the + sheet is up, and whether it opened with its search, from a /.
    @State private var plusOpen = false
    @State private var plusSearch = false
    @FocusState private var focused: Bool

    var body: some View {
        let model = session.model
        let mention = mentionQuery(text)
        let picks = mention.map { model.picks($0.query) } ?? []
        VStack(spacing: Tokens.Space.xs) {
            if let mention, !picks.isEmpty {
                MentionPicker(picks: picks) { pick in
                    guard let made = model.pick(pick, at: mention.at, in: text) else { return }
                    text = made.text
                    mentions.append(made.mention)
                }
                .padding(.horizontal, Tokens.Space.m)
                .padding(.top, Tokens.Space.xs)
            }
            box
        }
        .background(P.bg)
        .onChange(of: text) { _, now in
            // A / alone opens the + sheet with its search.
            if now == "/" {
                text = ""
                plusSearch = true
                plusOpen = true
                return
            }
            // Follows the @ being typed, and asks for the files that match it.
            if let q = mentionQuery(now) { session.send(model.findFiles(q.query)) }
            if now.isEmpty { mentions = [] }
        }
    }

    /// The composer itself: attachments over the field, with + before it and Send after.
    private var box: some View {
        let model = session.model
        let busy = model.busy
        return VStack(alignment: .leading, spacing: Tokens.Space.s) {
            if !model.attachments.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: Tokens.Space.s) {
                        ForEach(model.attachments) { a in
                            AttachmentChip(attachment: a) { model.unattach(a.id) }
                        }
                    }
                    .padding(.horizontal, Tokens.Space.xs)
                }
            }
            HStack(alignment: .bottom, spacing: Tokens.Space.s) {
                Button {
                    plusSearch = false
                    plusOpen = true
                } label: {
                    Image(systemName: "plus")
                        .font(.system(size: 17, weight: .medium))
                        .foregroundStyle(P.ink2)
                        .frame(width: Tokens.Control.iconComposer, height: Tokens.Control.iconComposer)
                        .contentShape(Circle())
                }
                .accessibilityLabel("Attach, skills and routines")

                TextField("Reply to \(agentName)", text: $text, axis: .vertical)
                    .font(Fonts.ui(Tokens.PhoneSize.composer))
                    .foregroundStyle(P.ink)
                    .lineLimit(1...6)
                    .focused($focused)
                    .padding(.vertical, 8)
                    // As far from the plus as the plus is from the edge, with the 2 points
                    // the field pads its words itself.
                    .padding(.leading, Tokens.Space.xs - Tokens.Space.s - 2)

                Button {
                    if busy && text.isEmpty && model.attachments.isEmpty {
                        session.send(model.cancel())
                    } else if let frame = model.send(text, mentions: mentions) {
                        session.send(frame)
                        text = ""
                        mentions = []
                        sent()
                    }
                } label: {
                    Image(systemName: busy && text.isEmpty && model.attachments.isEmpty ? "stop.fill" : "arrow.up")
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(P.buttonInk)
                        .frame(width: Tokens.Control.iconComposer, height: Tokens.Control.iconComposer)
                        .background(P.button, in: Circle())
                        .opacity(canSend || busy ? 1 : 0.35)
                }
                .disabled(!(canSend || busy))
                .accessibilityLabel(busy && text.isEmpty ? "Stop" : "Send")
                .padding(.bottom, 1)
            }
        }
        .padding(.leading, Tokens.Space.xs)
        .padding(.trailing, Tokens.Space.xs + 2)
        .padding(.vertical, Tokens.Space.xs + 2)
        .background(P.raised, in: RoundedRectangle(cornerRadius: Tokens.Radius.composer, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.composer, style: .continuous).strokeBorder(P.line, lineWidth: 1))
        .padding(.horizontal, Tokens.Space.m)
        .padding(.top, Tokens.Space.xs)
        .padding(.bottom, Tokens.Space.s)
        .photosPicker(isPresented: $choosingPhotos, selection: $photos, maxSelectionCount: 10, matching: .images)
        .onChange(of: photos) { _, items in
            photos = []
            for item in items { Task { await attachPhoto(item) } }
        }
        .fileImporter(isPresented: $choosingFiles, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            if case .success(let urls) = result {
                for url in urls { attachFile(url) }
            }
        }
        .sheet(isPresented: $plusOpen) {
            PlusSheet(take: take, searchFirst: plusSearch)
        }
    }

    /// Does what the + sheet picked, and shuts it.
    private func take(_ what: PlusTake) {
        let model = session.model
        plusOpen = false
        switch what {
        case .photos:
            choosingPhotos = true
        case .files:
            choosingFiles = true
        case .photo(let asset):
            let options = PHImageRequestOptions()
            options.isNetworkAccessAllowed = true
            options.version = .current
            PHImageManager.default().requestImageDataAndOrientation(for: asset, options: options) { data, _, _, _ in
                guard let data else { return }
                Task { @MainActor in await attachPhoto(data: data) }
            }
        case .sent(let f):
            _ = model.attachSent(f)
        case .skill(let name):
            // Where the person was writing, as the @ picker puts a mention.
            let mention = skillMention(name)
            let gap = text.isEmpty || text.hasSuffix(" ") ? "" : " "
            text += gap + mention.words + " "
            mentions.append(mention)
            focused = true
        }
    }

    private var canSend: Bool {
        let model = session.model
        let hasText = !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        let ready = model.attachments.contains { $0.file != nil }
        return !model.uploading && (hasText || ready)
    }

    /// A photo goes up as a JPEG, which every agent's model takes (PLAN.md, "Files and
    /// images"), from a copy in the app's temporary folder.
    private func attachPhoto(_ item: PhotosPickerItem) async {
        guard let data = try? await item.loadTransferable(type: Data.self) else {
            session.model.notice = "That photo couldn't be read."
            return
        }
        await attachPhoto(data: data)
    }

    /// A photo's bytes, from the picker or the + sheet's row of newest photos.
    private func attachPhoto(data: Data) async {
        // The core names the file by its path, so each photo gets a folder of its own.
        let stamp = Date.now.formatted(.verbatim("\(year: .defaultDigits)\(month: .twoDigits)\(day: .twoDigits)-\(hour: .twoDigits(clock: .twentyFourHour, hourCycle: .zeroBased))\(minute: .twoDigits)\(second: .twoDigits)", timeZone: .current, calendar: .current))
        let name = "Photo \(stamp).jpg"
        let folder = Composer.outbox().appendingPathComponent(UUID().uuidString, isDirectory: true)
        let url = folder.appendingPathComponent(name)
        guard (try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)) != nil else {
            session.model.notice = "That photo couldn't be kept to send."
            return
        }
        // Decoded and written as a JPEG off the main thread, and its chip's picture made
        // there too.
        let made = await Task.detached(priority: .userInitiated) { Pictures.jpeg(data, to: url) }.value
        guard made else {
            session.model.notice = "That photo couldn't be read."
            return
        }
        _ = await Pictures.load(url.path, side: AttachmentChip.side)
        session.send(session.model.attach(path: url.path, name: name))
    }

    /// A file from Files is copied in first, since its own place is only open for a
    /// moment.
    private func attachFile(_ url: URL) {
        let open = url.startAccessingSecurityScopedResource()
        defer { if open { url.stopAccessingSecurityScopedResource() } }
        let folder = Composer.outbox().appendingPathComponent(UUID().uuidString, isDirectory: true)
        let copy = folder.appendingPathComponent(url.lastPathComponent)
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try FileManager.default.copyItem(at: url, to: copy)
            session.send(session.model.attach(path: copy.path, name: url.lastPathComponent))
        } catch {
            session.model.notice = "\(url.lastPathComponent) couldn't be read."
        }
    }

    /// Where files wait to go up.
    static func outbox() -> URL {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("outgoing", isDirectory: true)
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }
}

/// A file picked for the next message, while it goes up and once it's there.
private struct AttachmentChip: View {
    var attachment: Attachment
    var remove: () -> Void
    /// How big its picture is, in points.
    static let side: CGFloat = 28
    @State private var picture: UIImage?

    var body: some View {
        HStack(spacing: Tokens.Space.s) {
            // A picture made small off the main thread, not the photo itself.
            if attachment.path.hasSuffix(".jpg"), let image = picture ?? Pictures.kept(attachment.path, side: Self.side) {
                Image(uiImage: image).resizable().scaledToFill().frame(width: Self.side, height: Self.side).clipShape(RoundedRectangle(cornerRadius: 6))
            } else {
                Image(systemName: "doc.text").foregroundStyle(P.ink3)
            }
            VStack(alignment: .leading, spacing: 0) {
                Text(attachment.name).font(Fonts.caption).foregroundStyle(P.ink).lineLimit(1).frame(maxWidth: 140, alignment: .leading)
                Group {
                    if let error = attachment.error {
                        Text(error).foregroundStyle(P.bad)
                    } else if let f = attachment.file {
                        Text(When.size(f.size)).foregroundStyle(P.ink3)
                    } else {
                        Text("Sending…").foregroundStyle(P.ink3)
                    }
                }
                .font(Fonts.caption)
                .lineLimit(1)
            }
            Button(action: remove) {
                Image(systemName: "xmark").font(.system(size: 10, weight: .semibold)).foregroundStyle(P.ink3).frame(width: 22, height: 22)
            }
            .accessibilityLabel("Remove \(attachment.name)")
        }
        .padding(.leading, Tokens.Space.s)
        .padding(.trailing, 2)
        .padding(.vertical, 5)
        .background(P.bubble, in: RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous))
        .task(id: attachment.path) {
            guard attachment.path.hasSuffix(".jpg"), Pictures.kept(attachment.path, side: Self.side) == nil else { return }
            picture = await Pictures.load(attachment.path, side: Self.side)
        }
    }
}

/// The @ picker, over the composer (PLAN.md, "Mentions"): the person's other agents that
/// match, each with its thread and its name in its colour and the first line of its
/// instructions, then the files in this agent's folder and the shared one, each with its
/// icon, its name and its folder. A tap takes one.
private struct MentionPicker: View {
    var picks: [Pick]
    var take: (Pick) -> Void

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Tokens.Radius.card, style: .continuous)
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(picks.enumerated()), id: \.offset) { _, pick in
                Button {
                    take(pick)
                } label: {
                    row(pick)
                        .padding(.horizontal, Tokens.Space.m)
                        .frame(maxWidth: .infinity, minHeight: 40, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.vertical, Tokens.Space.xs)
        .background(P.raised, in: shape)
        .overlay(shape.strokeBorder(P.lineStrong, lineWidth: 1))
        .shadow(color: .black.opacity(0.08), radius: 12, y: 4)
    }

    @ViewBuilder
    private func row(_ pick: Pick) -> some View {
        HStack(spacing: Tokens.Space.s) {
            switch pick {
            case .agent(let a, let about):
                Image(uiImage: MentionGlyph.thread(a.outline))
                    .renderingMode(.template)
                    .foregroundStyle(P.mention(a.color))
                    .frame(width: 18)
                Text(a.name)
                    .font(Fonts.ui(Tokens.PhoneSize.small, .medium, relativeTo: .subheadline))
                    .foregroundStyle(P.mention(a.color))
                    .layoutPriority(1)
                Text(about).font(Fonts.small).foregroundStyle(P.ink3)
            case .file(let f):
                Image(systemName: isPicture(f.name) ? "photo" : "doc")
                    .font(.system(size: 13))
                    .foregroundStyle(P.ink3)
                    .frame(width: 18)
                Text(f.name).font(Fonts.small).foregroundStyle(P.ink).layoutPriority(1)
                Text(f.place).font(Fonts.small).foregroundStyle(P.ink3)
            }
        }
        .lineLimit(1)
        .truncationMode(.tail)
        .accessibilityElement(children: .combine)
    }
}
