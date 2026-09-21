extension ColourTheme {
    public static let swarm = ColourTheme(
        id: "swarm",
        title: "Swarm",
        glass: .thick,
        surfaces: ThemeSurfaces(
            surface: PaletteInk.surface,
            raised: PaletteInk.surfaceRaised,
            sunken: PaletteInk.surfaceSunken,
            sidebar: PaletteInk.sidebar,
            border: PaletteInk.border,
            selected: PaletteInk.selected,
            glassTint: PaletteInk.sidebar,
            glassTintOpacity: 0.8
        )
    )

    public static let charcoalGlass = ColourTheme(
        id: "neutral",
        title: "Charcoal Glass",
        glass: .thick,
        surfaces: ThemeSurfaces(
            surface: .init(light: 0xFFFFFF, dark: 0x292C33),
            raised: .init(light: 0xFFFFFF, dark: 0x2A2D34),
            sunken: .init(light: 0xFAFAFA, dark: 0x25282F),
            sidebar: .init(light: 0xF5F5F5, dark: 0x30343B),
            border: .init(light: 0xDEDEDE, dark: 0x4F535C),
            selected: .init(light: 0xE5E5E5, dark: 0x42454B),
            glassTint: .init(light: 0xFAFAFA, dark: 0x212938),
            glassTintOpacity: 0.4,
            muted: .init(light: 0x6E6E73, dark: 0x98989D)
        ),
        codeScheme: "charcoal", terminalScheme: "charcoal"
    )

    // Warm stone, sampled off Conductor's own screenshots, then the classic editor palettes.
    // Each keeps its own grounds; the meaning inks follow them. See `ThemeSurfaces.readable`.
    public static let conductor = ColourTheme(
        id: "conductor", title: "Conductor", glass: .off,
        surfaces: ThemeSurfaces(
            surface: .init(light: 0xFFFFFF, dark: 0x13110F), raised: .init(light: 0xFFFFFF, dark: 0x201E1C),
            sunken: .init(light: 0xFAFAFA, dark: 0x1B1918), sidebar: .init(light: 0xFAFAFA, dark: 0x1B1918),
            border: .init(light: 0xE4E3E1, dark: 0x312E2C), selected: .init(light: 0xEFEEEC, dark: 0x292725),
            bubble: .init(light: 0xF3F1EF, dark: 0x272220), accent: .init(light: 0x2F7D3A, dark: 0x7CD992),
            muted: .init(light: 0x7E7B78, dark: 0x8A8784)
        ),
        codeScheme: "conductor", terminalScheme: "conductor"
    )

    public static let catppuccin = ColourTheme(
        id: "catppuccin", title: "Catppuccin", glass: .off,
        surfaces: ThemeSurfaces(
            surface: .init(light: 0xEFF1F5, dark: 0x1E1E2E), raised: .init(light: 0xF8F9FB, dark: 0x313244),
            sunken: .init(light: 0xE6E9EF, dark: 0x181825), sidebar: .init(light: 0xE6E9EF, dark: 0x181825),
            border: .init(light: 0xCCD0DA, dark: 0x45475A), selected: .init(light: 0xDCE0E8, dark: 0x313244),
            bubble: .init(light: 0xDCE0E8, dark: 0x313244), accent: .init(light: 0x8839EF, dark: 0xCBA6F7),
            muted: .init(light: 0x6C6F85, dark: 0x9399B2)
        ),
        codeScheme: "catppuccin", terminalScheme: "catppuccin"
    )

    public static let dracula = ColourTheme(
        id: "dracula", title: "Dracula", glass: .off,
        surfaces: ThemeSurfaces(
            surface: .init(light: 0xFFFBEB, dark: 0x282A36), raised: .init(light: 0xFFFFFF, dark: 0x343746),
            sunken: .init(light: 0xF6F2E2, dark: 0x21222C), sidebar: .init(light: 0xF6F2E2, dark: 0x21222C),
            border: .init(light: 0xDED9C4, dark: 0x44475A), selected: .init(light: 0xECE7D4, dark: 0x3B3E4E),
            bubble: .init(light: 0xECE7D4, dark: 0x44475A), accent: .init(light: 0x644AC9, dark: 0xBD93F9),
            muted: .init(light: 0x6C664B, dark: 0x6272A4)
        ),
        codeScheme: "dracula", terminalScheme: "dracula"
    )

    public static let nord = ColourTheme(
        id: "nord", title: "Nord", glass: .off,
        surfaces: ThemeSurfaces(
            surface: .init(light: 0xECEFF4, dark: 0x2E3440), raised: .init(light: 0xF8F9FB, dark: 0x3B4252),
            sunken: .init(light: 0xE5E9F0, dark: 0x292E39), sidebar: .init(light: 0xE5E9F0, dark: 0x292E39),
            border: .init(light: 0xCBD2DE, dark: 0x4C566A), selected: .init(light: 0xD8DEE9, dark: 0x3B4252),
            bubble: .init(light: 0xD8DEE9, dark: 0x3B4252), accent: .init(light: 0x5E81AC, dark: 0x88C0D0),
            muted: .init(light: 0x4C566A, dark: 0x7B88A1)
        ),
        codeScheme: "nord", terminalScheme: "nord"
    )

    public static let tokyoNight = ColourTheme(
        id: "tokyoNight", title: "Tokyo Night", glass: .off,
        surfaces: ThemeSurfaces(
            surface: .init(light: 0xE9EAEF, dark: 0x1A1B26), raised: .init(light: 0xF4F5F8, dark: 0x24283B),
            sunken: .init(light: 0xE1E2E7, dark: 0x16161E), sidebar: .init(light: 0xE1E2E7, dark: 0x16161E),
            border: .init(light: 0xC4C8DA, dark: 0x3B4261), selected: .init(light: 0xD6D9E4, dark: 0x292E42),
            bubble: .init(light: 0xD6D9E4, dark: 0x292E42), accent: .init(light: 0x2E7DE9, dark: 0x7AA2F7),
            muted: .init(light: 0x6172B0, dark: 0x565F89)
        ),
        codeScheme: "tokyoNight", terminalScheme: "tokyoNight"
    )

    public static let gruvbox = ColourTheme(
        id: "gruvbox", title: "Gruvbox", glass: .off,
        surfaces: ThemeSurfaces(
            surface: .init(light: 0xFBF1C7, dark: 0x282828), raised: .init(light: 0xFDF7DD, dark: 0x3C3836),
            sunken: .init(light: 0xF2E5BC, dark: 0x1D2021), sidebar: .init(light: 0xF2E5BC, dark: 0x1D2021),
            border: .init(light: 0xD5C4A1, dark: 0x504945), selected: .init(light: 0xEBDBB2, dark: 0x3C3836),
            bubble: .init(light: 0xEBDBB2, dark: 0x3C3836), accent: .init(light: 0xAF3A03, dark: 0xFABD2F),
            muted: .init(light: 0x7C6F64, dark: 0x928374)
        ),
        codeScheme: "gruvbox", terminalScheme: "gruvbox"
    )

    public static let solarized = ColourTheme(
        id: "solarized", title: "Solarized", glass: .off,
        surfaces: ThemeSurfaces(
            surface: .init(light: 0xFDF6E3, dark: 0x002B36), raised: .init(light: 0xFFFBF0, dark: 0x073642),
            sunken: .init(light: 0xF5EFDC, dark: 0x00252F), sidebar: .init(light: 0xEEE8D5, dark: 0x00252F),
            border: .init(light: 0xDDD6C1, dark: 0x1C4B57), selected: .init(light: 0xEEE8D5, dark: 0x073642),
            bubble: .init(light: 0xEEE8D5, dark: 0x073642), accent: .init(light: 0x268BD2, dark: 0x268BD2),
            muted: .init(light: 0x657B83, dark: 0x839496)
        ),
        codeScheme: "solarized", terminalScheme: "solarized"
    )

    public static let rosePine = ColourTheme(
        id: "rosePine", title: "Rosé Pine", glass: .off,
        surfaces: ThemeSurfaces(
            surface: .init(light: 0xFAF4ED, dark: 0x191724), raised: .init(light: 0xFFFAF3, dark: 0x26233A),
            sunken: .init(light: 0xF4EDE8, dark: 0x1F1D2E), sidebar: .init(light: 0xF2E9E1, dark: 0x1F1D2E),
            border: .init(light: 0xDCD7D6, dark: 0x403D52), selected: .init(light: 0xF2E9E1, dark: 0x26233A),
            bubble: .init(light: 0xF2E9E1, dark: 0x26233A), accent: .init(light: 0x907AA9, dark: 0xC4A7E7),
            muted: .init(light: 0x797593, dark: 0x6E6A86)
        ),
        codeScheme: "rosePine", terminalScheme: "rosePine"
    )

    public static let github = ColourTheme(
        id: "github", title: "GitHub", glass: .off,
        surfaces: ThemeSurfaces(
            surface: .init(light: 0xFFFFFF, dark: 0x0D1117), raised: .init(light: 0xFFFFFF, dark: 0x161B22),
            sunken: .init(light: 0xF6F8FA, dark: 0x010409), sidebar: .init(light: 0xF6F8FA, dark: 0x010409),
            border: .init(light: 0xD0D7DE, dark: 0x30363D), selected: .init(light: 0xEAEEF2, dark: 0x21262D),
            bubble: .init(light: 0xEAEEF2, dark: 0x21262D), accent: .init(light: 0x0969DA, dark: 0x4493F8),
            muted: .init(light: 0x656D76, dark: 0x7D8590)
        ),
        codeScheme: "github", terminalScheme: "github"
    )

    public static let one = ColourTheme(
        id: "one", title: "One", glass: .off,
        surfaces: ThemeSurfaces(
            surface: .init(light: 0xFAFAFA, dark: 0x282C34), raised: .init(light: 0xFFFFFF, dark: 0x2C313A),
            sunken: .init(light: 0xF0F0F1, dark: 0x21252B), sidebar: .init(light: 0xEAEAEB, dark: 0x21252B),
            border: .init(light: 0xDADADB, dark: 0x3E4451), selected: .init(light: 0xE5E5E6, dark: 0x2C313A),
            bubble: .init(light: 0xE5E5E6, dark: 0x2C313A), accent: .init(light: 0x4078F2, dark: 0x61AFEF),
            muted: .init(light: 0x696C77, dark: 0x7F848E)
        ),
        codeScheme: "one", terminalScheme: "one"
    )
}
