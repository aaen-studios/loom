import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers

/// Where the person writes: text, photos and files, and Send, or Stop while the agent
/// answers. What's attached, mentioned or pasted as a link shows as the sent message will
/// (PLAN.md, "Files and images", "Mentions" and "Links in the composer"): each one piece
/// in the words, and over them its picture, its fan, its pill or its card.
struct Composer: View {
    @Environment(Session.self) private var session
    var agentName: String
    var sent: () -> Void
    @State private var box = ComposerBox()
    @State private var photos: [PhotosPickerItem] = []
    @State private var choosingPhotos = false
    @State private var choosingFiles = false

    var body: some View {
        let model = session.model
        let mention = mentionQuery(box.typed)
        let picks = mention.map { model.picks($0.query) } ?? []
        VStack(spacing: Tokens.Space.xs) {
            if let mention, !picks.isEmpty {
                MentionPicker(picks: picks) { pick in
                    guard let agent = model.selected else { return }
                    let m: Mention
                    switch pick {
                    case .agent(let a, _): m = agentMention(id: a.id, name: a.name)
                    case .file(let f): m = fileMention(agent: agent, path: f.path, name: f.name)
                    }
                    box.put(Piece(id: m.link, text: m.words), replacingFrom: mention.at)
                }
                .padding(.horizontal, Tokens.Space.m)
                .padding(.top, Tokens.Space.xs)
            }
            composerBox
        }
        .background(P.bg)
        .onChange(of: box.typed) { _, now in
            // Follows the @ being typed, and asks for the files that match it.
            if let q = mentionQuery(now) { session.send(model.findFiles(q.query)) }
        }
        .onChange(of: box.attached) { _, ids in model.syncAttachments(ids) }
        .onAppear {
            box.opened = { piece in open(piece) }
            box.pasted = { url in session.send(model.previewLink(url)) }
        }
    }

