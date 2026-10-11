import AppKit
import OSLog
import UserNotifications

private let log = Logger(subsystem: "io.github.AlbertoBarrago.Iris", category: "notifications")

/// New-mail notifications through the Mac's notification centre. The
/// core words them and decides which mail is news (`MailListener.newMail`);
/// this posts them, with the buttons Preferences picks as the category's
/// actions, and hands a click or a button back to the model.
@MainActor
final class Notifier: NSObject, UNUserNotificationCenterDelegate {
    private weak var model: MailModel?
    private let center = UNUserNotificationCenter.current()
    private static let category = "mail"

    init(model: MailModel) {
        self.model = model
        super.init()
        center.delegate = self
    }

    /// Asks once whether Iris may notify. macOS remembers the answer, and
    /// System Settings changes it later.
    func start() {
        // The badge needs asking for too: on macOS 26 the Dock shows an
        // app's badge only with the notification permission's Badges on.
        center.requestAuthorization(options: [.alert, .sound, .badge]) { granted, error in
            if let error {
                log.error("Iris could not ask to notify: \(error, privacy: .public)")
            } else if !granted {
                log.notice("Iris may not notify; System Settings, Notifications, turns it on")
            }
            UNUserNotificationCenter.current().getNotificationSettings { settings in
                log.notice("Iris notifications: authorized \(settings.authorizationStatus.rawValue), alerts \(settings.alertSetting.rawValue), style \(settings.alertStyle.rawValue), badges \(settings.badgeSetting.rawValue)")
            }
        }
    }

    /// Posts `notices`, unless Iris is the app in front: the window shows
    /// the mail already, as the GTK app decides too.
    func show(_ notices: [MailNotice], buttons: [NoticeButton]) {
        guard !NSApp.isActive else {
            log.notice("Iris has \(notices.count) notices but is in front, so it shows none")
            return
        }
        register(buttons)
        log.notice("Iris posts \(notices.count) notices")
        for notice in notices {
            let content = UNMutableNotificationContent()
            content.title = notice.title
            content.body = notice.body
            content.sound = .default
            if let account = notice.accountId, let thread = notice.threadId {
                content.threadIdentifier = "\(account):\(thread)"
                content.userInfo = ["account": account, "thread": thread, "message": notice.messageId ?? ""]
                if notice.messageId != nil { content.categoryIdentifier = Self.category }
            }
            center.add(UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil)) { error in
                if let error { log.error("Iris could not post a notification: \(error, privacy: .public)") }
            }
        }
    }

    /// The buttons as the category's actions, read again each time since
    /// Preferences may have changed them. Reply brings Iris forward to
    /// write; Delete shows as destructive.
    private func register(_ buttons: [NoticeButton]) {
        let actions = buttons.map { button in
            let options: UNNotificationActionOptions = switch button.key {
            case "reply": [.foreground]
            case "delete": [.destructive]
            default: []
            }
            return UNNotificationAction(identifier: button.key, title: button.label, options: options)
        }
        center.setNotificationCategories([
            UNNotificationCategory(identifier: Self.category, actions: actions, intentIdentifiers: []),
        ])
    }

    /// Takes a conversation's notifications out of Notification Centre
    /// once it is open: the mail has been seen.
    func seen(account: Int64, thread: String) {
        let key = "\(account):\(thread)"
        center.getDeliveredNotifications { delivered in
            let ids = delivered.filter { $0.request.content.threadIdentifier == key }.map(\.request.identifier)
            if !ids.isEmpty { UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: ids) }
        }
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        let info = response.notification.request.content.userInfo
        guard let account = info["account"] as? Int64, let thread = info["thread"] as? String else {
            // A summary names no one message: it brings the window forward.
            await MainActor.run { NSApp.activate() }
            return
        }
        let message = info["message"] as? String ?? ""
        let action = response.actionIdentifier
        await MainActor.run {
            switch action {
            case UNNotificationDefaultActionIdentifier:
                model?.openFromNotice(account: account, thread: thread)
            case UNNotificationDismissActionIdentifier:
                break
            default:
                model?.noticeChose(account: account, thread: thread, message: message, button: action)
            }
        }
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter,
                                            willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .sound]
    }
}
