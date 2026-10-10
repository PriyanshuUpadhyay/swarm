import Foundation
import Testing
@testable import SwarmCore

/// Tests use claimed temporary homes; the CLI owns the real marker.
func claimedChoicesFolder(_ folder: URL) throws -> URL {
    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    try Data("swarm\n".utf8).write(to: folder.appendingPathComponent("swarm-home"))
    return folder
}

@MainActor
func savedChoices(from store: SwarmProjectStore) throws -> OwnerChoices {
    try #require(store.loadChoices(reportError: { Issue.record("\($0.message)") }))
}

@MainActor
func refresh(_ tree: SessionsTree, in projects: SwarmProjectStore, choicesFolder: URL) throws {
    let suite = "ChoicesHomeFixture.\(UUID().uuidString)"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defer { defaults.removePersistentDomain(forName: suite) }
    let store = WorkspaceNavigationStore(defaults: defaults, choicesFolder: choicesFolder)
    _ = projects.refreshChoices(shown: tree.projects, navigation: store.load(),
                                navigationStore: store, reportError: { Issue.record("\($0.message)") })
}
