import Foundation

public enum RowField: String, Codable, CaseIterable, Sendable {
    case status, title, provider, branch, age, children, question, unread, dirty, model, effort, steps, pr, ci, tokens, cost
}

/// The owner picks the order separately for each surface. Unknown names are skipped so a
/// choices file from a newer app still keeps the fields this app knows (ADR 0052).
public struct RowFieldLists: Codable, Equatable, Sendable {
    public var project: [RowField] = [.title, .status]
    public var workspace: [RowField] = [.status, .title, .branch, .children, .age, .steps]
    public var chat: [RowField] = [.status, .title, .age, .steps, .children, .unread, .tokens]
    public var tab: [RowField] = [.status, .title, .provider, .unread, .tokens]

    public init() {}

    public init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        func fields(_ key: CodingKeys, default bundled: [RowField]) throws -> [RowField] {
            try values.decodeIfPresent([String].self, forKey: key)?.compactMap(RowField.init(rawValue:)) ?? bundled
        }
        project = try fields(.project, default: project)
        workspace = try fields(.workspace, default: workspace)
        chat = try fields(.chat, default: chat)
        tab = try fields(.tab, default: tab)
    }
}
