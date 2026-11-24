import AuthenticationServices
import UIKit

/// Signing in to one of the person's MCP servers (PLAN.md, "Signing in to MCP servers"),
/// as the desktop's `signin.rs` does with a listener on loopback: the server's sign-in
/// page in `ASWebAuthenticationSession`, which shares the person's Safari sign-ins and
/// hands back what the browser brings to the `loom:` redirect. The instance holds the
/// PKCE verifier, so the code in it is of no use to anything else.
@MainActor
final class SignInPage: NSObject, ASWebAuthenticationPresentationContextProviding {
    /// Where the browser comes back to.
    static let redirect = "loom:/callback"

    /// How a sign-in page ended: what came back to the redirect, as it came, or why
    /// nothing did, in words.
    enum Ended {
        case back(query: String)
        case failed(String)
    }

    /// The server whose page is open, if one is.
    private(set) var server: String?
    private var page: ASWebAuthenticationSession?
    private var ended: ((String, Ended) -> Void)?
    /// Counts the pages opened, so a page closed since can't end the one open now.
    private var opened = 0

    /// Opens `url`, the sign-in page the instance gave for `server`, over the app. One
    /// page is open at a time: opening another ends the one before.
    func open(_ url: URL, server: String, ended: @escaping (String, Ended) -> Void) {
        close(saying: "You started another sign-in.")
        opened += 1
        let n = opened
        let page = ASWebAuthenticationSession(url: url, callback: .customScheme("loom")) { back, error in
            let query = back.map(SignInPage.query)
            let words = error.map(SignInPage.words)
            Task { @MainActor [weak self] in
                self?.finish(n, query.map { .back(query: $0) } ?? .failed(words ?? "The sign-in didn't finish."))
            }
        }
        page.presentationContextProvider = self
        self.server = server
        self.page = page
        self.ended = ended
        if !page.start() {
            finish(n, .failed("The sign-in page couldn't open."))
        }
    }

    /// What came to the `loom:` redirect from outside the page, such as a sign-in
    /// finished in Safari: it ends the page that's open with it. Anything else, or with
    /// no page open, is let be.
    func took(_ url: URL) {
        guard server != nil, url.scheme?.lowercased() == "loom", url.path == "/callback" else { return }
        let open = page
        endWith(.back(query: SignInPage.query(url)))
        open?.cancel()
    }

    private func finish(_ n: Int, _ how: Ended) {
        if n == opened { endWith(how) }
    }

    private func close(saying words: String) {
        let open = page
        endWith(.failed(words))
        open?.cancel()
    }

    private func endWith(_ how: Ended) {
        guard let server, let ended else { return }
        self.server = nil
        self.page = nil
        self.ended = nil
        ended(server, how)
    }

    /// The query of what came back, as it came.
    private nonisolated static func query(_ url: URL) -> String {
        URLComponents(url: url, resolvingAgainstBaseURL: false)?.percentEncodedQuery ?? ""
    }

    /// Why a page ended without coming back, in plain words.
    private nonisolated static func words(_ error: Error) -> String {
        switch (error as? ASWebAuthenticationSessionError)?.code {
        case .canceledLogin: "You closed the sign-in page before it finished."
        case .presentationContextNotProvided, .presentationContextInvalid: "The sign-in page couldn't open."
        default: "The sign-in page couldn't open: \(error.localizedDescription)"
        }
    }

    nonisolated func presentationAnchor(for session: ASWebAuthenticationSession) -> ASPresentationAnchor {
        MainActor.assumeIsolated {
            UIApplication.shared.connectedScenes
                .compactMap { $0 as? UIWindowScene }
                .flatMap(\.windows)
                .first(where: \.isKeyWindow) ?? ASPresentationAnchor()
        }
    }
}
