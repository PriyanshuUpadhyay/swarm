import Foundation
import CoreGraphics
import Testing
@testable import SwarmCore

@Suite("Appearance preferences")
struct AppearanceTests {
    @Test("Missing, unknown, null and malformed appearance values keep defaults")
    func tolerantDecode() throws {
        let decoder = JSONDecoder()
        for json in ["{}", #"{"theme":"future","textSize":"huge","density":"dense","sendKey":"enter"}"#,
                     #"{"theme":null,"textSize":1,"density":false,"sendKey":[]}"#] {
            #expect(try decoder.decode(Prefs.self, from: Data(json.utf8)) == Prefs())
        }
        let prefs = Prefs(settingsPage: "appearance", splitDiff: true, theme: .dark,
                          textSize: .large, density: .compact, sendKey: .commandReturn)
        #expect(try decoder.decode(Prefs.self, from: JSONEncoder().encode(prefs)) == prefs)
        let partial = try decoder.decode(Prefs.self, from: Data(#"{"theme":"light","textSize":"future","splitDiff":true}"#.utf8))
        #expect(partial.theme == .light && partial.textSize == .default && partial.splitDiff)
    }

    @Test("Text sizes scale both fonts and density selects row height")
    func scale() {
        for (size, multiplier) in [(TextSize.small, 0.9), (.default, 1.0), (.large, 1.15)] {
            let fonts = AppearanceScale.fonts(textSize: size)
            #expect(fonts.body == 13 * multiplier)
            #expect(fonts.mono == 12 * multiplier)
        }
        #expect(AppearanceScale.rowHeight(density: .comfortable) == 28)
        #expect(AppearanceScale.rowHeight(density: .compact) == 24)
    }

    @Test("Only the selected send chord sends, and shifted Return always inserts")
    func sendChord() {
        for setting in SendKey.allCases {
            for flags in 0..<16 {
                let modifiers = KeyChord.Modifiers(rawValue: flags)
                let expected = modifiers == (setting == .return ? [] : .command)
                #expect(SendKeyRule.sends(press: KeyChord(.returnKey, modifiers), sendKey: setting) == expected)
                #expect(!SendKeyRule.sends(press: KeyChord(.tab, modifiers), sendKey: setting))
            }
        }
    }

    @Test("Known providers have distinct symbols and unknown providers keep their letter")
    func providers() {
        let symbols = ["claude", "codex", "agy"].compactMap { ProviderGlyph.symbol(provider: $0) }
        #expect(symbols.count == 3 && Set(symbols).count == 3)
        #expect(ProviderGlyph.symbol(provider: "CLAUDE") == ProviderGlyph.symbol(provider: "claude"))
        #expect(ProviderGlyph.symbol(provider: "unknown") == nil)
        #expect(ProviderGlyph.symbol(provider: "") == nil)
        #expect(ChatTab.badge("unknown") == "U")
    }

    @Test("Sidebar range includes 560 pt with a 20 pt adjustment step")
    func sidebarRange() {
        #expect(SidebarWidth.range == 230...560)
        #expect(SidebarWidth.step == 20)
        #expect(!SidebarWidth.range.contains(229) && !SidebarWidth.range.contains(561))
    }

    @Test("Compact spacing follows row density, while comfortable spacing stays unchanged")
    func spacingScale() {
        #expect(AppearanceScale.spacing(density: .comfortable) == 1)
        #expect(AppearanceScale.spacing(density: .compact) == 24.0 / 28.0)
    }

    @Test("Screenshot theme overrides win and unknown overrides preserve the preference")
    func themeOverride() {
        for theme in Theme.allCases {
            #expect(AppearanceTheme.effective(theme: theme, override: "dark") == .dark)
            #expect(AppearanceTheme.effective(theme: theme, override: "light") == .light)
            #expect(AppearanceTheme.effective(theme: theme, override: nil) == theme)
            #expect(AppearanceTheme.effective(theme: theme, override: "other") == theme)
        }
    }

    @Test("The first window centers at 1280 by 820 and fits smaller visible frames")
    func initialWindow() {
        let desktop = CGRect(x: 100, y: -200, width: 1600, height: 1000)
        #expect(FirstWindowFrame.frame(visibleFrame: desktop) == CGRect(x: 260, y: -110, width: 1280, height: 820))
        let laptop = CGRect(x: -1200, y: 40, width: 1100, height: 700)
        #expect(FirstWindowFrame.frame(visibleFrame: laptop) == laptop)
        let narrow = CGRect(x: 0, y: 0, width: 1000, height: 1000)
        #expect(FirstWindowFrame.frame(visibleFrame: narrow) == CGRect(x: 0, y: 90, width: 1000, height: 820))
    }
}
