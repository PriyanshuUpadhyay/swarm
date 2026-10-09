import Foundation
import Testing
@testable import SwarmCore

@Suite("App run lock")
struct AppRunLockTests {
    private struct PathVector: Decodable {
        let home: String
        let branch: String
        let explicit: String?
        let lock: String
    }

    @Test("Swift and Rust read the same shared app-lock paths")
    func paths() throws {
        let repository = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let fixture = repository.appendingPathComponent("tests/fixtures/app-lock-paths.json")
        for vector in try JSONDecoder().decode([PathVector].self, from: Data(contentsOf: fixture)) {
            let home = SwarmHome.resolve(swarmHome: vector.explicit, home: { vector.home }, branch: vector.branch)
            let folder = try #require(SwarmHome.dataFolder(home: home))
            #expect(AppRunLock.file(in: folder).path == vector.lock)
        }
    }

    @Test("A held lock stores the live pid, releases once, and does not remove another app's lock")
    func lifecycle() throws {
        let scratch = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: scratch) }
        let folder = try claimedChoicesFolder(scratch)
        let lock = try AppRunLock(folder: folder)
        #expect(try String(contentsOf: lock.file, encoding: .utf8) == "\(ProcessInfo.processInfo.processIdentifier)\n")
        try lock.release()
        #expect(!FileManager.default.fileExists(atPath: lock.file.path))
        try lock.release()
        let held = try AppRunLock(folder: folder, pid: 42)
        try Data("43\n".utf8).write(to: held.file)
        try held.release()
        #expect(try String(contentsOf: held.file, encoding: .utf8) == "43\n")
        #expect(throws: CocoaError.self) { try AppRunLock(folder: folder, pid: 0) }
        #expect(try String(contentsOf: held.file, encoding: .utf8) == "43\n")
    }

    @Test("Initialization uses the CLI before the app writes to its home")
    func initializeHome() async throws {
        let calls = InitCalls()
        let bus = SwarmCLIBus(environment: [:], cwd: "/scratch") { _, arguments, _, _, _, _ in
            await calls.record(arguments)
            return ShellResult(status: 0, stdout: "", stderr: "")
        }
        try await bus.initializeHome()
        #expect(await calls.arguments == [["init"]])
    }
}

private actor InitCalls {
    var arguments: [[String]] = []
    func record(_ value: [String]) { arguments.append(value) }
}