    /// The composer itself: what's attached over the field, with + before it and Send
    /// after.
    private var composerBox: some View {
        let model = session.model
        let busy = model.busy
        let idle = box.typed.isEmpty && model.attachments.isEmpty
        return VStack(alignment: .leading, spacing: Tokens.Space.s) {
            if !model.attachments.isEmpty || !box.links.isEmpty {
                AttachedRow(links: box.links, take: { box.take($0) }, unpreview: { box.unpreview($0) })
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

                ComposerField(box: box, art: PieceArt(model: model))
                    .overlay(alignment: .topLeading) {
                        if box.typed.isEmpty {
                            Text("Reply to \(agentName)")
                                .font(Fonts.ui(Tokens.PhoneSize.composer))
                                .foregroundStyle(P.ink3)
                                .padding(.top, 8)
                                .allowsHitTesting(false)
                        }
                    }
                    // As far from the plus as the plus is from the edge.
                    .padding(.leading, Tokens.Space.xs - Tokens.Space.s)

                Button {
                    if busy && idle {
                        session.send(model.cancel())
                    } else if let frame = model.send(composerText(box.parts())) {
                        session.send(frame)
                        box.clear()
                        sent()
                    }
                } label: {
                    Image(systemName: busy && idle ? "stop.fill" : "arrow.up")
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(P.buttonInk)
                        .frame(width: Tokens.Control.iconComposer, height: Tokens.Control.iconComposer)
                        .background(P.button, in: Circle())
                        .opacity(canSend || busy ? 1 : 0.35)
                }
                .disabled(!(canSend || busy))
                .accessibilityLabel(busy && idle ? "Stop" : "Send")
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
    }

    private var canSend: Bool {
        let model = session.model
        let hasText = !box.typed.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        let ready = model.attachments.contains { $0.file != nil }
        return !model.uploading && (hasText || ready)
    }

    /// Opens what a tapped piece is: an agent's chat, a file, a picture, a page.
    private func open(_ piece: Piece) {
        let model = session.model
        switch LinkTo(piece.id) {
        case .agent(let id) where model.agent(id) != nil:
            model.openAgent = id
        case .file:
            session.send(model.openFileMention(piece.id))
        default:
            if piece.id.hasPrefix(PieceKind.link), let url = URL(string: String(piece.id.dropFirst(PieceKind.link.count))) {
                UIApplication.shared.open(url)
            } else if piece.id.hasPrefix(PieceKind.attach),
                      let a = model.attachments.first(where: { PieceKind.attach + $0.id == piece.id }),
                      let f = a.file, f.isImage {
                model.view([f])
            }
        }
    }

    /// Sends a file for the next message, and puts its piece in the words at the caret.
    private func attach(path: String, name: String) {
        let model = session.model
        session.send(model.attach(path: path, name: name))
        if let a = model.attachments.last { box.put(attachPiece(a)) }
    }

    /// A photo goes up as a JPEG, which every agent's model takes (PLAN.md, "Files and
    /// images"), from a copy in the app's temporary folder.
    private func attachPhoto(_ item: PhotosPickerItem) async {
        guard let data = try? await item.loadTransferable(type: Data.self) else {
            session.model.notice = "That photo couldn't be read."
            return
        }
        // The core names the file by its path, so each photo gets a folder of its own.
        let stamp = Date.now.formatted(.verbatim("\(year: .defaultDigits)\(month: .twoDigits)\(day: .twoDigits)-\(hour: .twoDigits(clock: .twentyFourHour, hourCycle: .zeroBased))\(minute: .twoDigits)\(second: .twoDigits)", timeZone: .current, calendar: .current))
        let name = "Photo \(stamp).jpg"
        let folder = Composer.outbox().appendingPathComponent(UUID().uuidString, isDirectory: true)
        let url = folder.appendingPathComponent(name)
        guard (try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)) != nil else {
            session.model.notice = "That photo couldn't be kept to send."
            return
        }
        // Decoded and written as a JPEG off the main thread, and its pictures made
        // there too.
        let made = await Task.detached(priority: .userInitiated) { Pictures.jpeg(data, to: url) }.value
        guard made else {
            session.model.notice = "That photo couldn't be read."
            return
        }
        _ = await Pictures.load(url.path, side: AttachedRow.picture)
        attach(path: url.path, name: name)
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
            attach(path: copy.path, name: url.lastPathComponent)
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

// MARK: The words and their pieces

/// The composer's words, each piece in them one attachment character, so it moves and
/// goes as one: the caret can't land inside it, Backspace takes it out whole, and the
/// field's undo puts it back. What the screen reads of it: the words as typed, with each
/// piece as U+FFFC, the attached files' ids and the links, in the order they're written.
@MainActor @Observable
final class ComposerBox {
    private(set) var typed = ""
    private(set) var attached: [String] = []
    private(set) var links: [String] = []
    @ObservationIgnored weak var view: PieceTextView?
    /// Set by the composer: opens a tapped piece, and looks up a pasted link.
    @ObservationIgnored var opened: (Piece) -> Void = { _ in }
    @ObservationIgnored var pasted: (String) -> Void = { _ in }
    /// The pieces put in before the field was made.
    @ObservationIgnored private var waiting: [Piece] = []

    /// Reads the field again after it changed.
    func changed() {
        guard let v = view else { return }
        typed = v.text ?? ""
        var att: [String] = []
        var lin: [String] = []
        for case .piece(let p) in parts() {
            if p.id.hasPrefix(PieceKind.attach) { att.append(String(p.id.dropFirst(PieceKind.attach.count))) }
            if p.id.hasPrefix(PieceKind.link) { lin.append(String(p.id.dropFirst(PieceKind.link.count))) }
        }
        if att != attached { attached = att }
        if lin != links { links = lin }
    }

    /// The field's words and pieces, in order.
    func parts() -> [Either] {
        guard let s = view?.textStorage else { return [] }
        var out: [Either] = []
        let all = NSRange(location: 0, length: s.length)
        s.enumerateAttribute(.attachment, in: all) { value, range, _ in
            if let a = value as? PieceAttachment {
                out.append(.piece(a.piece))
            } else {
                let words = (s.string as NSString).substring(with: range).replacingOccurrences(of: "\u{FFFC}", with: "")
                if !words.isEmpty { out.append(.words(words)) }
            }
        }
        return out
    }

    /// Puts a piece in at the caret, or in place of what's typed from character `from`
    /// to the caret (an @ and what follows it), with a space after it.
    func put(_ piece: Piece, replacingFrom from: Int? = nil) {
        guard let v = view else {
            waiting.append(piece)
            return
        }
        var range = v.selectedRange
        if let from {
            let text = v.text ?? ""
            let start = text.index(text.startIndex, offsetBy: min(from, text.count))
            let at = NSRange(start..<start, in: text).location
            range = NSRange(location: at, length: max(0, v.selectedRange.location - at))
        }
        let made = NSMutableAttributedString(attributedString: v.piece(piece))
        made.append(NSAttributedString(string: " ", attributes: v.typingAttributes))
        v.edit(range, made)
    }

    /// Takes out the piece of the attached file `id`, and the space after it, as an edit
    /// the field's undo puts back.
    func take(_ id: String) {
        guard let v = view, let range = v.range(of: PieceKind.attach + id) else { return }
        let s = v.textStorage.string as NSString
        let end = range.upperBound < s.length && s.substring(with: NSRange(location: range.upperBound, length: 1)) == " " ? range.length + 1 : range.length
        v.edit(NSRange(location: range.location, length: end), NSAttributedString())
    }

    /// Turns a link's preview back into its address, as plain text.
    func unpreview(_ url: String) {
        guard let v = view, let range = v.range(of: PieceKind.link + url) else { return }
        v.edit(range, NSAttributedString(string: url, attributes: v.typingAttributes))
    }

    /// Empties the field once the message has gone.
    func clear() {
        guard let v = view else { return }
        v.attributedText = NSAttributedString(string: "", attributes: v.typingAttributes)
        v.undoManager?.removeAllActions()
        changed()
    }

    /// The pieces waiting for the field, once it's made.
    func adopt(_ v: PieceTextView) {
        view = v
        for p in waiting { put(p) }
        waiting = []
        changed()
    }
}

/// A piece in the field: one character, drawn as the sent message draws what it is.
final class PieceAttachment: NSTextAttachment {
    var piece = Piece(id: "", text: "")
    /// What it was last drawn as, so it's drawn again only when that changes.
    var drawn = ""
}

/// The composer's field: a text view whose pieces are attachments, which pastes a link
/// on its own as its preview.
final class PieceTextView: UITextView {
    weak var box: ComposerBox?
    var art = PieceArt()

    override func paste(_ sender: Any?) {
        if let text = UIPasteboard.general.string, let url = pastedLink(text) {
            pasteLink(url)
            return
        }
        super.paste(sender)
    }

    /// A link pasted on its own: first as its address, then, as an edit of its own, as
    /// its piece, so an undo right after takes it back to the address.
    private func pasteLink(_ url: String) {
        let at = selectedRange.location
        edit(selectedRange, NSAttributedString(string: url, attributes: typingAttributes))
        box?.pasted(url)
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            let range = NSRange(location: at, length: (url as NSString).length)
            guard range.upperBound <= self.textStorage.length,
                  self.textStorage.attributedSubstring(from: range).string == url else { return }
            self.edit(range, self.piece(linkPiece(url)))
        }
    }

    /// A piece as its attachment, drawn in the field's font.
    func piece(_ p: Piece) -> NSAttributedString {
        let a = PieceAttachment()
        a.piece = p
        art.draw(a, font: font ?? UIFont.systemFont(ofSize: 17))
        let made = NSMutableAttributedString(attachment: a)
        made.addAttributes(typingAttributes, range: NSRange(location: 0, length: made.length))
        return made
    }

    /// Where the piece with this id is.
    func range(of id: String) -> NSRange? {
        var found: NSRange?
        textStorage.enumerateAttribute(.attachment, in: NSRange(location: 0, length: textStorage.length)) { value, range, stop in
            if let a = value as? PieceAttachment, a.piece.id == id {
                found = range
                stop.pointee = true
            }
        }
        return found
    }

    /// Replaces `range` with `new`, as an edit the undo puts back, the caret after it.
    func edit(_ range: NSRange, _ new: NSAttributedString) {
        let old = textStorage.attributedSubstring(from: range)
        let back = NSRange(location: range.location, length: new.length)
        undoManager?.registerUndo(withTarget: self) { v in v.edit(back, old) }
        textStorage.replaceCharacters(in: range, with: new)
        selectedRange = NSRange(location: range.location + new.length, length: 0)
        delegate?.textViewDidChange?(self)
    }

    /// Draws each piece again, as what it shows changed: a link's page was looked up, a
    /// picture's tiny copy was made, or files came to stand side by side.
    func redraw() {
        let all = NSRange(location: 0, length: textStorage.length)
        guard all.length > 0 else { return }
        art.together = together()
        textStorage.beginEditing()
        textStorage.enumerateAttribute(.attachment, in: all) { value, range, _ in
            guard let a = value as? PieceAttachment else { return }
            if art.draw(a, font: font ?? UIFont.systemFont(ofSize: 17)) {
                textStorage.edited(.editedAttributes, range: range, changeInLength: 0)
            }
        }
        textStorage.endEditing()
    }

    /// The attached files that stand beside another, with only spaces between them.
    private func together() -> Set<String> {
        var spans: [(NSRange, String)] = []
        textStorage.enumerateAttribute(.attachment, in: NSRange(location: 0, length: textStorage.length)) { value, range, _ in
            if let a = value as? PieceAttachment, a.piece.id.hasPrefix(PieceKind.attach) { spans.append((range, a.piece.id)) }
        }
        let s = textStorage.string as NSString
        var out: Set<String> = []
        for (a, b) in zip(spans, spans.dropFirst()) {
            let gap = s.substring(with: NSRange(location: a.0.upperBound, length: b.0.location - a.0.upperBound))
            if gap.trimmingCharacters(in: .whitespaces).isEmpty {
                out.insert(a.1)
                out.insert(b.1)
            }
        }
        return out
    }
}

/// What a piece is drawn from, read from the model as the composer is drawn: the agents'
/// colours and threads, the attached files and their tiny pictures, and the links'
/// pages.
struct PieceArt {
    var agents: [String: (color: String, outline: [CGPoint]?)] = [:]
    var files: [String: (name: String, path: String, picture: Bool)] = [:]
    var previews: [String: (title: String, icon: String?)] = [:]
    var together: Set<String> = []

    init() {}

    @MainActor init(model: Model) {
        for a in model.agents { agents[a.id] = (a.color, a.outline) }
        for a in model.attachments + model.dropped {
            files[PieceKind.attach + a.id] = (a.name, a.path, isPicture(a.name))
        }
        for (url, p) in model.linkPreviews {
            switch p {
            case .ready(let l):
                previews[url] = (l.title.trimmingCharacters(in: .whitespaces), l.icon.flatMap { model.files["\($0).thumb"] })
            default:
                break
            }
        }
    }

    /// Draws `a` as its piece is shown, and gives whether it looks different now.
    @discardableResult
    func draw(_ a: PieceAttachment, font: UIFont) -> Bool {
        let p = a.piece
        let ink = UIColor(P.ink)
        var icon: UIImage?
        var words = ""
        var color = ink
        var underline = true
        var medium = false
        switch LinkTo(p.id) {
        case .agent(let id):
            let name = String(p.text.drop { $0 == "@" })
            if let agent = agents[id] {
                color = UIColor(P.mention(agent.color))
                icon = MentionGlyph.thread(agent.outline).withTintColor(color, renderingMode: .alwaysOriginal)
                underline = false
                medium = true
                words = name
            } else {
                words = "@\(name)"
                underline = false
            }
        case .file:
            icon = UIImage(systemName: isPicture(p.text) ? "photo" : "doc")?.withTintColor(UIColor(P.ink3), renderingMode: .alwaysOriginal)
            words = middleCut(p.text)
        default:
            if p.id.hasPrefix(PieceKind.attach) {
                let f = files[p.id]
                if let f, f.picture, let small = Pictures.kept(f.path, side: AttachedRow.picture) {
                    icon = small
                } else {
                    icon = UIImage(systemName: "doc")?.withTintColor(UIColor(P.ink3), renderingMode: .alwaysOriginal)
                }
                words = together.contains(p.id) ? "" : middleCut(f?.name ?? p.text)
            } else if p.id.hasPrefix(PieceKind.link) {
                let url = String(p.id.dropFirst(PieceKind.link.count))
                let page = previews[url]
                if let path = page?.icon, let i = MarkdownView.siteIcon(path) {
                    icon = i
                } else {
                    icon = UIImage(systemName: "globe")?.withTintColor(UIColor(P.ink3), renderingMode: .alwaysOriginal)
                }
                let title = page?.title ?? ""
                words = title.isEmpty ? siteOf(url) : middleCut(title, 40)
            }
        }
        let key = "\(p.id)|\(words)|\(icon.map { ObjectIdentifier($0).hashValue } ?? 0)|\(color)"
        if a.drawn == key { return false }
        a.drawn = key
        let face = medium ? (UIFont(name: "Geist-Medium", size: font.pointSize) ?? font) : font
        var attrs: [NSAttributedString.Key: Any] = [.font: face, .foregroundColor: color]
        if underline { attrs[.underlineStyle] = NSUnderlineStyle.single.rawValue }
        let text = NSAttributedString(string: words, attributes: attrs)
        let side: CGFloat = 16
        let gap: CGFloat = words.isEmpty || icon == nil ? 0 : 3
        let textSize = text.size()
        let height = ceil(font.lineHeight)
        let width = ceil((icon == nil ? 0 : side) + gap + textSize.width) + 4
        let image = UIGraphicsImageRenderer(size: CGSize(width: width, height: height)).image { _ in
            var x: CGFloat = 2
            if let icon {
                let box = CGRect(x: x, y: (height - side) / 2, width: side, height: side)
                UIBezierPath(roundedRect: box, cornerRadius: 3).addClip()
                icon.draw(in: box)
                UIGraphicsGetCurrentContext()?.resetClip()
                x += side + gap
            }
            text.draw(at: CGPoint(x: x, y: (height - textSize.height) / 2))
        }
        a.image = image
        a.bounds = CGRect(x: 0, y: font.descender, width: width, height: height)
        return true
    }
}

/// The composer's field, growing to six lines, then scrolling.
private struct ComposerField: UIViewRepresentable {
    var box: ComposerBox
    var art: PieceArt

    func makeCoordinator() -> Coordinator { Coordinator(box: box) }

    func makeUIView(context: Context) -> PieceTextView {
        let v = PieceTextView()
        let font = UIFontMetrics(forTextStyle: .body).scaledFont(for: UIFont(name: Tokens.Font.ui, size: Tokens.PhoneSize.composer) ?? .systemFont(ofSize: Tokens.PhoneSize.composer))
        v.font = font
        v.adjustsFontForContentSizeCategory = true
        v.textColor = UIColor(P.ink)
        v.typingAttributes = [.font: font, .foregroundColor: UIColor(P.ink)]
        v.backgroundColor = .clear
        v.isScrollEnabled = false
        v.textContainerInset = UIEdgeInsets(top: 8, left: 0, bottom: 8, right: 0)
        v.textContainer.lineFragmentPadding = 2
        v.delegate = context.coordinator
        v.box = box
        v.art = art
        v.accessibilityLabel = "Message"
        let tap = UITapGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.tapped(_:)))
        tap.delegate = context.coordinator
        v.addGestureRecognizer(tap)
        v.setContentHuggingPriority(.defaultLow, for: .horizontal)
        v.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        box.adopt(v)
        return v
    }

