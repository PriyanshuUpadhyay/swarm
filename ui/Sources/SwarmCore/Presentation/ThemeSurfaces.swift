import Foundation

public struct ThemeSurfaces: Codable, Sendable, Hashable {
    public var surface: PaletteInk.Pair
    public var raised: PaletteInk.Pair
    public var sunken: PaletteInk.Pair
    public var sidebar: PaletteInk.Pair
    public var border: PaletteInk.Pair
    public var selected: PaletteInk.Pair
    public var glassTint: PaletteInk.Pair?
    public var glassTintOpacity: Double?
    /// The fill behind the reader's own message. Nil keeps the filled accent bubble.
    public var bubble: PaletteInk.Pair?
    /// Links, ticks and other brand ink. Nil is Swarm's teal.
    public var accent: PaletteInk.Pair?
    /// The quietest text: counts, ages, section headings. Nil is Swarm's blue grey.
    public var muted: PaletteInk.Pair?

    public init(
        surface: PaletteInk.Pair, raised: PaletteInk.Pair, sunken: PaletteInk.Pair,
        sidebar: PaletteInk.Pair, border: PaletteInk.Pair, selected: PaletteInk.Pair,
        glassTint: PaletteInk.Pair? = nil, glassTintOpacity: Double? = nil,
        bubble: PaletteInk.Pair? = nil, accent: PaletteInk.Pair? = nil, muted: PaletteInk.Pair? = nil
    ) {
        self.surface = surface
        self.raised = raised
        self.sunken = sunken
        self.sidebar = sidebar
        self.border = border
        self.selected = selected
        self.glassTint = glassTint
        self.glassTintOpacity = glassTintOpacity
        self.bubble = bubble
        self.accent = accent
        self.muted = muted
    }

    /// The grounds text is set on, which every meaning ink has to clear. The page comes first,
    /// because it is the one `readable` puts first when the grounds cannot all be served.
    public var textGrounds: [PaletteInk.Pair] { [surface, raised, sunken, sidebar] }

    /// The same, and the two fills label text also sits on: a selected row and the reader's own
    /// message. Only the label inks are held to these; the meaning inks keep their tuned values.
    public var labelGrounds: [PaletteInk.Pair] { textGrounds + [selected] + (bubble.map { [$0] } ?? []) }

    /// Done and passed. Swarm's accent when the theme keeps Swarm's accent, which is what the ramp
    /// asks for; a green of its own when the theme brings an accent, because a blue or amber accent
    /// would say "done" in the colour that already says "running" or "warning".
    public var positive: PaletteInk.Pair { accent == nil ? PaletteInk.accent : PaletteInk.success }

    /// `ink` with its lightness moved, hue and saturation kept, until it clears `floor` on every
    /// ground in `textGrounds`.
    ///
    /// The meaning inks were tuned against Swarm's white and navy. A cream or a lilac ground takes
    /// a few percent of contrast off each of them, so either a theme bleaches its grounds to fit
    /// the inks or the inks follow the grounds. This is the second, so Solarized stays cream.
    public func readable(
        _ ink: PaletteInk.Pair, floor: Double = Contrast.textFloor, labels: Bool = false
    ) -> PaletteInk.Pair {
        let grounds = labels ? labelGrounds : textGrounds
        return PaletteInk.Pair(
            light: Self.readable(ink.light, on: grounds.map(\.light), floor: floor),
            dark: Self.readable(ink.dark, on: grounds.map(\.dark), floor: floor)
        )
    }

