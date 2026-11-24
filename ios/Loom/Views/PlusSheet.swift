import Photos
import SwiftUI

/// The composer's + menu on an iPhone (PLAN.md, "The composer's + menu"): a sheet from the
/// bottom with a search, the newest photos in a row, and the same groups as the desktop's
/// menu. Skills, Routines and Recent files open their lists in the sheet, with a way back.
struct PlusSheet: View {
    @Environment(Session.self) private var session
    @Environment(\.dismiss) private var dismiss
    /// What the person picked, for the composer to do.
    var take: (PlusTake) -> Void
    @State private var query = ""
    @State private var open: PlusSub?
    /// The routine that asks "Run it now?" in its row, by id.
    @State private var confirm: String?
    @FocusState private var searching: Bool
    var searchFirst = false

    var body: some View {
        let model = session.model
        VStack(spacing: 0) {
            if let open {
                header(open)
            } else {
                search
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    if let open {
                        sub(open)
                    } else {
                        if query.isEmpty {
                            RecentPhotos { take(.photo($0)) } more: { take(.photos) }
                                .padding(.bottom, Tokens.Space.s)
                        }
                        let groups = model.plusGroups(query)
                        if groups.isEmpty {
                            none("Nothing matches “\(query.trimmingCharacters(in: .whitespaces))”.")
                        }
                        ForEach(Array(groups.enumerated()), id: \.offset) { n, group in
                            if n > 0 { line }
                            ForEach(Array(group.enumerated()), id: \.offset) { _, row in
                                self.row(row)
                            }
                        }
                    }
                }
                .padding(.horizontal, Tokens.Space.s)
                .padding(.bottom, Tokens.Space.m)
            }
        }
        .padding(.top, Tokens.Space.m)
        .background(P.raised)
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
        .presentationCornerRadius(Tokens.Radius.card * 2)
        .onAppear {
            for f in model.loadPlus() { session.send(f) }
            if searchFirst { searching = true }
        }
    }

    private var search: some View {
        HStack(spacing: Tokens.Space.s) {
            Image(systemName: "magnifyingglass").foregroundStyle(P.ink3)
            TextField("Find a file, skill or routine…", text: $query)
                .font(Fonts.ui(Tokens.PhoneSize.small))
                .focused($searching)
                .submitLabel(.go)
                .onSubmit {
                    if let first = session.model.plusGroups(query).first?.first { pick(first) }
                }
        }
        .padding(.horizontal, Tokens.Space.m)
        .frame(height: 40)
        .background(P.bg, in: RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous))
        .padding(.horizontal, Tokens.Space.m)
        .padding(.bottom, Tokens.Space.s)
    }

    private func header(_ which: PlusSub) -> some View {
        HStack {
            Button {
                open = nil
                confirm = nil
            } label: {
                Image(systemName: "chevron.left").font(.system(size: 15, weight: .semibold)).foregroundStyle(P.ink2)
                    .frame(width: 40, height: 40)
            }
            .accessibilityLabel("Back")
            Text(title(.open(which))).font(Fonts.ui(Tokens.PhoneSize.body, .semibold))
            Spacer()
        }
        .padding(.horizontal, Tokens.Space.s)
        .padding(.bottom, Tokens.Space.xs)
    }

    @ViewBuilder
    private func sub(_ which: PlusSub) -> some View {
        let rows = session.model.plusSub(which)
        if rows.isEmpty {
            switch which {
            case .recent: none("No files sent yet.")
            case .skills: none("No skills yet. Make one in loom on your computer.")
            case .routines: none("No routines yet. Make one in loom on your computer.")
            }
        }
        ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
            self.row(row)
        }
    }

    private var line: some View {
        Rectangle().fill(P.line).frame(height: 1).padding(.horizontal, Tokens.Space.s).padding(.vertical, Tokens.Space.xs)
    }

    private func none(_ text: String) -> some View {
        Text(text).font(Fonts.small).foregroundStyle(P.ink3)
            .padding(.horizontal, Tokens.Space.m).padding(.vertical, Tokens.Space.s)
    }

    private func title(_ row: PlusRow) -> String {
        switch row {
        case .attach: "Files"
        case .open(.recent): "Recent files"
        case .open(.skills): "Skills"
        case .open(.routines): "Routines"
        case .file(let f): f.file.name
        case .skill(let k): k.name
        case .routine(let r): r.name
        }
    }

    private func icon(_ row: PlusRow) -> String {
        switch row {
        case .attach: "folder"
        case .open(.recent): "clock.arrow.circlepath"
        case .open(.skills), .skill: "book"
        case .open(.routines), .routine: "clock"
        case .file(let f): f.file.isImage ? "photo" : "doc.text"
        }
    }

    /// The grey line under a row, and its colour.
    private func detail(_ row: PlusRow) -> (String, Color)? {
        let model = session.model
        switch row {
        case .file(let f): return (When.time(f.at), P.ink3)
        case .skill(let k): return k.description.isEmpty ? nil : (k.description, P.ink3)
        case .routine(let r):
            let (went, bad) = r.went
            let other = r.agent != model.selected ? model.agent(r.agent)?.name : nil
            return (other.map { "\($0) · \(went)" } ?? went, bad ? P.bad : P.ink3)
        default: return nil
        }
    }

    @ViewBuilder
    private func row(_ row: PlusRow) -> some View {
        if case .routine(let r) = row, confirm == r.id {
            HStack(spacing: Tokens.Space.s) {
                Text("Run \(r.name) now?").font(Fonts.small).foregroundStyle(P.ink).lineLimit(1)
                Spacer()
                Button("Cancel") { confirm = nil }.buttonStyle(.loomQuiet)
                Button("Run") {
                    session.send(session.model.runRoutine(r.id))
                    dismiss()
                }
                .buttonStyle(.loom)
            }
            .padding(.horizontal, Tokens.Space.m)
            .frame(minHeight: 48)
            .background(P.hover, in: RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous))
        } else {
            Button {
                pick(row)
            } label: {
                HStack(spacing: Tokens.Space.m) {
                    Image(systemName: icon(row)).font(.system(size: 15)).foregroundStyle(P.ink2).frame(width: 22)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(title(row)).font(Fonts.ui(Tokens.PhoneSize.small)).foregroundStyle(P.ink)
                        if let d = detail(row) {
                            Text(d.0).font(Fonts.caption).foregroundStyle(d.1)
                        }
                    }
                    .lineLimit(1)
                    Spacer(minLength: 0)
                    if case .open = row {
                        Image(systemName: "chevron.right").font(.system(size: 12, weight: .semibold)).foregroundStyle(P.ink3)
                    }
                }
                .padding(.horizontal, Tokens.Space.m)
                .frame(minHeight: 48)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityElement(children: .combine)
        }
    }

    private func pick(_ row: PlusRow) {
        switch row {
        case .attach: take(.files)
        case .open(let which):
            searching = false
            open = which
        case .file(let f): take(.sent(f.file))
        case .skill(let k): take(.skill(k.name))
        case .routine(let r): confirm = r.id
        }
    }
}

