import Foundation

public struct Prefs: Codable, Sendable, Hashable {
    public var settingsPage: String?
    public var splitDiff: Bool

    public init(settingsPage: String? = nil, splitDiff: Bool = false) {
        self.settingsPage = settingsPage
        self.splitDiff = splitDiff
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        settingsPage = try container.decodeIfPresent(String.self, forKey: .settingsPage)
        splitDiff = try container.decodeIfPresent(Bool.self, forKey: .splitDiff) ?? false
    }
}

public enum SettingsPage: String, CaseIterable, Sendable, Hashable, Identifiable {
    case profiles, skills, accounts, setup, managedChanges, appearance, notifications, keys, advanced

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .profiles: "Profiles"
        case .skills: "Skills"
        case .accounts: "Accounts"
        case .setup: "Setup"
        case .managedChanges: "Managed Changes"
        case .appearance: "Appearance"
        case .notifications: "Notifications"
        case .keys: "Keys"
        case .advanced: "Advanced"
        }
    }

    public var symbol: String {
        switch self {
        case .profiles: "person.crop.rectangle"
        case .skills: "book"
        case .accounts: "person.crop.circle"
        case .setup: "wrench.and.screwdriver"
        case .managedChanges: "doc.badge.gearshape"
        case .appearance: "paintpalette"
        case .notifications: "bell"
        case .keys: "keyboard"
        case .advanced: "gearshape.2"
        }
    }

    public var placeholder: String? {
        switch self {
        case .skills: "Comes with slice 2"
        case .accounts: "Comes with slice 3"
        case .notifications: "Comes with step 25"
        case .keys: "Comes with step 23"
        case .profiles, .setup, .managedChanges, .appearance, .advanced: nil
        }
    }

    public static let paletteItems: [PaletteItem] = [Self.setup, .managedChanges, .profiles].map {
        PaletteItem(id: "settings:" + $0.rawValue, title: $0.title, group: .action)
    }

    public static func openable(from palette: PaletteItem) -> Self? {
        guard palette.group == .action, palette.id.hasPrefix("settings:") else { return nil }
        return Self(rawValue: String(palette.id.dropFirst("settings:".count)))
    }
}
