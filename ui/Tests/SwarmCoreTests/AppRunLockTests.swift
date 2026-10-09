import Foundation
import Darwin
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

    @Test("An app holds the file lock until release and leaves only informational pid text")
    func lifecycle() throws {
        let scratch = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: scratch) }
        let folder = try claimedChoicesFolder(scratch)
        let lock = try AppRunLock(folder: folder)
        #expect(try String(contentsOf: lock.file, encoding: .utf8) == "\(ProcessInfo.processInfo.processIdentifier)\n")
        let probe = open(lock.file.path, O_RDONLY)
        #expect(probe >= 0)
        defer { close(probe) }
        #expect(flock(probe, LOCK_SH | LOCK_NB) == -1)
        #expect(errno == EWOULDBLOCK)
        #expect(throws: POSIXError.self) { try AppRunLock(folder: folder) }
        try lock.release()
        #expect(flock(probe, LOCK_SH | LOCK_NB) == 0)
        #expect(FileManager.default.fileExists(atPath: lock.file.path))
        try lock.release()
        #expect(flock(probe, LOCK_UN) == 0)
        let next = try AppRunLock(folder: folder, pid: 42)
        #expect(try String(contentsOf: next.file, encoding: .utf8) == "42\n")
        try next.release()
        #expect(throws: CocoaError.self) { try AppRunLock(folder: folder, pid: 0) }
    }

    @Test("Dropping the app lock closes its descriptor without removing the file")
    func drop() throws {
        let scratch = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: scratch) }
        let folder = try claimedChoicesFolder(scratch)
        var lock: AppRunLock? = try AppRunLock(folder: folder)
        let probe = open(try #require(lock).file.path, O_RDONLY)
        defer { close(probe) }
        #expect(flock(probe, LOCK_SH | LOCK_NB) == -1)
        lock = nil
        #expect(flock(probe, LOCK_SH | LOCK_NB) == 0)
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
