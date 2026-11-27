import SwiftUI
import UserNotifications

/// The account, the instance, the secrets, how the chat looks, notifications, and
/// signing out.
struct SettingsView: View {
    @Environment(Session.self) private var session
    @Environment(\.dismiss) private var dismiss
    @AppStorage("loom.server") private var server = ""
    @AppStorage("loom.username") private var username = ""
    @AppStorage(Prefs.replies) private var replies = Prefs.bubbles
    @AppStorage(Prefs.vibrant) private var vibrant = false
    @AppStorage(Prefs.showContext) private var showContext = false
    @State private var notifications: UNAuthorizationStatus = .notDetermined
    @State private var confirmSignOut = false
    /// Add a device, open at launch with `-addDevice`, for its screenshot.
    @State private var addingDevice = ProcessInfo.processInfo.arguments.contains("-addDevice")
    /// Settings → Emoji, open at launch with `-emojiSettings`, for its screenshot.
    @State private var emojiSettings = ProcessInfo.processInfo.arguments.contains("-emojiSettings")
    /// Settings → Spools, open at launch with `-spools`, for its screenshots.
    @State private var spoolsOpen = ProcessInfo.processInfo.arguments.contains("-spools")
    /// Settings → Routines, open at launch with `-routines`, for its screenshots.
    @State private var routinesOpen = ProcessInfo.processInfo.arguments.contains("-routines")
    /// Settings → Sounds, open at launch with `-sounds`, for its screenshots.
    @State private var soundsOpen = ProcessInfo.processInfo.arguments.contains("-sounds")