    func updateUIView(_ v: PieceTextView, context: Context) {
        v.art = art
        v.redraw()
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView v: PieceTextView, context: Context) -> CGSize? {
        let width = proposal.width ?? 240
        let fits = v.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude))
        let most = ceil((v.font?.lineHeight ?? 20) * 6) + v.textContainerInset.top + v.textContainerInset.bottom
        let scrolls = fits.height > most
        if v.isScrollEnabled != scrolls { v.isScrollEnabled = scrolls }
        return CGSize(width: width, height: min(fits.height, most))
    }

    @MainActor final class Coordinator: NSObject, UITextViewDelegate, UIGestureRecognizerDelegate {
        let box: ComposerBox
        init(box: ComposerBox) { self.box = box }

        func textViewDidChange(_ textView: UITextView) {
            box.changed()
            (textView as? PieceTextView)?.redraw()
        }

        /// A tap on a piece opens what it is.
        @objc func tapped(_ g: UITapGestureRecognizer) {
            guard let v = g.view as? PieceTextView else { return }
            let point = g.location(in: v)
            guard let pos = v.closestPosition(to: point) else { return }
            let at = v.offset(from: v.beginningOfDocument, to: pos)
            for i in [at, at - 1] where i >= 0 && i < v.textStorage.length {
                guard let a = v.textStorage.attribute(.attachment, at: i, effectiveRange: nil) as? PieceAttachment,
                      let start = v.position(from: v.beginningOfDocument, offset: i),
                      let end = v.position(from: start, offset: 1),
                      let range = v.textRange(from: start, to: end) else { continue }
                if v.firstRect(for: range).insetBy(dx: -4, dy: -4).contains(point) {
                    box.opened(a.piece)
                    return
                }
            }
        }

        func gestureRecognizer(_ g: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
            true
        }
    }
}

