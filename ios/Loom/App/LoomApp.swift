import SwiftUI
import UserNotifications

@main
struct LoomApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var session = Session()
    @Environment(\.scenePhase) private var phase
    @State private var wasAway = false

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(session)
                .onAppear { delegate.session = session }
        }
        .onChange(of: phase) { _, phase in
            switch phase {
            case .active:
                session.active(true, returning: wasAway)
                wasAway = false
            case .background:
                session.active(false, returning: false)
                wasAway = true
            default:
                break
            }
        }
    }
}

/// What the system hands the app outside SwiftUI: the APNs token, and the answer to a
/// notification's Allow or Deny.
final class AppDelegate: NSObject, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    @MainActor var session: Session?

    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        Notifications.setUp()
        // Navigation titles in Geist, as the rest.
        let bar = UINavigationBar.appearance()
        if let title = UIFont(name: "Geist-SemiBold", size: 17), let large = UIFont(name: "Geist-SemiBold", size: Tokens.PhoneSize.hero) {
            bar.titleTextAttributes = [.font: UIFontMetrics(forTextStyle: .headline).scaledFont(for: title)]
            bar.largeTitleTextAttributes = [.font: UIFontMetrics(forTextStyle: .largeTitle).scaledFont(for: large)]
        }
        return true
    }

    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        MainActor.assumeIsolated { session?.registered(pushToken: deviceToken) }
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        // Without the push entitlement (a build not signed with Elliot's team), there's no
        // token. Notifications still show while the app runs.
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .sound, .list]
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        // One the core showed has `call` itself; a push has it under `loom`.
        let info = response.notification.request.content.userInfo
        let call = info["call"] as? String ?? (info["loom"] as? [String: Any])?["call"] as? String
        let action = response.actionIdentifier
        await MainActor.run {
            guard let session, let call else { return }
            if action == Notifications.allowAction {
                session.send(session.model.approve(call, allow: true))
            } else if action == Notifications.denyAction {
                session.send(session.model.approve(call, allow: false))
            }
        }
    }
}
