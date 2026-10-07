import Darwin
import Foundation
import Observation
import Synchronization
import Testing
@testable import SwarmCore

@Suite("Owner choices")
@MainActor
struct OwnerChoicesTests {
    @Test("An empty swarm home neither reads nor writes choices")
    func emptyHome() throws {
        let folder = SwarmHome.dataFolder(home: "")
        #expect(folder == nil)
        let store = OwnerChoicesStore(folder: folder)
        #expect(try store.load() == OwnerChoices())
        #expect(throws: OwnerChoicesError.self) {
            try store.update { $0.pinned.insert("/project/main") }
        }
    }

    @Test("Choices round-trip while selection and folds stay in defaults")
    func roundTrip() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let suite = "OwnerChoicesTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        var navigation = WorkspaceNavigation()
        navigation.pinned = ["/project/main"]
        navigation.archived = ["/project/old"]
        navigation.names = ["/project/main": "Main work"]
        navigation.selectedWorkspace = "/project/main"
        navigation.selectedChats = ["/project/main": "chat"]
        navigation.collapsed = [WorkspaceNavigation.projectCollapseID("/project")]
        let store = WorkspaceNavigationStore(defaults: defaults, choicesFolder: try claimedChoicesFolder(folder))
        store.save(navigation)
        #expect(WorkspaceNavigationStore(defaults: defaults, choicesFolder: try claimedChoicesFolder(folder)).load() == navigation)
        let choices = try OwnerChoicesStore(folder: folder).load()
        #expect(choices.pinned == navigation.pinned)
        #expect(choices.archived == navigation.archived)
        #expect(choices.names == navigation.names)
        let data = try #require(defaults.data(forKey: "workspaces.navigation"))
        let viewState = try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])
        #expect(viewState["pinned"] == nil)
        #expect(viewState["archived"] == nil)
        #expect(viewState["names"] == nil)
        #expect(viewState["selectedWorkspace"] as? String == "/project/main")
        #expect(viewState["collapsed"] as? [String] == [WorkspaceNavigation.projectCollapseID("/project")])
    }

    @Test("Absent choices keys start empty")
    func absentKeys() throws {
        #expect(try JSONDecoder().decode(OwnerChoices.self, from: Data("{}".utf8)) == OwnerChoices())
    }

    @Test("A bad file is kept before empty choices are saved")
    func badFile() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let bad = Data("{broken".utf8)
        try bad.write(to: folder.appendingPathComponent("choices.json"))
        let store = OwnerChoicesStore(folder: try claimedChoicesFolder(folder))
        #expect(try store.load() == OwnerChoices())
        let backup = try #require(FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)
            .first { $0.lastPathComponent.hasPrefix("choices.json.bad-") })
        #expect(try Data(contentsOf: backup) == bad)
        try store.update { $0.pinned.insert("/project/main") }
        #expect(try Data(contentsOf: backup) == bad)
        #expect(try store.load().pinned == ["/project/main"])
    }

    @Test("Pins in two data homes stay apart with shared view defaults")
    func separateHomes() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let suite = "OwnerChoicesTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: root)
        }
        let first = WorkspaceNavigationStore(defaults: defaults, choicesFolder: try claimedChoicesFolder(root.appendingPathComponent("first")))
        let second = WorkspaceNavigationStore(defaults: defaults, choicesFolder: try claimedChoicesFolder(root.appendingPathComponent("second")))
        var navigation = WorkspaceNavigation()
        navigation.pinned = ["/first"]
        first.save(navigation)
        #expect(second.load().pinned.isEmpty)
        navigation.pinned = ["/second"]
        second.save(navigation)
        #expect(first.load().pinned == ["/first"])
        #expect(second.load().pinned == ["/second"])
    }
    @Test("An unclaimed home keeps all files unchanged and reports the failed save")
    func unclaimedHome() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let store = OwnerChoicesStore(folder: folder)
        #expect(throws: (any Error).self) {
            try store.update { $0.pinned.insert("/project/main") }
        }
        #expect(!FileManager.default.fileExists(atPath: folder.path))
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let file = folder.appendingPathComponent("choices.json")
        let bad = Data("{broken".utf8)
        try bad.write(to: file)
        #expect(try store.load() == OwnerChoices())
        #expect(try Data(contentsOf: file) == bad)
        #expect(try FileManager.default.contentsOfDirectory(atPath: folder.path) == ["choices.json"])
    }

    @Test("An update waits for another process and loads its completed choices")
    func processLock() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        try Data("swarm\n".utf8).write(to: folder.appendingPathComponent("swarm-home"))
        let ready = folder.appendingPathComponent("ready")
        let child = Process()
        child.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        child.arguments = ["-c", """
        import fcntl, json, pathlib, sys, time
        folder = pathlib.Path(sys.argv[1])
        with (folder / 'choices.lock').open('w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            (folder / 'ready').write_text('locked')
            time.sleep(0.25)
            (folder / 'choices.json').write_text(json.dumps({'pinned': ['/other/process']}))
        """, folder.path]
        try child.run()
        for _ in 0..<500 where !FileManager.default.fileExists(atPath: ready.path) { usleep(10_000) }
        #expect(FileManager.default.fileExists(atPath: ready.path))
        let store = OwnerChoicesStore(folder: folder)
        try store.update { $0.pinned.insert("/this/process") }
        child.waitUntilExit()
        #expect(child.terminationStatus == 0)
        #expect(try store.load().pinned == ["/other/process", "/this/process"])
    }

    @Test("The launch read waits for a writer and keeps pins, names, projects, and view state")
    func launchReadWaitsForWriter() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        let suite = "OwnerChoicesTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        var viewState = WorkspaceNavigation()
        viewState.selectedWorkspace = "/repo"
        viewState.collapsed = ["pinned"]
        defaults.set(try JSONEncoder().encode(viewState), forKey: "workspaces.navigation")
        let ready = folder.appendingPathComponent("ready")
        let child = Process()
        child.executableURL = URL(fileURLWithPath: "/usr/bin/python3")
        child.arguments = ["-c", """
        import fcntl, json, pathlib, sys, time
        folder = pathlib.Path(sys.argv[1])
        with (folder / 'choices.lock').open('w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            (folder / 'ready').write_text('locked')
            time.sleep(0.25)
            (folder / 'choices.json').write_text(json.dumps({
                'pinned': ['/repo'], 'names': {'/repo': 'Main work'}, 'projectPaths': ['/repo']
            }))
        """, folder.path]
        try child.run()
        defer { child.waitUntilExit() }
        for _ in 0..<500 where !FileManager.default.fileExists(atPath: ready.path) { usleep(10_000) }
        try #require(FileManager.default.fileExists(atPath: ready.path))
        let choices = OwnerChoicesStore(folder: folder)
        let store = WorkspaceNavigationStore(defaults: defaults, choices: choices)
        let loaded = store.load()
        #expect(loaded.pinned == ["/repo"])
        #expect(loaded.names == ["/repo": "Main work"])
        #expect(store.savedChoices.projectPaths == ["/repo"])
        #expect(loaded.selectedWorkspace == "/repo")
        #expect(loaded.collapsed == ["pinned"])
        #expect(choices.alerts.message == nil)
    }

    @Test("A launch lock timeout reports a load failure within its bounded wait")
    func launchLockTimeoutReportsFailure() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        let suite = "OwnerChoicesTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        let descriptor = open(folder.appendingPathComponent("choices.lock").path, O_CREAT | O_RDWR, 0o600)
        try #require(descriptor >= 0)
        defer { _ = flock(descriptor, LOCK_UN); close(descriptor) }
        try #require(flock(descriptor, LOCK_EX | LOCK_NB) == 0)
        let choices = OwnerChoicesStore(folder: folder)
        let started = ContinuousClock.now
        _ = WorkspaceNavigationStore(defaults: defaults, choices: choices).load()
        #expect(started.duration(to: .now) >= .milliseconds(900))
        #expect(started.duration(to: .now) < .seconds(OwnerChoicesStore.writeLockTimeout + 0.75))
        #expect(choices.alerts.message == OwnerChoicesFailure(OwnerChoicesError.lockBusy.localizedDescription, operation: .load).message)
    }

    @Test("A held choices lock fails within one second and remains usable after release")
    func heldLockTimesOut() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        defer { try? FileManager.default.removeItem(at: folder) }
        let descriptor = open(folder.appendingPathComponent("choices.lock").path, O_CREAT | O_RDWR, 0o600)
        #expect(descriptor >= 0)
        defer { close(descriptor) }
        #expect(flock(descriptor, LOCK_EX | LOCK_NB) == 0)
        let released = DispatchSemaphore(value: 0)
        let lockHold = OwnerChoicesStore.writeLockTimeout + 1
        let lockLimit = Duration.seconds(OwnerChoicesStore.writeLockTimeout + 0.75)
        DispatchQueue.global().asyncAfter(deadline: .now() + lockHold) {
            _ = flock(descriptor, LOCK_UN)
            released.signal()
        }
        let store = OwnerChoicesStore(folder: folder)
        let started = ContinuousClock.now
        #expect(throws: OwnerChoicesError.self) { try store.update { $0.pinned.insert("/repo") } }
        #expect(started.duration(to: .now) < lockLimit)
        released.wait()
        try store.update { $0.pinned.insert("/repo") }
        #expect(try store.load().pinned == ["/repo"])
    }

    @Test("A busy refresh read keeps the last good snapshot without waiting or reporting")
    func busyRefreshKeepsSnapshot() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: folder)
        try choices.update { $0.projectPaths = ["/repo"]; $0.pinned = ["/repo"] }
        let projects = SwarmProjectStore(choices: choices)
        let expected = try #require(projects.loadChoices(reportError: { Issue.record("\($0.message)") }))
        let descriptor = open(folder.appendingPathComponent("choices.lock").path, O_RDWR)
        #expect(descriptor >= 0)
        defer { _ = flock(descriptor, LOCK_UN); close(descriptor) }
        #expect(flock(descriptor, LOCK_EX | LOCK_NB) == 0)
        var failures: [OwnerChoicesFailure] = []
        for _ in 0..<3 {
            let started = ContinuousClock.now
            let retained = projects.loadChoices(reportError: { failures.append($0) })
            #expect(started.duration(to: .now) < .milliseconds(250))
            #expect(retained == expected)
            #expect(projects.choicesLoadFailed)
        }
        #expect(failures.isEmpty)
        _ = flock(descriptor, LOCK_UN)
        #expect(projects.loadChoices(reportError: { Issue.record("\($0.message)") }) == expected)
        #expect(!projects.choicesLoadFailed)
    }

    @Test("A failed choices write still saves selection and folds in view defaults")
    func failedChoicesKeepsViewState() throws {
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let suite = "OwnerChoicesTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        var navigation = WorkspaceNavigation()
        navigation.selectedWorkspace = "/project/main"
        navigation.selectedChats = ["/project/main": "chat"]
        navigation.collapsed = ["pinned"]
        navigation.pinned = ["/project/main"]
        let store = WorkspaceNavigationStore(defaults: defaults, choicesFolder: folder)
        #expect(store.save(navigation)?.message.contains("Run swarm init") == true)
        let loaded = store.load()
        #expect(loaded.selectedWorkspace == navigation.selectedWorkspace)
        #expect(loaded.selectedChats == navigation.selectedChats)
        #expect(loaded.collapsed == navigation.collapsed)
        #expect(!FileManager.default.fileExists(atPath: folder.path))
    }

    @Test("Choices errors name the operation once and give a next step")
    func choicesErrorMessages() {
        let unclaimed = OwnerChoicesFailure(OwnerChoicesError.unclaimedHome("/unclaimed").localizedDescription,
                                            operation: .save)
        #expect(unclaimed.message == "Could not save sidebar choices. Run swarm init with SWARM_HOME set to /unclaimed, then try again.")
        let busy = OwnerChoicesFailure(OwnerChoicesError.lockBusy.localizedDescription, operation: .load)
        #expect(busy.message == "Could not load sidebar choices. Another update is in progress. Try again.")
    }

    @Test("A view-state encode failure is separate from a choices save failure")
    func viewStateEncodingFailure() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        let suite = "OwnerChoicesTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        let choices = OwnerChoicesStore(folder: folder)
        var failEncode = true
        let store = WorkspaceNavigationStore(defaults: defaults, choices: choices, encodeViewState: { value in
            if failEncode {
                throw EncodingError.invalidValue(value, .init(codingPath: [], debugDescription: "View state encoding failed."))
            }
            return try JSONEncoder().encode(value)
        })
        var navigation = WorkspaceNavigation()
        navigation.selectedWorkspace = "/repo"
        navigation.pinned = ["/repo"]
        let failure = try #require(store.save(navigation))
        #expect(failure.operation == .saveViewState)
        #expect(failure.message.hasPrefix("Could not save sidebar view state."))
        #expect(!failure.message.contains("sidebar choices"))
        #expect(defaults.data(forKey: "workspaces.navigation") == nil)
        #expect(!FileManager.default.fileExists(atPath: folder.appendingPathComponent("choices.json").path))
        let choicesFailure = OwnerChoicesFailure("Write denied.", operation: .save)
        choices.alerts.report(choicesFailure)
        choices.alerts.report(failure)
        failEncode = false
        navigation.pinned = []
        #expect(store.save(navigation) == nil)
        let viewData = try #require(defaults.data(forKey: "workspaces.navigation"))
        #expect(try JSONDecoder().decode(WorkspaceNavigation.self, from: viewData).selectedWorkspace == "/repo")
        #expect(choices.alerts.message == choicesFailure.message)
        choices.alerts.dismiss()
        #expect(choices.alerts.message == nil)
    }

    @Test("Load and save alerts wait for dismissal and each distinct error appears once")
    func distinctChoicesAlerts() {
        let loadFailure = OwnerChoicesFailure("Read denied.", operation: .load)
        let saveFailure = OwnerChoicesFailure("Write denied.", operation: .save)
        var alerts = OwnerChoicesAlerts()
        let addedLoad = alerts.report(loadFailure)
        let addedSave = alerts.report(saveFailure)
        let repeatedLoad = alerts.report(loadFailure)
        #expect(addedLoad)
        #expect(addedSave)
        #expect(!repeatedLoad)
        #expect(alerts.message == loadFailure.message)
        alerts.dismiss()
        #expect(alerts.message == saveFailure.message)
        alerts.dismiss()
        #expect(alerts.message == nil)
        let repeatedSaveAfterDismissal = alerts.report(saveFailure)
        let repeatedLoadAfterDismissal = alerts.report(loadFailure)
        #expect(!repeatedSaveAfterDismissal)
        #expect(!repeatedLoadAfterDismissal)
        #expect(alerts.message == nil)
        alerts.resolve(.load)
        let loadAfterRecovery = alerts.report(loadFailure)
        let saveDuringLoadRecovery = alerts.report(saveFailure)
        #expect(loadAfterRecovery)
        #expect(!saveDuringLoadRecovery)
        #expect(alerts.message == loadFailure.message)
        alerts.resolve(.save)
        #expect(alerts.message == loadFailure.message)
        alerts.dismiss()
        let saveAfterRecovery = alerts.report(saveFailure)
        let loadDuringSaveRecovery = alerts.report(loadFailure)
        #expect(saveAfterRecovery)
        #expect(!loadDuringSaveRecovery)
        #expect(alerts.message == saveFailure.message)
    }

    @Test("Only an actual choices write resolves a save failure")
    func choicesAlertsAfterSuccessfulWrite() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        defer { try? FileManager.default.removeItem(at: folder) }
        let choices = OwnerChoicesStore(folder: folder)
        let failure = OwnerChoicesFailure("Write denied.", operation: .save)
        choices.alerts.report(failure)
        choices.alerts.dismiss()
        try choices.update { _ in }
        let repeatedBeforeWrite = choices.alerts.report(failure)
        #expect(!repeatedBeforeWrite)
        try choices.update { $0.pinned = ["/repo"] }
        #expect(try choices.load().pinned == ["/repo"])
        let afterRecovery = choices.alerts.report(failure)
        #expect(afterRecovery)
    }

    @Test("A real successful load resolves load failures without resolving save failures")
    func choicesAlertsAfterSuccessfulLoad() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        defer { try? FileManager.default.removeItem(at: folder) }
        try OwnerChoicesStore(folder: folder).update { $0.pinned = ["/repo"] }
        var failRead = true
        let choices = OwnerChoicesStore(folder: folder, readFile: {
            if failRead { throw CocoaError(.fileReadNoPermission) }
            return try Data(contentsOf: $0)
        })
        let loadFailure = OwnerChoicesFailure("Read denied.", operation: .load)
        let saveFailure = OwnerChoicesFailure("Write denied.", operation: .save)
        choices.alerts.report(loadFailure)
        choices.alerts.report(saveFailure)
        choices.alerts.dismiss()
        choices.alerts.dismiss()
        #expect(throws: CocoaError.self) { try choices.load() }
        let repeatedFailedLoad = choices.alerts.report(loadFailure)
        #expect(!repeatedFailedLoad)
        failRead = false
        #expect(try choices.load().pinned == ["/repo"])
        let loadAfterRecovery = choices.alerts.report(loadFailure)
        #expect(loadAfterRecovery)
        let saveDuringLoadRecovery = choices.alerts.report(saveFailure)
        #expect(!saveDuringLoadRecovery)
    }

    @Test("Resolving an operation with no failure leaves the alerts unchanged")
    func resolvingAbsentFailureKeepsAlerts() {
        var alerts = OwnerChoicesAlerts()
        alerts.report(OwnerChoicesFailure("Write denied.", operation: .save))
        let before = alerts
        alerts.resolve(.load)
        #expect(alerts == before)
    }

    @Test("A load and view-state save without matching failures do not write observed alerts")
    func successfulReadsAndSavesKeepObservedAlerts() throws {
        let folder = try claimedChoicesFolder(FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString))
        let suite = "OwnerChoicesTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            defaults.removePersistentDomain(forName: suite)
            try? FileManager.default.removeItem(at: folder)
        }
        let choices = OwnerChoicesStore(folder: folder)
        choices.alerts.report(OwnerChoicesFailure("Write denied.", operation: .save))
        let before = choices.alerts
        let changes = Mutex(0)
        withObservationTracking { _ = choices.alerts.message } onChange: { changes.withLock { $0 += 1 } }
        _ = try choices.load()
        withObservationTracking { _ = choices.alerts.message } onChange: { changes.withLock { $0 += 1 } }
        #expect(WorkspaceNavigationStore(defaults: defaults, choices: choices).save(WorkspaceNavigation()) == nil)
        #expect(choices.alerts == before)
        #expect(changes.withLock { $0 } == 0)

        choices.alerts.report(OwnerChoicesFailure("Read denied.", operation: .load))
        changes.withLock { $0 = 0 }
        withObservationTracking { _ = choices.alerts.message } onChange: { changes.withLock { $0 += 1 } }
        _ = try choices.load()
        #expect(choices.alerts == before)
        #expect(changes.withLock { $0 } == 1)
    }

    @Test("Resolving an operation removes its pending failures and permits one new alert")
    func resolvingDropsPendingFailures() {
        let loadFailure = OwnerChoicesFailure("Read denied.", operation: .load)
        let saveFailure = OwnerChoicesFailure("Write denied.", operation: .save)
        var alerts = OwnerChoicesAlerts()
        alerts.report(loadFailure)
        alerts.report(saveFailure)
        alerts.resolve(.save)
        #expect(alerts.message == loadFailure.message)
        alerts.dismiss()
        #expect(alerts.message == nil)
        let saveAfterRecovery = alerts.report(saveFailure)
        #expect(saveAfterRecovery)
        let repeatedSave = alerts.report(saveFailure)
        #expect(!repeatedSave)
        alerts.dismiss()
        #expect(alerts.message == nil)
    }

}
