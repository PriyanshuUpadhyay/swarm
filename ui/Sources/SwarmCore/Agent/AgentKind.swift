import Foundation

public enum AgentKind: String, Sendable, Codable, CaseIterable, Identifiable {
    case claudeCode, codex, grok, cursor, openCode
    public var id: String { rawValue }
    public var label: String {
        switch self {
        case .claudeCode: "Claude Code"
        case .codex: "Codex"
        case .grok: "Grok"
        case .cursor: "Cursor"
        case .openCode: "OpenCode"
        }
    }
    public var executableName: String {
        switch self {
        case .claudeCode: "claude"
        case .codex: "codex"
        case .grok: "grok"
        case .cursor: "cursor-agent"
        case .openCode: "opencode"
        }
    }
}

public enum PermissionMode: String, Sendable, Codable, CaseIterable {
    case auto, acceptEdits, autoReview, bypassPermissions, plan

    public func nearest(on kind: AgentKind) -> Self {
        switch self {
        case .autoReview: kind == .codex ? self : .auto
        case .plan: kind == .codex ? .auto : self
        default: self
        }
    }
    public var cliValue: String {
        switch self {
        case .auto, .autoReview: "auto"
        case .acceptEdits: "acceptEdits"
        case .bypassPermissions: "bypassPermissions"
        case .plan: "plan"
        }
    }
    public var codexSandbox: String {
        switch self {
        case .bypassPermissions: "danger-full-access"
        case .acceptEdits, .autoReview: "workspace-write"
        case .auto, .plan: "read-only"
        }
    }
    public var codexApproval: String { self == .bypassPermissions ? "never" : "on-request" }
}
