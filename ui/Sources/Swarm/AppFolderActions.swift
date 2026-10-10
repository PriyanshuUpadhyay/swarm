import AppKit

@MainActor
enum AppFolderActions {
    private enum Failure: LocalizedError {
        case terminalNotFound

        var errorDescription: String? { "Terminal.app was not found." }
    }

    static func reveal(_ folder: String) {
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: folder, isDirectory: true)])
    }

    static func openInTerminal(_ folder: String) async throws {
        guard let terminal = NSWorkspace.shared.urlForApplication(withBundleIdentifier: "com.apple.Terminal") else {
            throw Failure.terminalNotFound
        }
        _ = try await NSWorkspace.shared.open(
            [URL(fileURLWithPath: folder, isDirectory: true)], withApplicationAt: terminal,
            configuration: NSWorkspace.OpenConfiguration()
        )
    }
}