// MARK: What's attached, over the words

/// What's attached to the next message, as the sent message shows it: a picture as
/// itself, two or more as their fan, any other file as its pill, and each link pasted as
/// a preview as its card. One row, however many there are.
private struct AttachedRow: View {
    @Environment(Session.self) private var session
    var links: [String]
    var take: (String) -> Void
    var unpreview: (String) -> Void
    /// How big a piece's tiny picture is made, in points.
    static let picture: CGFloat = 32

    var body: some View {
        let model = session.model
        let pictures = model.attachments.filter { isPicture($0.name) && $0.error == nil }
        let others = model.attachments.filter { !(isPicture($0.name) && $0.error == nil) }
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(alignment: .bottom, spacing: Tokens.Space.s) {
                if pictures.count == 1, let a = pictures.first {
                    AttachedPicture(attachment: a)
                        .overlay(alignment: .topTrailing) { cross("Remove \(a.name)") { take(a.id) } }
                } else if pictures.count > 1 {
                    AttachedFan(pictures: pictures)
                        .overlay(alignment: .topTrailing) {
                            cross("Remove \(pictures.count) pictures") { for a in pictures { take(a.id) } }
                        }
                }
                ForEach(others) { a in
                    FilePill(attachment: a) { take(a.id) }
                }
                ForEach(links, id: \.self) { url in
                    LinkCard(url: url, preview: model.linkPreviews[url] ?? .loading)
                        .overlay(alignment: .topTrailing) { cross("As plain text") { unpreview(url) } }
                }
            }
            .padding(.horizontal, Tokens.Space.xs)
            .padding(.top, 2)
        }
    }

    private func cross(_ label: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: "xmark")
                .font(.system(size: 9, weight: .bold))
                .foregroundStyle(.white)
                .frame(width: 20, height: 20)
                .background(.black.opacity(0.6), in: Circle())
                .padding(3)
        }
        .accessibilityLabel(label)
    }
}