    var body: some View {
        let model = session.model
        NavigationStack {
            List {
                Section("Account") {
                    Row(label: "Username", value: username.isEmpty ? "—" : username)
                    Row(label: "This phone", value: model.thisDevice?.name ?? UIDevice.current.name)
                }
                Section("Instance") {
                    Row(label: "Address", value: server.isEmpty ? "—" : server)
                    Row(label: "Connection", value: connection(model.conn))
                }
                Section {
                    NavigationLink {
                        AddDeviceView()
                    } label: {
                        Text("Add a device").font(Fonts.body).foregroundStyle(P.ink)
                    }
                }
                if !model.devices.isEmpty {
                    Section("Your devices") {
                        ForEach(model.devices) { d in
                            HStack {
                                Image(systemName: icon(d.kind)).foregroundStyle(P.ink3).frame(width: 22)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(d.name).font(Fonts.body).foregroundStyle(P.ink)
                                    if d.id != model.me?.device, let line = seen(d) {
                                        Text(line).font(Fonts.caption).foregroundStyle(P.ink3)
                                    }
                                }
                                Spacer()
                                if d.id == model.me?.device {
                                    Text("This phone").font(Fonts.caption).foregroundStyle(P.ink3)
                                }
                            }
                            .swipeActions {
                                if d.id != model.me?.device {
                                    Button("Remove", role: .destructive) { session.send(model.removeDevice(d.id)) }
                                }
                            }
                        }
                    }
                }
                if model.hasPassword == false {
                    Section {
                        SetPassword()
                    }
                }
                Section {
                    NavigationLink {
                        SpoolsView()
                    } label: {
                        Text("Spools").font(Fonts.body).foregroundStyle(P.ink)
                    }
                    NavigationLink {
                        SecretsView()
                    } label: {
                        Text("Secrets").font(Fonts.body).foregroundStyle(P.ink)
                    }
                    NavigationLink {
                        RoutinesView()
                    } label: {
                        Text("Routines").font(Fonts.body).foregroundStyle(P.ink)
                    }
                }
                Section("Chat") {
                    Picker(selection: $replies) {
                        Text("Bubbles").tag(Prefs.bubbles)
                        Text("Plain").tag(Prefs.plain)
                    } label: {
                        Text("Agent replies").font(Fonts.body).foregroundStyle(P.ink)
                    }
                    .tint(P.ink3)
                    Toggle(isOn: $vibrant) {
                        Text("Vibrant bubbles").font(Fonts.body).foregroundStyle(P.ink)
                    }
                    Toggle(isOn: $showContext) {
                        Text("Show context").font(Fonts.body).foregroundStyle(P.ink)
                    }
                    NavigationLink {
                        EmojiSettingsView()
                    } label: {
                        Text("Emoji").font(Fonts.body).foregroundStyle(P.ink)
                    }
                }
                Section {
                    HStack {
                        Text("Notifications").font(Fonts.body).foregroundStyle(P.ink)
                        Spacer()
                        switch notifications {
                        case .authorized, .provisional, .ephemeral:
                            Text("On").font(Fonts.body).foregroundStyle(P.ink3)
                        case .denied:
                            Button("Open Settings") {
                                if let url = URL(string: UIApplication.openSettingsURLString) {
                                    UIApplication.shared.open(url)
                                }
                            }
                            .font(Fonts.body)
                        default:
                            Button("Turn on") {
                                Task {
                                    _ = await Notifications.ask()
                                    notifications = await Notifications.status()
                                }
                            }
                            .font(Fonts.body)
                        }
                    }
                    NavigationLink {
                        SoundsView()
                    } label: {
                        Text("Sounds").font(Fonts.body).foregroundStyle(P.ink)
                    }
                } footer: {
                    Text("The lock screen needs push on your instance")
                        .font(Fonts.caption)
                }
                Section("Credits") {
                    EmojiCredits()
                }
                Section {
                    Button(role: .destructive) {
                        confirmSignOut = true
                    } label: {
                        Text("Sign out").font(Fonts.body).foregroundStyle(P.bad)
                    }
                } footer: {
                    Text("loom \(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "")\(session.sample ? ", on sample data" : "")")
                        .font(Fonts.caption)
                }
            }
            .scrollContentBackground(.hidden)
            .background(P.side)
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }.font(Fonts.ui(Tokens.PhoneSize.body, .semibold))
                }
            }
            .confirmationDialog("Sign out of this instance?", isPresented: $confirmSignOut, titleVisibility: .visible) {
                Button("Sign out", role: .destructive) {
                    session.signOut()
                    dismiss()
                }
            } message: {
                Text("Your agents and chats stay on the instance")
            }
            .navigationDestination(isPresented: $addingDevice) { AddDeviceView() }
            .navigationDestination(isPresented: $emojiSettings) { EmojiSettingsView() }
            .navigationDestination(isPresented: $spoolsOpen) { SpoolsView() }
            .navigationDestination(isPresented: $routinesOpen) { RoutinesView() }
            .navigationDestination(isPresented: $soundsOpen) { SoundsView() }
        }
        .task { notifications = await Notifications.status() }
        .onAppear {
            model.loadDevices()
            session.flush()
        }
    }

    private func connection(_ c: Conn) -> String {
        switch c {
        case .live: return "Connected"
        case .starting, .connecting: return "Connecting"
        case .offline: return "Offline"
        case .signedOut: return "Signed out"
        case .updateNeeded: return "Needs an update"
        }
    }

    private func icon(_ kind: String) -> String {
        switch kind {
        case "ios": return "iphone"
        case "macos": return "laptopcomputer"
        case "cli": return "terminal"
        default: return "desktopcomputer"
        }
    }

    /// Whether a device is connected, or when it last was.
    private func seen(_ d: Device) -> String? {
        if d.online { return "Connected" }
        guard d.seen > 0 else { return nil }
        let when = When.time(d.seen)
        return "Last seen " + (when.hasPrefix("Yesterday") ? "yesterday" + when.dropFirst("Yesterday".count) : when)
    }
}

/// Add a device: the QR code another phone scans, and the code a computer pastes, for a
/// device that joins this account once the person says yes here.
struct AddDeviceView: View {
    @Environment(Session.self) private var session

