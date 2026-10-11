import SwiftUI
import SwarmCore

struct ScaledDesignTokens {
    var textSize: TextSize = .default
    var density: Density = .comfortable

    var body: Font { DesignTokens.body(textSize: textSize) }
    var mono: Font { DesignTokens.mono(textSize: textSize) }
    var bodyLineSpacing: CGFloat { DesignTokens.bodyLineSpacing(textSize: textSize) }
    var row: CGFloat { DesignTokens.Size.row(density: density) }
    var spacing: Spacing { Spacing(scale: AppearanceScale.spacing(density: density)) }

    struct Spacing {
        let scale: Double
        var xxs: CGFloat { DesignTokens.Spacing.xxs * scale }
        var xs: CGFloat { DesignTokens.Spacing.xs * scale }
        var s: CGFloat { DesignTokens.Spacing.s * scale }
        var m: CGFloat { DesignTokens.Spacing.m * scale }
        var l: CGFloat { DesignTokens.Spacing.l * scale }
        var xl: CGFloat { DesignTokens.Spacing.xl * scale }
    }
}

extension EnvironmentValues {
    @Entry var designTokens = ScaledDesignTokens()
    @Entry var sendKey = SendKey.return
}

struct AppearancePreferences: ViewModifier {
    let prefs: Prefs

    func body(content: Content) -> some View {
        content
            .environment(\.designTokens, ScaledDesignTokens(textSize: prefs.textSize, density: prefs.density))
            .environment(\.sendKey, prefs.sendKey)
            .onChange(of: prefs.theme, initial: true) { _, theme in
                let effective = AppearanceTheme.effective(
                    theme: theme, override: ProcessInfo.processInfo.environment["SWARM_APPEARANCE"]
                )
                NSApp.appearance = switch effective {
                case .system: nil
                case .light: NSAppearance(named: .aqua)
                case .dark: NSAppearance(named: .darkAqua)
                }
            }
    }
}
