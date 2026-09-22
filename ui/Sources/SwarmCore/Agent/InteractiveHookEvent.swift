import Foundation

/// The hook events that interactive agent CLIs report back to Swarm.
///
/// Every hook handler updates the status file that Swarm watches to track process health and
/// session state. Without this typed catalog, unlisted events were silently ignored, causing
/// chats to stall at startup screens without updating the interface.
public enum InteractiveHookEvent: String, CaseIterable, Sendable {
    case SessionStart
    case Setup
    case UserPromptSubmit
    case UserPromptExpansion
    case PreToolUse
    case PermissionRequest
    case PermissionDenied
    case PostToolUse
    case PostToolUseFailure
    case PostToolBatch
    case Notification
    case MessageDisplay
    case SubagentStart
    case SubagentStop
    case TaskCreated
    case TaskCompleted
    case Stop
    case StopFailure
    case TeammateIdle
    case InstructionsLoaded
    case ConfigChange
    case CwdChanged
    case DirectoryAdded
    case FileChanged
    case WorktreeCreate
    case WorktreeRemove
    case PreCompact
    case PostCompact
    case PreModelSwitch
    case PostModelSwitch
    case Elicitation
    case ElicitationResult
    case SessionEnd

    /// Maps an incoming hook event payload to the corresponding session state.
    ///
    /// The switch is exhaustive with no fallback branch so that every new hook event requires an
    /// explicit lifecycle state decision at compile time.
    public func state(payload: [String: Any]) -> SessionState? {
        switch self {
        // Active execution: the turn is progressing or handling an intermediate tool or subagent step.
        case .UserPromptSubmit, .PreToolUse, .PostToolUse, .PermissionDenied,
             .PostToolUseFailure, .PostToolBatch, .SubagentStart, .SubagentStop,
             .TaskCreated, .TaskCompleted, .PreCompact, .PostCompact,
             .UserPromptExpansion, .PostModelSwitch, .ElicitationResult:
            return .running

        // Awaiting input: the agent is blocked pending user approval, prompt answering, or model selection.
        case .PermissionRequest, .Elicitation, .PreModelSwitch:
            return .waiting

        // Clean turn completion: the agent has finished its work and awaits further instructions.
        case .Stop, .SessionEnd:
            return .idle

        // Execution failure: the agent halted unexpectedly or encountered an unrecoverable error.
        case .StopFailure:
            return .failed

        // Session startup: a compaction resumption stays running, whereas an initial launch sits idle.
        case .SessionStart:
            return payload["source"] as? String == "compact" ? .running : .idle

        // Prompt notifications: distinguish between a permission ask and an idle prompt.
        case .Notification:
            switch payload["notification_type"] as? String {
            case "permission_prompt":
                return .waiting
            case "idle_prompt":
                return .idle
            default:
                return nil
            }

        // Informational and environment events that do not alter session lifecycle state.
        case .Setup, .MessageDisplay, .TeammateIdle, .InstructionsLoaded,
             .ConfigChange, .CwdChanged, .DirectoryAdded, .FileChanged,
             .WorktreeCreate, .WorktreeRemove:
            return nil
        }
    }

    /// Events skipped for Claude Code, paired with the reason each is omitted from CLI arguments.
    public static let claudeSkippedReasons: [InteractiveHookEvent: String] = [
        .MessageDisplay: "Fires while text is drawn, so a shell per event costs far more than it tells.",
        .FileChanged: "Fires on every watched write, and the chat has no state that a file write changes.",
        .InstructionsLoaded: "Changes no chat state, and fires once per instruction file at every start.",
        .ConfigChange: "Changes no chat state.",
        .Setup: "Fires only in `-p` runs with `--init` or `--maintenance`, which a chat never is.",
        .WorktreeCreate: "A hook here replaces how Claude Code makes the worktree, which Swarm must not do.",
        .WorktreeRemove: "A hook here replaces how Claude Code removes the worktree, which Swarm must not do."
    ]

    /// Events verified and supported by the installed Codex version (0.154.0).
    public static let codexRegisteredEvents: Set<InteractiveHookEvent> = [
        .SessionStart,
        .UserPromptSubmit,
        .PreToolUse,
        .PostToolUse,
        .PermissionRequest,
        .Stop,
        .SessionEnd
    ]

    /// Whether this hook event should be registered for the given agent kind.
    public func isRegistered(for agent: AgentKind) -> Bool {
        switch agent {
        case .claudeCode:
            return Self.claudeSkippedReasons[self] == nil
        case .codex:
            return Self.codexRegisteredEvents.contains(self)
        case .cursor, .openCode, .grok:
            return false
        }
    }

    /// Reason why an event was skipped for Claude Code, or nil if registered.
    public static func claudeSkippedReason(for event: InteractiveHookEvent) -> String? {
        claudeSkippedReasons[event]
    }
}
