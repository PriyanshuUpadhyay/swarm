import AppKit
import SwarmCore

@MainActor
final class AppLock: NSObject {
    static let shared = AppLock()
    private var lock: AppRunLock?

    static func hold() async throws {
        guard shared.lock == nil else { return }
        await LoginShellPath.ready()
        try await SwarmCLIBus().initializeHome()
        guard let folder = SwarmHome.dataFolder else { throw OwnerChoicesError.emptyHome }
        shared.lock = try await AppRunLock.acquire(folder: folder, attempts: 3, delay: .milliseconds(20))
        NotificationCenter.default.addObserver(shared, selector: #selector(releaseLock),
                                               name: NSApplication.willTerminateNotification, object: nil)
    }

    @objc private func releaseLock() {
        do { try lock?.release() }
        catch { NSLog("Swarm could not remove its app lock: %@", error.localizedDescription) }
        lock = nil
    }
}
