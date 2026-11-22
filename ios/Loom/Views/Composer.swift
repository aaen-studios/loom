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
    @State private var photos: [PhotosPickerItem] = []
    @State private var choosingPhotos = false
    @State private var choosingFiles = false
    @FocusState private var focused: Bool

    var body: some View {
        let model = session.model
        let busy = model.busy
        VStack(alignment: .leading, spacing: Tokens.Space.s) {
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
                Menu {
                    Button {
                        choosingPhotos = true
                    } label: {
                        Label("Photos", systemImage: "photo.on.rectangle")
                    }
                    Button {
                        choosingFiles = true
                    } label: {
                        Label("Files", systemImage: "folder")
                    }
                } label: {
                    Image(systemName: "plus")
                        .font(.system(size: 17, weight: .medium))
                        .foregroundStyle(P.ink2)
                        .frame(width: Tokens.Control.iconComposer, height: Tokens.Control.iconComposer)
                        .contentShape(Circle())
                }
                .accessibilityLabel("Add photos or files")

                TextField("Reply to \(agentName)", text: $text, axis: .vertical)
                    .font(Fonts.ui(Tokens.PhoneSize.composer))
                    .foregroundStyle(P.ink)
                    .lineLimit(1...6)
                    .focused($focused)
                    .padding(.vertical, 8)

                Button {
                    if busy && text.isEmpty && model.attachments.isEmpty {
                        session.send(model.cancel())
                    } else if let frame = model.send(text) {
                        session.send(frame)
                        text = ""
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
        .background(P.bg)
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
        guard let data = try? await item.loadTransferable(type: Data.self),
              let image = UIImage(data: data),
              let jpeg = image.jpegData(compressionQuality: 0.85)
        else {
            session.model.notice = "That photo couldn't be read."
            return
        }
        // The core names the file by its path, so each photo gets a folder of its own.
        let stamp = Date.now.formatted(.verbatim("\(year: .defaultDigits)\(month: .twoDigits)\(day: .twoDigits)-\(hour: .twoDigits(clock: .twentyFourHour, hourCycle: .zeroBased))\(minute: .twoDigits)\(second: .twoDigits)", timeZone: .current, calendar: .current))
        let name = "Photo \(stamp).jpg"
        let folder = Composer.outbox().appendingPathComponent(UUID().uuidString, isDirectory: true)
        let url = folder.appendingPathComponent(name)
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try jpeg.write(to: url)
            session.send(session.model.attach(path: url.path, name: name))
        } catch {
            session.model.notice = "That photo couldn't be kept to send."
        }
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

    var body: some View {
        HStack(spacing: Tokens.Space.s) {
            if attachment.path.hasSuffix(".jpg"), let image = UIImage(contentsOfFile: attachment.path) {
                Image(uiImage: image).resizable().scaledToFill().frame(width: 28, height: 28).clipShape(RoundedRectangle(cornerRadius: 6))
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
    }
}
