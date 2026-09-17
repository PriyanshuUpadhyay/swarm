import Testing
import Foundation
@testable import SwarmCore

/// What a setup, archive or run script is launched with.
///
/// Pinned as a set of names rather than left to be discovered when somebody's script breaks. The
/// names are an interface: repositories commit scripts that bind them, teams share those files,
/// and a rename that looks tidy from inside the app is a shell error inside somebody else's
/// worktree ten minutes after they asked for a workspace.
@Suite("Script environment")
struct ScriptEnvironmentTests {
    private func makeEnvironment(port: Int = 3_100) throws -> [String: String] {
        let repo = Repo(id: RepoID("r1"), name: "There There", path: "/tmp/there", defaultBranch: "main")
        let workspace = Workspace(
            id: WorkspaceID("w1"),
            repoID: repo.id,
            name: "Fix the inbox",
            branch: "freek/fix-the-inbox",
            path: "/tmp/there-fix-the-inbox",
            baseBranch: "main"
        )
        let manager = WorkspaceManager(store: try Store.inMemory())
        return manager.environment(for: workspace, repo: repo, port: port)
    }

    @Test("SWARM_UI_ is the interface, and it is complete")
    func swarmNamesAreTheInterface() throws {
        let env = try makeEnvironment()

        #expect(env["SWARM_UI_WORKSPACE_NAME"] == "freek-fix-the-inbox")
        #expect(env["SWARM_UI_WORKSPACE_ID"] == "w1")
        #expect(env["SWARM_UI_WORKSPACE_PATH"] == "/tmp/there-fix-the-inbox")
        #expect(env["SWARM_UI_PROJECT_NAME"] == "there")
        #expect(env["SWARM_UI_ROOT_PATH"] == "/tmp/there")
        #expect(env["SWARM_UI_DEFAULT_BRANCH"] == "main")
        #expect(env["SWARM_UI_PORT"] == "3100")
        #expect(env["SWARM_UI_IS_LOCAL"] == "1")

        // A slash in the branch would make a script that uses the name as a folder, a database or
        // a Valet site create something nested instead.
        #expect(env["SWARM_UI_WORKSPACE_NAME"]?.contains("/") == false)
    }

    @Test("CONDUCTOR_ is set to exactly the same values, and nothing else is")
    func theDeprecatedAliasMirrorsIt() throws {
        let env = try makeEnvironment()

        // The reason it is still here: a repository whose committed setup script says
        // `$CONDUCTOR_ROOT_PATH` must not break the day the variable is renamed.
        let swarm = env.filter { $0.key.hasPrefix("SWARM_UI_") }
        let conductor = env.filter { $0.key.hasPrefix("CONDUCTOR_") }

        #expect(swarm.count == conductor.count)
        // Nothing else at all: the two prefixes are the whole of what a script is handed.
        #expect(swarm.count + conductor.count == env.count)
        for (key, value) in swarm {
            let alias = key.replacingOccurrences(of: "SWARM_UI_", with: "CONDUCTOR_")
            #expect(conductor[alias] == value, "\(alias) does not mirror \(key)")
        }
    }

    @Test("the two prefixes are named apart, so a reader can tell an alias from an interface")
    func thePrefixesAreDistinguishable() {
        #expect(WorkspaceManager.environmentPrefix == "SWARM_UI")
        #expect(WorkspaceManager.deprecatedEnvironmentPrefix == "CONDUCTOR")
        #expect(WorkspaceManager.environmentPrefixes == ["SWARM_UI", "CONDUCTOR"])
    }

    @Test("a workspace with no port yet still gets the variable, set to zero")
    func aPortlessWorkspaceStillBindsThePort() throws {
        // A workspace whose repository has no script that wants a block never asks for one, and
        // an archive does not allocate one just to tear a workspace down. A script that reads
        // `$SWARM_UI_PORT` should see an empty-ish value rather than an unbound variable, which
        // under `set -u` is a hard failure in the middle of tearing a workspace down.
        let env = try makeEnvironment(port: 0)
        #expect(env["SWARM_UI_PORT"] == "0")
        #expect(env["CONDUCTOR_PORT"] == "0")
    }

    /// The failure this variable exists to prevent, written down as the two values a script would
    /// build a database name out of.
    @Test("two projects with the same branch name are told apart by the project name")
    func theProjectNameSeparatesTwoProjectsOnTheSameBranch() throws {
        let manager = WorkspaceManager(store: try Store.inMemory())
        let names = ["/Users/freek/dev/there-there", "/Users/freek/dev/mailcoach"].map { path -> (String, String) in
            let repo = Repo(id: RepoID("r"), name: "Project", path: path, defaultBranch: "main")
            let workspace = Workspace(
                id: WorkspaceID("w"),
                repoID: repo.id,
                name: "Main",
                branch: "main",
                path: path + "-main",
                baseBranch: "main"
            )
            let env = manager.environment(for: workspace, repo: repo, port: 3_100)
            return (env["SWARM_UI_WORKSPACE_NAME"] ?? "", env["SWARM_UI_PROJECT_NAME"] ?? "")
        }

        // The name a script had before this: identical, so both worktrees name one database.
        #expect(names[0].0 == names[1].0)
        #expect(names[0].1 != names[1].1)
        #expect(names[0].1 == "there_there")
        #expect(names[1].1 == "mailcoach")
    }

    /// A script pastes this into `CREATE DATABASE` and into container names, unquoted.
    @Test("the project name is safe to paste into an identifier")
    func theProjectNameIsAnIdentifier() {
        let cases = [
            ("/Users/freek/dev/there-there", "there_there"),
            ("/Users/freek/dev/My Project", "My_Project"),
            ("/Users/freek/dev/laravel.dev", "laravel_dev"),
            ("/Users/freek/dev/naïve", "na_ve"),
        ]
        for (path, expected) in cases {
            let repo = Repo(id: RepoID("r"), name: "Fallback", path: path, defaultBranch: "main")
            #expect(WorkspaceManager.projectName(for: repo) == expected)
        }
    }

    /// A repository registered at the filesystem root has no folder to be named after, and a
    /// variable that is set to nothing at all is the one shape a script cannot recover from.
    @Test("a repository with no folder name falls back rather than binding an empty string")
    func aRepositoryWithNoFolderNameStillGetsAName() {
        let repo = Repo(id: RepoID("r"), name: "Rescue", path: "/", defaultBranch: "main")
        #expect(WorkspaceManager.projectName(for: repo) == "Rescue")
    }
}
