import Foundation

/// `project_list`: the projects Bloom has, for a caller that wants to name one.
///
/// The owner's own client has no workspace to be scoped by, so listing is the only way it can find
/// out what it may name.
///
/// It used to be owner only, on the grounds that a workspace agent already knew its project and
/// could act in no other, so the names and paths of every other repository were information it had
/// no use for. That stopped being true when `workspace_start` let a workspace agent name another
/// project to hand work to, and a tool that takes a project name from a caller who cannot find out
/// the names is a tool that gets guessed at. The rest of the project tools stay with the owner:
/// adding a repository and hiding one are the owner's decisions about the sidebar, not an agent's.
///
/// Read only, and it says so in the description because that is what the model weighs before
/// deciding whether the call is worth making.
public struct ProjectListTool: BridgeToolHandling {
    public init() {}

    public let roles: Set<BridgeRole> = [.workspace, .owner]

    public let tool = BridgeTool(
        name: "project_list",
        description: """
            The projects Swarm knows about: the name and the path of each git repository \
            registered in the sidebar, its default branch, how many workspaces it has, how many \
            of those have an agent working or stopped on a question, whether it is still where \
            Swarm recorded it, and whether the owner has hidden it from the sidebar.

            Those are three different numbers and none of them stands in for another. \
            workspaces counts the worktrees the project has that nobody has archived, whether or \
            not anything is happening in them, and it is what the sidebar draws under the \
            project. agents_running counts how many of those have an agent mid turn right now, \
            and awaiting_permission how many have one stopped on a permission question. A \
            project with workspaces and agents_running 0 has worktrees sitting idle, which is not \
            the same as having none, and a project with workspaces 0 has none at all.

            workspace_list, for a caller that has it, names those same workspaces one by one and \
            is counted from the same rows, so this project's workspaces is how many it lists for the project and its \
            agents_running is how many of them it marks agent_running.

            Call it before naming a project in any other tool, because Swarm will only act on \
            repositories it already has and this is the list of them. Every project here can be \
            worked in, hidden or not: hidden is a view preference of the owner's sidebar and \
            says nothing about whether the project is finished with. Takes no arguments, reads \
            nothing but Swarm's own database, changes nothing and costs nothing.
            """,
        inputSchema: BridgeTool.noArguments
    )

    public func call(
        _ request: MCPRequest,
        as identity: BridgeIdentity,
        store: Store
    ) async -> BridgeToolResult {
        do {
            let projects = try await store.repos()
            guard !projects.isEmpty else {
                return .json(.object([
                    "projects": .array([]),
                    "note": .string(
                        "Swarm has no projects yet. Register an existing git repository with "
                            + "project_add."
                    ),
                ]))
            }

            // One reading for every project, and the same reading `workspace_list` answers from.
            // This used to be a query per project counting `state != .archived`, published under
            // the key `workspaces_running`, and the key is the whole bug: a model told four
            // projects had a workspace running, then handed `agent_running: false` on every row
            // by `workspace_list`, reported the two tools as contradicting each other. See
            // `BridgeWorkspaceCensus`.
            let census = try await BridgeWorkspaceCensus.read(from: store)

            var rows: [JSONValue] = []
            for project in projects {
                let counts = census.counts(repoID: project.id)
                rows.append(.object([
                    "id": .string(project.id.rawValue),
                    "name": .string(project.name),
                    "path": .string(project.path),
                    "default_branch": .string(project.defaultBranch),
                    // Three numbers rather than one, because the one was read as whichever of the
                    // three the reader wanted. Archived workspaces are in none of them.
                    "workspaces": .integer(counts.workspaces),
                    "agents_running": .integer(counts.agentsRunning),
                    "awaiting_permission": .integer(counts.awaitingPermission),
                    // Asked of the file system rather than assumed, because a project whose folder
                    // has been moved still has a row, and a caller that starts a workspace in it
                    // gets a failure it could have been warned about here for nothing.
                    "on_disk": .bool(FileManager.default.fileExists(atPath: project.path)),
                    // Reported per project rather than by leaving the hidden ones out, because a
                    // client that could not see them would name one, be refused, and add it again
                    // as a duplicate. It is stated as the state it is (`hidden`) with the tool
                    // that reverses it named in the note below, so an agent asked to tidy or to
                    // restore has something to act on rather than a flag to guess at.
                    "hidden": .bool(project.hidden),
                ]))
            }

            let hidden = ProjectVisibility.hiddenCount(projects)
            guard hidden > 0 else { return .json(.object(["projects": .array(rows)])) }
            // The way back is named only to a caller that can take it. `project_unhide` is the
            // owner's, and a workspace agent told to call a tool it does not have would try.
            let wayBack = identity.role == .owner
                ? "and project_unhide puts one back in the list."
                : "and the owner can put one back in the list."
            return .json(.object([
                "projects": .array(rows),
                "hidden_projects": .integer(hidden),
                "note": .string(
                    "\(hidden == 1 ? "One project is" : "\(hidden) projects are") hidden from "
                        + "Swarm's sidebar. That is a view preference and nothing else: they are "
                        + "still projects, their workspaces still run, " + wayBack
                ),
            ]))
        } catch {
            return .failure("Swarm could not read its projects: \(error.readableMessage)")
        }
    }
}