    /// Moves away from the page: lighter on a dark page, darker on a light one. A person can pick
    /// a white page with dark panels, and then no ink reads on both; the page wins, since that is
    /// where most text is.
    static func readable(_ ink: UInt32, on grounds: [UInt32], floor: Double) -> UInt32 {
        guard let page = grounds.first else { return ink }
        let lifts = Contrast.relativeLuminance(of: page) < 0.18
        var hsl = HSL(ink)
        var colour = ink
        var readsOnPage: UInt32?
        for _ in 0..<200 {
            if grounds.allSatisfy({ Contrast.ratio(colour, $0) >= floor }) { return colour }
            if readsOnPage == nil, Contrast.ratio(colour, page) >= floor { readsOnPage = colour }
            hsl.lightness = min(1, max(0, hsl.lightness + (lifts ? 0.005 : -0.005)))
            colour = hsl.rgb
        }
        return readsOnPage ?? colour
    }
}

/// One colour of a theme a person can change in Settings.
public enum ThemeColourRole: String, Codable, CaseIterable, Sendable, Identifiable {
    case surface, sidebar, sunken, raised, border, selected, bubble, accent, muted

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .surface: "Background"
        case .sidebar: "Title bar"
        case .sunken: "Sidebar and panels"
        case .raised: "Composer and controls"
        case .border: "Dividers"
        case .selected: "Selected row"
        case .bubble: "Your messages"
        case .accent: "Accent"
        case .muted: "Muted text"
        }
    }

    public func value(in surfaces: ThemeSurfaces) -> PaletteInk.Pair {
        switch self {
        case .surface: surfaces.surface
        case .sidebar: surfaces.sidebar
        case .sunken: surfaces.sunken
        case .raised: surfaces.raised
        case .border: surfaces.border
        case .selected: surfaces.selected
        case .bubble: surfaces.bubble ?? surfaces.raised
        case .accent: surfaces.accent ?? PaletteInk.accent
        case .muted: surfaces.muted ?? PaletteInk.textTertiary
        }
    }

    public func set(_ value: PaletteInk.Pair, in surfaces: inout ThemeSurfaces) {
        switch self {
        case .surface: surfaces.surface = value
        case .sidebar: surfaces.sidebar = value
        case .sunken: surfaces.sunken = value
        case .raised: surfaces.raised = value
        case .border: surfaces.border = value
        case .selected: surfaces.selected = value
        case .bubble: surfaces.bubble = value
        case .accent: surfaces.accent = value
        case .muted: surfaces.muted = value
        }
    }
}

/// Hue, saturation and lightness, only as far as `ThemeSurfaces.readable` needs them.
struct HSL {
    var hue: Double
    var saturation: Double
    var lightness: Double

    init(_ rgb: UInt32) {
        let r = Double((rgb >> 16) & 0xFF) / 255
        let g = Double((rgb >> 8) & 0xFF) / 255
        let b = Double(rgb & 0xFF) / 255
        let high = max(r, g, b)
        let low = min(r, g, b)
        lightness = (high + low) / 2
        let chroma = high - low
        guard chroma > 0 else {
            hue = 0
            saturation = 0
            return
        }
        saturation = chroma / (1 - abs(2 * lightness - 1))
        let sector: Double = switch high {
        case r: ((g - b) / chroma).truncatingRemainder(dividingBy: 6)
        case g: (b - r) / chroma + 2
        default: (r - g) / chroma + 4
        }
        hue = (sector * 60 + 360).truncatingRemainder(dividingBy: 360)
    }

    var rgb: UInt32 {
        let chroma = (1 - abs(2 * lightness - 1)) * saturation
        let x = chroma * (1 - abs((hue / 60).truncatingRemainder(dividingBy: 2) - 1))
        let m = lightness - chroma / 2
        let (r, g, b): (Double, Double, Double) = switch hue {
        case ..<60: (chroma, x, 0)
        case ..<120: (x, chroma, 0)
        case ..<180: (0, chroma, x)
        case ..<240: (0, x, chroma)
        case ..<300: (x, 0, chroma)
        default: (chroma, 0, x)
        }
        let byte = { (v: Double) in UInt32(max(0, min(255, ((v + m) * 255).rounded()))) }
        return byte(r) << 16 | byte(g) << 8 | byte(b)
    }
}
