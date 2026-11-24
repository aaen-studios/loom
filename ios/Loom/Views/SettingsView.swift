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
    @AppStorage(Prefs.vibrant) private var vibrant = true
    @State private var notifications: UNAuthorizationStatus = .notDetermined
    @State private var confirmSignOut = false

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
                if !model.devices.isEmpty {
                    Section("Your devices") {
                        ForEach(model.devices) { d in
                            HStack {
                                Image(systemName: icon(d.kind)).foregroundStyle(P.ink3).frame(width: 22)
                                Text(d.name).font(Fonts.body).foregroundStyle(P.ink)
                                Spacer()
                                if d.id == model.me?.device {
                                    Text("This phone").font(Fonts.caption).foregroundStyle(P.ink3)
                                }
                            }
                        }
                    }
                }
                Section {
                    NavigationLink {
                        SecretsView()
                    } label: {
                        Text("Secrets").font(Fonts.body).foregroundStyle(P.ink)
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
                } header: {
                    Text("Notifications")
                } footer: {
                    Text("While loom is open, it tells you when a reply is done or an agent asks something. Notifications on the lock screen come once your instance sends push.")
                        .font(Fonts.caption)
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
                Text("This phone forgets its sign-in. Your agents and chats stay on the instance.")
            }
        }
        .task { notifications = await Notifications.status() }
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
