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
        let guardsOverride: String?
        let guards: String
    }

    @Test("Swift and Rust read the same shared app-lock and guards paths")
    func paths() throws {
        let repository = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        let fixture = repository.appendingPathComponent("tests/fixtures/settings-paths.json")
        for vector in try JSONDecoder().decode([PathVector].self, from: Data(contentsOf: fixture)) {
            let home = SwarmHome.resolve(swarmHome: vector.explicit, home: { vector.home }, branch: vector.branch)
            let folder = try #require(SwarmHome.dataFolder(home: home))
            #expect(AppRunLock.file(in: folder).path == vector.lock)
            var environment = ["HOME": vector.home]
            environment["SWARM_HOME"] = vector.explicit
            environment["SWARM_GUARDS"] = vector.guardsOverride
            #expect(try GuardRules.fileURL(environment: environment).path == vector.guards)
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

    @Test("A short CLI shared-lock probe does not prevent the app from acquiring its lock")
    @MainActor
    func transientProbe() async throws {
        let scratch = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: scratch) }
        let folder = try claimedChoicesFolder(scratch)
        let probe = open(AppRunLock.file(in: folder).path, O_RDWR | O_CREAT | O_CLOEXEC, S_IRUSR | S_IWUSR)
        #expect(probe >= 0)
        defer { close(probe) }
        #expect(flock(probe, LOCK_SH | LOCK_NB) == 0)
        let retryDelay: Duration = .milliseconds(20)
        let releaseProbe = Task.detached {
            try await Task.sleep(for: retryDelay / 2)
            return flock(probe, LOCK_UN)
        }
        let acquired: Result<AppRunLock, Error>
        do { acquired = .success(try await AppRunLock.acquire(folder: folder, attempts: 10, delay: retryDelay)) }
        catch { acquired = .failure(error) }
        #expect(try await releaseProbe.value == 0)
        let lock = try acquired.get()
        #expect(try String(contentsOf: lock.file, encoding: .utf8) == "\(ProcessInfo.processInfo.processIdentifier)\n")
        try lock.release()
    }

    @Test("Async acquisition stops on held locks, invalid attempts, and cancellation")
    @MainActor
    func acquisitionFailures() async throws {
        let scratch = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: scratch) }
        let folder = try claimedChoicesFolder(scratch)
        await #expect(throws: POSIXError(.EINVAL)) {
            try await AppRunLock.acquire(folder: folder, attempts: 0)
        }
        let cancelled = Task { try await AppRunLock.acquire(folder: folder) }
        cancelled.cancel()
        await #expect(throws: CancellationError.self) { try await cancelled.value }
        #expect(!FileManager.default.fileExists(atPath: AppRunLock.file(in: folder).path))
        let held = try AppRunLock(folder: folder)
        defer { try? held.release() }
        await #expect(throws: POSIXError(.EWOULDBLOCK)) {
            try await AppRunLock.acquire(folder: folder, attempts: 3, delay: .zero)
        }
        #expect(try String(contentsOf: held.file, encoding: .utf8) == "\(ProcessInfo.processInfo.processIdentifier)\n")
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
