import SwiftUI
import VisionKit

/// Signing in: the instance's address, a username and a password (the core's `login`),
/// or scanning the code the desktop app shows under Add a phone (`link`).
struct SignInView: View {
    @Environment(Session.self) private var session
    @AppStorage("loom.server") private var server = ""
    @AppStorage("loom.username") private var username = ""
    @State private var password = ""
    @State private var scanning = false
    @FocusState private var field: Field?

    private enum Field { case server, username, password }

    var body: some View {
        let model = session.model
        ScrollView {
            VStack(alignment: .leading, spacing: Tokens.Space.xl) {
                VStack(alignment: .leading, spacing: Tokens.Space.m) {
                    ThreadView(mood: .writing, color: P.thread("periwinkle"), large: true)
                        .frame(width: 120, height: 16)
                    Text("loom").font(Fonts.hero).foregroundStyle(P.ink)
                    Text("Sign in to your instance.").font(Fonts.body).foregroundStyle(P.ink3)
                }
                .padding(.top, 56)

                Button {
                    scanning = true
                } label: {
                    Label("Scan a code", systemImage: "qrcode.viewfinder")
                }
                .buttonStyle(.loomWide)
                .disabled(model.loggingIn)

                Text("On your computer, open loom and choose Add a phone.")
                    .font(Fonts.caption)
                    .foregroundStyle(P.ink3)
                    .padding(.top, -Tokens.Space.m)

                HStack(spacing: Tokens.Space.m) {
                    Rectangle().fill(P.line).frame(height: 1)
                    Text("or").font(Fonts.caption).foregroundStyle(P.ink3)
                    Rectangle().fill(P.line).frame(height: 1)
                }

                VStack(spacing: Tokens.Space.m) {
                    Field_(label: "Instance") {
                        TextField("https://loom.example.com", text: $server)
                            .keyboardType(.URL)
                            .textContentType(.URL)
                            .focused($field, equals: .server)
                            .submitLabel(.next)
                            .onSubmit { field = .username }
                    }
                    Field_(label: "Username") {
                        TextField("Username", text: $username)
                            .textContentType(.username)
                            .focused($field, equals: .username)
                            .submitLabel(.next)
                            .onSubmit { field = .password }
                    }
                    Field_(label: "Password") {
                        SecureField("Password", text: $password)
                            .textContentType(.password)
                            .focused($field, equals: .password)
                            .submitLabel(.go)
                            .onSubmit(signIn)
                    }
                }
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()

                if let error = model.loginError {
                    Text(error).font(Fonts.small).foregroundStyle(P.bad)
                }

                Button(action: signIn) {
                    if model.loggingIn {
                        ProgressView().tint(P.buttonInk)
                    } else {
                        Text("Sign in")
                    }
                }
                .buttonStyle(LoomButtonStyle(primary: false, wide: true))
                .disabled(model.loggingIn || username.isEmpty || password.isEmpty)
            }
            .padding(.horizontal, Tokens.PhoneSpace.gutter + 4)
            .padding(.bottom, Tokens.Space.xxl)
        }
        .scrollDismissesKeyboard(.interactively)
        .background(P.bg)
        .sheet(isPresented: $scanning) {
            ScanSheet { text in
                scanning = false
                session.send(session.model.link(qr: text, device: Session.device))
            }
        }
    }

    private func signIn() {
        guard !session.model.loggingIn, !username.isEmpty, !password.isEmpty else { return }
        field = nil
        session.send(session.model.login(server: server, username: username, password: password, device: Session.device))
        password = ""
    }
}

/// A labelled field in loom's style.
private struct Field_<Content: View>: View {
    var label: String
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(label).font(Fonts.caption).foregroundStyle(P.ink3)
            content
                .font(Fonts.body)
                .foregroundStyle(P.ink)
                .padding(.horizontal, Tokens.Space.m)
                .frame(minHeight: Tokens.Control.buttonWide)
                .background(P.raised, in: RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: Tokens.Radius.input, style: .continuous).strokeBorder(P.line, lineWidth: 1))
        }
    }
}

/// The camera, reading the QR code. VisionKit's scanner where the phone has it; the
/// simulator has no camera, so there the code's text can be pasted.
struct ScanSheet: View {
    var found: (String) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var pasted = ""

    var body: some View {
        NavigationStack {
            Group {
                if DataScannerViewController.isSupported && DataScannerViewController.isAvailable {
                    Scanner(found: found)
                        .ignoresSafeArea()
                        .overlay(alignment: .bottom) {
                            Text("Point the camera at the code on your computer.")
                                .font(Fonts.small)
                                .foregroundStyle(.white)
                                .padding(.horizontal, Tokens.Space.l)
                                .padding(.vertical, Tokens.Space.s)
                                .background(.black.opacity(0.55), in: Capsule())
                                .padding(.bottom, Tokens.Space.xxl)
                        }
                } else {
                    VStack(alignment: .leading, spacing: Tokens.Space.l) {
                        Text("This device has no camera to scan with. Paste the code's text instead: it starts with loom://link.")
                            .font(Fonts.body)
                            .foregroundStyle(P.ink2)
                        TextField("loom://link?…", text: $pasted, axis: .vertical)
                            .font(Fonts.code)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .padding(Tokens.Space.m)
                            .background(P.raised, in: RoundedRectangle(cornerRadius: Tokens.Radius.input))
                        Button("Join") { found(pasted.trimmingCharacters(in: .whitespacesAndNewlines)) }
                            .buttonStyle(.loomWide)
                            .disabled(!pasted.hasPrefix("loom://link"))
                        Spacer()
                    }
                    .padding(Tokens.PhoneSpace.gutter)
                    .background(P.bg)
                }
            }
            .navigationTitle("Scan a code")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
        }
    }
}

/// VisionKit's DataScannerViewController, reading QR codes, handing back the first that's
/// loom's.
private struct Scanner: UIViewControllerRepresentable {
    var found: (String) -> Void

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let scanner = DataScannerViewController(
            recognizedDataTypes: [.barcode(symbologies: [.qr])],
            qualityLevel: .balanced,
            recognizesMultipleItems: false,
            isHighFrameRateTrackingEnabled: false,
            isHighlightingEnabled: true
        )
        scanner.delegate = context.coordinator
        try? scanner.startScanning()
        return scanner
    }

    func updateUIViewController(_ controller: DataScannerViewController, context: Context) {}

    static func dismantleUIViewController(_ controller: DataScannerViewController, coordinator: Coordinator) {
        controller.stopScanning()
    }

    func makeCoordinator() -> Coordinator { Coordinator(found: found) }

    @MainActor
    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let found: (String) -> Void
        var done = false

        init(found: @escaping (String) -> Void) { self.found = found }

        func dataScanner(_ scanner: DataScannerViewController, didAdd items: [RecognizedItem], allItems: [RecognizedItem]) {
            for item in items {
                guard !done, case .barcode(let code) = item, let text = code.payloadStringValue, text.hasPrefix("loom://link") else { continue }
                done = true
                UINotificationFeedbackGenerator().notificationOccurred(.success)
                found(text)
            }
        }
    }
}

#Preview("Sign in") {
    SignInView().environment(Session(sample: true, signedIn: false))
}

#Preview("Sign in, dark") {
    SignInView().environment(Session(sample: true, signedIn: false)).preferredColorScheme(.dark)
}
