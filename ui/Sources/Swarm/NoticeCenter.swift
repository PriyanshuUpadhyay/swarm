import AppKit
import SwarmCore
import SwiftUI
import UserNotifications

@MainActor
final class NoticeCenter: NSObject, UNUserNotificationCenterDelegate {
    nonisolated private static let category = "swarm.chat"
    private let center = UNUserNotificationCenter.current()
    private let selectChat: @MainActor (SwarmSessionID) -> Void
    private let delivery = NoticeDelivery(authorize: {
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
        content.categoryIdentifier = NoticeCenter.category
        content.userInfo = ["sessionID": notice.sessionID.rawValue]
        content.sound = notice.sound ? .default : nil
        try await center.add(UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil))
    })

    init(selectChat: @escaping @MainActor (SwarmSessionID) -> Void) {
        self.selectChat = selectChat
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
              let session = response.notification.request.content.userInfo["sessionID"] as? String else { return }
        await MainActor.run { selectChat(SwarmSessionID(session)) }
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter,
                                            willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .list, .sound]
    }
}

struct NoticeWindowRouting: ViewModifier {
    let model: SessionsTreeModel
    @Environment(\.openWindow) private var openWindow

    func body(content: Content) -> some View {
        content.onAppear { [model, openWindow] in
            // Keep the scene's action while windows are closed, so a notice can reopen one.
            model.noticeAction = { [weak model, openWindow] session in
                guard let model else { return }
                switch NoticeDestination.resolve(sessionID: session, tree: model.tree, openChats: Set(model.chatWindows.keys)) {
                case .chatWindow(let id): openWindow(id: "chat", value: id)
                case .mainChat(let id):
                    model.select(id)
                    if let window = NSApp.windows.first(where: { $0.frameAutosaveName == SwarmWindows.sessionsFrameName }) {
                        window.deminiaturize(nil)
                        window.makeKeyAndOrderFront(nil)
                    } else {
                        openWindow(id: "sessions")
                    }
                case nil: return
                }
                NSApp.activate()
            }
        }
    }
}
