import Foundation

public enum Theme: String, Codable, CaseIterable, Sendable {
    case system, light, dark
}

public enum TextSize: String, Codable, CaseIterable, Sendable {
    case small, `default`, large
}

public enum Density: String, Codable, CaseIterable, Sendable {
    case comfortable, compact
}

public enum SendKey: String, Codable, CaseIterable, Sendable {
    case `return`, commandReturn
}

public enum AppearanceScale {
    public struct Fonts: Sendable, Equatable {
        public let body: Double
        public let mono: Double
    }

    public static func fonts(textSize: TextSize) -> Fonts {
        let scale: Double = switch textSize {
        case .small: 0.9
        case .default: 1
        case .large: 1.15
        }
        return Fonts(body: 13 * scale, mono: 12 * scale)
    }

    public static func rowHeight(density: Density) -> Double {
        density == .compact ? 24 : 28
    }
}

public enum SendKeyRule {
    public static func sends(press: KeyChord, sendKey: SendKey) -> Bool {
        press == KeyChord(.returnKey, sendKey == .commandReturn ? .command : [])
    }
}

public enum ProviderGlyph {
    public static func symbol(provider: String) -> String? {
        switch provider.lowercased() {
        case "claude": "sparkles"
        case "codex": "terminal"
        case "agy": "globe"
        default: nil
        }
    }
}

public enum SidebarWidth {
    public static let range: ClosedRange<Double> = 230...560
    public static let step: Double = 20
}