/// A picture waiting to go, as itself, faint while it goes up.
private struct AttachedPicture: View {
    var attachment: Attachment
    @State private var image: UIImage?

    var body: some View {
        let side: CGFloat = 96
        let shown = image ?? Pictures.kept(attachment.path, side: side)
        let size = shown.map { Pictures.fit($0.size, in: CGSize(width: 200, height: side)) } ?? CGSize(width: side, height: side)
        Group {
            if let shown {
                Image(uiImage: shown).resizable().scaledToFill()
            } else {
                P.bubble
            }
        }
        .frame(width: size.width, height: size.height)
        .clipShape(RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous))
        .opacity(attachment.file == nil ? 0.6 : 1)
        .accessibilityLabel("Picture, \(attachment.name)")
        .task(id: attachment.path) {
            if Pictures.kept(attachment.path, side: side) == nil { image = await Pictures.load(attachment.path, side: side) }
        }
    }
}

/// Two or more pictures waiting to go as the fan the message will have, at half its size.
private struct AttachedFan: View {
    var pictures: [Attachment]

    var body: some View {
        let (w, h): (CGFloat, CGFloat) = (120, 90)
        let reach = fanReach(pictures.count, aspect: h / w)
        ZStack {
            ForEach(fanCards(pictures.count), id: \.picture) { c in
                FanCard(attachment: pictures[c.picture], size: CGSize(width: w, height: h), count: nil)
                    .scaleEffect(c.scale)
                    .rotationEffect(.degrees(c.tilt))
                    .offset(x: c.x * w)
            }
            FanCard(attachment: pictures[0], size: CGSize(width: w, height: h), count: fanCount(pictures.count))
        }
        .frame(width: w, height: h)
        .padding(.leading, (reach.left * w).rounded(.up) + 4)
        .padding(.trailing, (reach.right * w).rounded(.up) + 4)
        .padding(.vertical, (reach.up * w).rounded(.up) + 4)
        .opacity(pictures.contains { $0.file == nil } ? 0.6 : 1)
        .accessibilityLabel("\(pictures.count) pictures")
    }

