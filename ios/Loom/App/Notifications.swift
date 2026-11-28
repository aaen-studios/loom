import UIKit
import UserNotifications

/// Notifications: the ones the core asks for while the app runs (`core.notify`), and the
/// place push lands once the instance sends it (LOO-47). An approval comes with Allow and
/// Deny, answered without opening the app.
enum Notifications {
    static let approvalCategory = "approval"
    static let allowAction = "allow"
    static let denyAction = "deny"

    /// What the person chose, without asking.
    static func status() async -> UNAuthorizationStatus {
        await UNUserNotificationCenter.current().notificationSettings().authorizationStatus
    }

    /// Asks for permission, and on a yes, registers with APNs for a device token, which
    /// the app delegate hands the session.
    @MainActor
    static func ask() async -> Bool {
        let center = UNUserNotificationCenter.current()
        let yes = (try? await center.requestAuthorization(options: [.alert, .sound, .badge])) ?? false
        if yes {
            UIApplication.shared.registerForRemoteNotifications()
        }
        return yes
    }

    /// The categories, with Allow and Deny on an approval.
    static func setUp() {
        let allow = UNNotificationAction(identifier: allowAction, title: "Allow", options: [.authenticationRequired])
        let deny = UNNotificationAction(identifier: denyAction, title: "Deny", options: [.destructive])
        let approval = UNNotificationCategory(identifier: approvalCategory, actions: [allow, deny], intentIdentifiers: [])
        UNUserNotificationCenter.current().setNotificationCategories([approval])
    }

    /// Shows what the core said is worth a notification.
    static func show(_ f: JSON) {
        let content = UNMutableNotificationContent()
        content.title = f["title"].text
        content.body = f["body"].text
        content.sound = Sounds.notification(f["sound"].text)
        content.threadIdentifier = f["agent"].text
        var info: [String: String] = ["agent": f["agent"].text, "conversation": f["conversation"].text]
        if f["kind"].text == "approval.needed", let call = f["call"].string {
            content.categoryIdentifier = approvalCategory
            info["call"] = call
        }
        content.userInfo = info
        // The core's tag, so `core.unnotify` takes it back once it's read or answered.
        let tag = f["tag"].text
        let request = UNNotificationRequest(identifier: tag.isEmpty ? UUID().uuidString : tag, content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
    }

    /// Takes back what the core says is read or answered (`core.unnotify`): the ones it
    /// showed by their tags, and with a `conversation` read through, every one about it,
    /// those that came by push too.
    static func withdraw(_ f: JSON) {
        let tags = f["tags"].array.map(\.text)
        let conversation = f["conversation"].text
        guard !conversation.isEmpty else {
            UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: tags)
            return
        }
        UNUserNotificationCenter.current().getDeliveredNotifications { delivered in
            let about = delivered.filter { n in
                let info = n.request.content.userInfo
                let c = info["conversation"] as? String ?? (info["loom"] as? [String: Any])?["conversation"] as? String
                return c == conversation
            }.map(\.request.identifier)
            UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: tags + about)
        }
    }
}
