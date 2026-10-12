import SwarmCore
import UserNotifications

@MainActor
final class NoticeCenter: NSObject, UNUserNotificationCenterDelegate {
    nonisolated private static let category = "swarm.chat"
    private let center = UNUserNotificationCenter.current()
    private let delivery: NoticeDelivery

    init(onDenied: @escaping @MainActor @Sendable () -> Void) {
        delivery = NoticeDelivery(authorize: {
            let center = UNUserNotificationCenter.current()
            let settings = await center.notificationSettings()
            switch settings.authorizationStatus {
            case .notDetermined: return try await center.requestAuthorization(options: [.alert, .sound, .badge])
            case .authorized, .provisional: return true
            default: return false
            }
        }, deliver: { notice in
            let center = UNUserNotificationCenter.current()
            let content = UNMutableNotificationContent()
            content.title = notice.title
            content.body = notice.body
            if let session = notice.sessionID {
                content.userInfo["sessionID"] = session.rawValue
            }
            content.categoryIdentifier = NoticeCenter.category
            if let agent = notice.agentID { content.userInfo["agentID"] = agent.rawValue }
            content.sound = notice.sound ? .default : nil
            try await center.add(UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil))
        }, onDenied: { await onDenied() })
        super.init()
        center.delegate = self
        center.setNotificationCategories([UNNotificationCategory(identifier: Self.category, actions: [],
                                                                 intentIdentifiers: [], options: [])])
    }

    func post(_ notice: Notice) async throws {
        try await delivery.post(notice)
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter,
                                            didReceive response: UNNotificationResponse) async {
        guard response.actionIdentifier == UNNotificationDefaultActionIdentifier,
              response.notification.request.content.categoryIdentifier == Self.category else { return }
        let info = response.notification.request.content.userInfo
        let session = (info["sessionID"] as? String).map(SwarmSessionID.init)
        let agent = (info["agentID"] as? String).map(SwarmAgentID.init)
        await NoticeClick.receive(session: session, agent: agent, focus: { agent, session in
            try await SwarmCLIBus().focus(agent, in: session)
        }, postError: { notice in
            let center = UNUserNotificationCenter.current()
            let content = UNMutableNotificationContent()
            content.title = notice.title
            content.body = notice.body
            // No category or ids: clicking an error cannot start another failed focus.
            try? await center.add(UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil))
        })
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter,
                                            willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .list, .sound]
    }
}