    var body: some View {
        let model = session.model
        List {
            Section {
                VStack(spacing: Tokens.Space.l) {
                    if let name = model.joined {
                        Text("\(name) joined").font(Fonts.body).foregroundStyle(P.ink)
                    } else if let code = model.addCode {
                        if let picture = QRPicture.of(code.qr) {
                            Image(uiImage: picture)
                                .interpolation(.none)
                                .resizable()
                                .frame(width: 220, height: 220)
                                .padding(Tokens.Space.s)
                                .background(Color.white, in: RoundedRectangle(cornerRadius: Tokens.Radius.input))
                        }
                        Text("Scan it with your phone, or paste the code on a computer")
                            .font(Fonts.body)
                            .foregroundStyle(P.ink)
                            .multilineTextAlignment(.center)
                        Text(code.text)
                            .font(Fonts.code)
                            .foregroundStyle(P.ink)
                            .textSelection(.enabled)
                            .multilineTextAlignment(.center)
                        HStack(spacing: Tokens.Space.m) {
                            Button("Copy code") { UIPasteboard.general.string = code.text }
                                .buttonStyle(.loom)
                            ShareLink(item: code.qr) { Text("Share link") }
                                .buttonStyle(.loomQuiet)
                        }
                        Text("Works once, until \(When.time(code.expires))")
                            .font(Fonts.caption)
                            .foregroundStyle(P.ink3)
                    } else {
                        ProgressView().tint(P.ink3)
                    }
                    Button("New code") { session.send(model.makeAddCode()) }
                        .font(Fonts.small)
                        .foregroundStyle(P.ink3)
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, Tokens.Space.l)
            }
        }
        .scrollContentBackground(.hidden)
        .background(P.side)
        .navigationTitle("Add a device")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear { session.send(model.makeAddCode()) }
    }
}

/// The first password of an account a link code made, for signing in with it where
/// there's no device to show a code.
private struct SetPassword: View {
    @Environment(Session.self) private var session
    @State private var password = ""
    @State private var open = false

    var body: some View {
        let model = session.model
        if open {
            SecureField("At least 8 characters", text: $password)
                .textContentType(.newPassword)
                .font(Fonts.body)
            if let error = model.passwordError {
                Text(error).font(Fonts.small).foregroundStyle(P.bad)
            }
            Button("Save") {
                session.send(model.setPassword(password))
            }
            .font(Fonts.body)
            .disabled(password.isEmpty)
        } else {
            Button {
                open = true
            } label: {
                Text("Set a password").font(Fonts.body).foregroundStyle(P.ink)
            }
        }
    }
}

/// The person's secrets, sealed on the instance, of which only the names come here, each
/// with the sites their agents' browsers may fill it in on: a swipe takes a site off, or
/// removes the secret. And a form for one more.
struct SecretsView: View {
    @Environment(Session.self) private var session
    @State private var name = ""
    @State private var value = ""

    var body: some View {
        let model = session.model
        List {
            ForEach(model.secrets) { s in
                Section {
                    HStack {
                        Image(systemName: "key").foregroundStyle(P.ink3).frame(width: 22)
                        Text(s.name).font(Fonts.body).foregroundStyle(P.ink)
                    }
                    .swipeActions {
                        Button("Remove", role: .destructive) { session.send(model.deleteSecret(s.name)) }
                    }
                    ForEach(s.sites, id: \.self) { site in
                        HStack {
                            Image(systemName: "globe").foregroundStyle(P.ink3).frame(width: 22)
                            Text(site.isEmpty ? "Files on this computer" : site).font(Fonts.small).foregroundStyle(P.ink2).lineLimit(1)
                        }
                        .swipeActions {
                            Button("Take off", role: .destructive) { session.send(model.removeSecretSite(s.name, site)) }
                        }
                    }
                }
            }
            Section {
                TextField("ntnu-password", text: $name)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                SecureField("Its value", text: $value)
                Button("Add") {
                    session.send(model.putSecret(name, value))
                    name = ""
                    value = ""
                }
                .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty || value.isEmpty)
            }
            .font(Fonts.body)
        }
        .scrollContentBackground(.hidden)
        .background(P.side)
        .navigationTitle("Secrets")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            model.loadSecrets()
            session.flush()
        }
    }
}

private struct Row: View {
    var label: String
    var value: String

    var body: some View {
        HStack {
            Text(label).font(Fonts.body).foregroundStyle(P.ink)
            Spacer()
            Text(value).font(Fonts.body).foregroundStyle(P.ink3).lineLimit(1).truncationMode(.middle)
        }
    }
}

#Preview("Settings") {
    SettingsView().environment(Session(sample: true))
}

#Preview("Settings, dark") {
    SettingsView().environment(Session(sample: true)).preferredColorScheme(.dark)
}
