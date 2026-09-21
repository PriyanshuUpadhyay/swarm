import Foundation
import Testing
@testable import SwarmCore

@Suite("Colour themes")
struct ColourThemeTests {
    @Test func unknownPreferenceUsesConductor() {
        #expect(ColourTheme(storedValue: nil) == .conductor)
        #expect(ColourTheme(storedValue: "removed") == .conductor)
        #expect(ColourTheme(storedValue: "neutral") == .charcoalGlass)
        #expect(ColourTheme(storedValue: "swarm") == .swarm)
    }

    @Test func presetIdsAreUnique() {
        #expect(Set(ColourTheme.allCases.map(\.id)).count == ColourTheme.allCases.count)
        for theme in ColourTheme.allCases {
            #expect(CodeScheme.all.contains { $0.id == theme.codeScheme })
            #expect(TerminalScheme.all.contains { $0.id == theme.terminalScheme })
        }
    }

    @Test(arguments: ColourTheme.allCases)
    func jsonRoundTrip(theme: ColourTheme) throws {
        let data = try JSONEncoder().encode(theme)
        let decoded = try JSONDecoder().decode(ColourTheme.self, from: data)
        #expect(decoded == theme)
    }

    @Test func charcoalKeepsApprovedGlass() {
        let theme = ColourTheme.charcoalGlass
        #expect(theme.glass == .thick)
        #expect(theme.surfaces.glassTint?.dark == 0x212938)
        #expect(theme.glass.tintOpacity(maximum: theme.surfaces.glassTintOpacity ?? 0.4) == 0.4)
    }

    @Test func customThemeDefinition() throws {
        let json = """
        {
            "schemaVersion": 1,
            "id": "custom", "title": "Custom", "glass": "off",
            "codeScheme": "swarm", "terminalScheme": "swarm",
            "codeTypography": {}, "terminalTypography": {},
            "chatFont": "system", "chatTextSize": "large", "chatLineHeight": "standard",
            "surfaces": {
                "surface": {"light": 16777215, "dark": 2698291},
                "raised": {"light": 16777215, "dark": 2764084},
                "sunken": {"light": 16448250, "dark": 2435119},
                "sidebar": {"light": 16119285, "dark": 3159099},
                "border": {"light": 14606046, "dark": 5198684},
                "selected": {"light": 15066597, "dark": 4343115}
            }
        }
        """
        let theme = try JSONDecoder().decode(ColourTheme.self, from: Data(json.utf8))
        #expect(theme.id == "custom")
        #expect(theme.glass == .off)
        #expect(theme.surfaces.surface.dark == 0x292C33)
    }

    @Test(arguments: ColourTheme.allCases, [false, true])
    func readableInk(theme: ColourTheme, dark: Bool) {
        let surfaces = theme.surfaces
        let grounds = [surfaces.surface, surfaces.raised, surfaces.sunken]
        let inks = [
            PaletteInk.textTertiary, PaletteInk.accent, PaletteInk.negative,
            PaletteInk.stop, PaletteInk.warning, PaletteInk.running, PaletteInk.merged,
        ]
        // The inks as the window draws them: moved to read on this theme. See `readable`.
        let themed = [surfaces.accent ?? PaletteInk.accent, surfaces.muted ?? PaletteInk.textTertiary]
        let drawn = (inks + themed).map { surfaces.readable($0) }
        for ground in grounds {
            for ink in drawn {
                #expect(Contrast.ratio(ink.member(dark: dark), ground.member(dark: dark)) >= Contrast.textFloor)
            }
            #expect(Contrast.ratio(surfaces.border.member(dark: dark), ground.member(dark: dark)) >= 1.2)
        }
    }

    /// Code sits on its scheme's own ground, and every token is checked against that ground in
    /// `codeSchemeRoundTripAndContrast`. What is left to hold here is that a preset's scheme
    /// ground is the preset's page, so a code block does not read as a patch.
    @Test(arguments: ColourTheme.allCases)
    func codeSitsOnThePage(theme: ColourTheme) {
        #expect(CodeScheme.find(theme.codeScheme).background == theme.surfaces.surface)
    }

    /// Solarized's cream costs the warning ink its contrast. The ink moves, darker, and keeps its
    /// hue, rather than the cream moving to white.
    @Test func inksFollowTheGroundAndKeepTheirHue() {
        let cream = ColourTheme.solarized.surfaces
        let moved = cream.readable(PaletteInk.warning)
        #expect(Contrast.ratio(PaletteInk.warning.light, cream.surface.light) < Contrast.textFloor)
        #expect(Contrast.ratio(moved.light, cream.surface.light) >= Contrast.textFloor)
        #expect(abs(HSL(moved.light).hue - HSL(PaletteInk.warning.light).hue) < 3)
        // An ink that already reads is left exactly where it was.
        #expect(ColourTheme.swarm.surfaces.readable(PaletteInk.negative).dark == PaletteInk.negative.dark)
    }

    /// On Tokyo Night, Nord, GitHub and One the accent is blue and on Gruvbox amber, so a "done"
    /// drawn in the accent read as "running" or "warning". A glance at the sidebar has to tell
    /// those apart on every preset.
    @Test(arguments: ColourTheme.allCases, [false, true])
    func doneIsNotRunningOrWarning(theme: ColourTheme, dark: Bool) {
        let surfaces = theme.surfaces
        let done = surfaces.readable(surfaces.positive).member(dark: dark)
        let running = surfaces.readable(PaletteInk.running).member(dark: dark)
        let warning = surfaces.readable(PaletteInk.warning).member(dark: dark)
        #expect(Contrast.deltaE(done, running) >= 20, "\(theme.id) done against running")
        #expect(Contrast.deltaE(done, warning) >= 20, "\(theme.id) done against warning")
    }

    /// Every preset keeps the system's own label colours, which is what tracks Increase Contrast.
    @Test(arguments: ColourTheme.allCases)
    func presetsKeepSystemLabels(theme: ColourTheme) {
        let surfaces = theme.surfaces
        #expect(surfaces.readable(PaletteInk.labelPrimary, labels: true) == PaletteInk.labelPrimary)
    }

    /// A white background chosen in dark appearance left the system's white label on it.
    @Test func labelInkFollowsAChangedGround() {
        var surfaces = ColourTheme.conductor.surfaces
        #expect(surfaces.readable(PaletteInk.labelPrimary, labels: true) == PaletteInk.labelPrimary)
        surfaces.surface = .init(light: 0xFFFFFF, dark: 0xFFFFFF)
        let moved = surfaces.readable(PaletteInk.labelPrimary, labels: true)
        #expect(Contrast.ratio(moved.dark, 0xFFFFFF) >= Contrast.textFloor)
    }

    @Test func changedColoursLayOverThePreset() throws {
        var changes = ThemeOverrides()
        changes.colours = [ThemeColourRole.surface.rawValue: .init(light: 0x123456, dark: 0x654321)]
        let surfaces = changes.surfaces(for: .conductor)
        #expect(surfaces.surface == .init(light: 0x123456, dark: 0x654321))
        #expect(surfaces.sunken == ColourTheme.conductor.surfaces.sunken)
        let restored = try JSONDecoder().decode(ThemeOverrides.self, from: JSONEncoder().encode(changes))
        #expect(restored == changes)
    }
}