    private struct FanCard: View {
        var attachment: Attachment
        var size: CGSize
        var count: String?
        @State private var image: UIImage?

        var body: some View {
            let shape = RoundedRectangle(cornerRadius: Tokens.Radius.bubble, style: .continuous)
            let shown = image ?? Pictures.kept(attachment.path, side: size.width)
            Group {
                if let shown { Image(uiImage: shown).resizable().scaledToFill() } else { P.bubble }
            }
            .frame(width: size.width, height: size.height)
            .clipShape(shape)
            .overlay(shape.strokeBorder(P.line, lineWidth: 1))
            .overlay(alignment: .bottomTrailing) {
                if let count {
                    Text(count)
                        .font(Fonts.ui(Tokens.PhoneSize.caption, .medium))
                        .monospacedDigit()
                        .foregroundStyle(.white)
                        .padding(.horizontal, 5)
                        .frame(minWidth: 18, minHeight: 18)
                        .background(.black.opacity(0.6), in: Capsule())
                        .padding(Tokens.Space.xs)
                }
            }
            .shadow(color: .black.opacity(0.14), radius: 4, x: 0, y: 2)
            .task(id: attachment.path) {
                if Pictures.kept(attachment.path, side: size.width) == nil { image = await Pictures.load(attachment.path, side: size.width) }
            }
        }
    }
}

/// Any other file waiting to go, as its pill: its icon, its name cut in the middle, its
/// size or how it's going up, and its ×.
private struct FilePill: View {
    var attachment: Attachment
    var remove: () -> Void

    var body: some View {
        HStack(spacing: Tokens.Space.s) {
            Image(systemName: "doc.text").foregroundStyle(P.ink3)
            VStack(alignment: .leading, spacing: 0) {
                Text(middleCut(attachment.name)).font(Fonts.caption).foregroundStyle(P.ink).lineLimit(1)
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
        .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous).strokeBorder(P.line, lineWidth: 1))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(attachment.name)
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
                Text(middleCut(f.name)).font(Fonts.small).foregroundStyle(P.ink).layoutPriority(1)
                Text(f.place).font(Fonts.small).foregroundStyle(P.ink3)
            }
        }
        .lineLimit(1)
        .truncationMode(.tail)
        .accessibilityElement(children: .combine)
    }
}