/// What the + sheet asks the composer to do.
enum PlusTake {
    case photos
    case photo(PHAsset)
    case files
    case sent(FileRef)
    case skill(String)
}

/// The newest photos on the phone, a row of them to tap, then one that opens the picker.
/// Only once the person lets loom see their photos; before, the picker alone.
private struct RecentPhotos: View {
    var pick: (PHAsset) -> Void
    var more: () -> Void
    @State private var assets: [PHAsset] = []
    @State private var thumbs: [String: UIImage] = [:]
    private static let side: CGFloat = 72

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: Tokens.Space.s) {
                ForEach(assets, id: \.localIdentifier) { a in
                    Button {
                        pick(a)
                    } label: {
                        Group {
                            if let image = thumbs[a.localIdentifier] {
                                Image(uiImage: image).resizable().scaledToFill()
                            } else {
                                P.bubble
                            }
                        }
                        .frame(width: Self.side, height: Self.side)
                        .clipShape(RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous))
                    }
                    .accessibilityLabel("Photo")
                }
                Button(action: more) {
                    VStack(spacing: 4) {
                        Image(systemName: "photo.on.rectangle").font(.system(size: 18))
                        Text("Photos").font(Fonts.caption)
                    }
                    .foregroundStyle(P.ink2)
                    .frame(width: Self.side, height: Self.side)
                    .background(P.bubble, in: RoundedRectangle(cornerRadius: Tokens.Radius.row, style: .continuous))
                }
            }
            .padding(.horizontal, Tokens.Space.m)
        }
        .task { await load() }
    }

    private func load() async {
        var status = PHPhotoLibrary.authorizationStatus(for: .readWrite)
        if status == .notDetermined {
            status = await PHPhotoLibrary.requestAuthorization(for: .readWrite)
        }
        guard status == .authorized || status == .limited else { return }
        let options = PHFetchOptions()
        options.sortDescriptors = [NSSortDescriptor(key: "creationDate", ascending: false)]
        options.fetchLimit = 12
        let found = PHAsset.fetchAssets(with: .image, options: options)
        var list: [PHAsset] = []
        found.enumerateObjects { a, _, _ in list.append(a) }
        assets = list
        let scale = UIScreen.main.scale
        let size = CGSize(width: Self.side * scale, height: Self.side * scale)
        let request = PHImageRequestOptions()
        request.deliveryMode = .opportunistic
        request.isNetworkAccessAllowed = true
        for a in list {
            let id = a.localIdentifier
            PHImageManager.default().requestImage(for: a, targetSize: size, contentMode: .aspectFill, options: request) { image, _ in
                guard let image else { return }
                Task { @MainActor in thumbs[id] = image }
            }
        }
    }
}
