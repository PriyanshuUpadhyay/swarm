extension CodeScheme {
    public static let swarm = builtin(id: "swarm", title: "Swarm", background: PaletteInk.surface)
    public static let charcoal = builtin(
        id: "charcoal", title: "Charcoal", background: .init(light: 0xFFFFFF, dark: 0x292C33)
    )


    public static let conductor = CodeScheme(
        id: "conductor", title: "Conductor", background: .init(light: 0xFFFFFF, dark: 0x13110F),
        foreground: .init(light: 0x232120, dark: 0xF1EFEE),
        gutter: .init(light: 0x6D7680, dark: 0x9A958F), caret: .init(light: 0x2F7D3A, dark: 0x7CD992),
        selection: .init(light: 0xEFEEEC, dark: 0x292725),
        diffAdd: .init(light: 0x1A7F37, dark: 0x7CD992), diffDelete: .init(light: 0xCF222E, dark: 0xF47067),
        tokens: [
            .keyword: .init(light: 0xCF222E, dark: 0xFF7B72),
            .type: .init(light: 0x953800, dark: 0xFFA657),
            .string: .init(light: 0x0A3069, dark: 0xA5D6FF),
            .number: .init(light: 0x0550AE, dark: 0x79C0FF),
            .comment: .init(light: 0x6D7680, dark: 0x9A958F),
            .function: .init(light: 0x8250DF, dark: 0xD2A8FF),
            .variable: .init(light: 0x953800, dark: 0xFFA657),
            .attribute: .init(light: 0x116329, dark: 0x7EE787),
            .`operator`: .init(light: 0x57534E, dark: 0xC9C5C0),
            .punctuation: .init(light: 0x57534E, dark: 0xC9C5C0),
            .regex: .init(light: 0x0A3069, dark: 0xA5D6FF),
            .constant: .init(light: 0x0550AE, dark: 0x79C0FF),
        ]
    )

    public static let catppuccin = CodeScheme(
        id: "catppuccin", title: "Catppuccin", background: .init(light: 0xEFF1F5, dark: 0x1E1E2E),
        foreground: .init(light: 0x4C4F69, dark: 0xCDD6F4),
        gutter: .init(light: 0x696B81, dark: 0x9399B2), caret: .init(light: 0x8839EF, dark: 0xCBA6F7),
        selection: .init(light: 0xDCE0E8, dark: 0x313244),
        diffAdd: .init(light: 0x40A02B, dark: 0xA6E3A1), diffDelete: .init(light: 0xD20F39, dark: 0xF38BA8),
        tokens: [
            .keyword: .init(light: 0x8839EF, dark: 0xCBA6F7),
            .type: .init(light: 0x976014, dark: 0xF9E2AF),
            .string: .init(light: 0x317A21, dark: 0xA6E3A1),
            .number: .init(light: 0xBC4501, dark: 0xFAB387),
            .comment: .init(light: 0x696B81, dark: 0x9399B2),
            .function: .init(light: 0x145FF5, dark: 0x89B4FA),
            .variable: .init(light: 0x4C4F69, dark: 0xCDD6F4),
            .attribute: .init(light: 0x976014, dark: 0xF9E2AF),
            .`operator`: .init(light: 0x0374A1, dark: 0x89DCEB),
            .punctuation: .init(light: 0x0374A1, dark: 0x89DCEB),
            .regex: .init(light: 0x317A21, dark: 0xA6E3A1),
            .constant: .init(light: 0xBC4501, dark: 0xFAB387),
        ]
    )

    public static let dracula = CodeScheme(
        id: "dracula", title: "Dracula", background: .init(light: 0xFFFBEB, dark: 0x282A36),
        foreground: .init(light: 0x1F1F1F, dark: 0xF8F8F2),
        gutter: .init(light: 0x6C664B, dark: 0x8692B9), caret: .init(light: 0x644AC9, dark: 0xBD93F9),
        selection: .init(light: 0xECE7D4, dark: 0x3B3E4E),
        diffAdd: .init(light: 0x14710A, dark: 0x50FA7B), diffDelete: .init(light: 0xCB3A2A, dark: 0xFF5555),
        tokens: [
            .keyword: .init(light: 0xA3144D, dark: 0xFF79C6),
            .type: .init(light: 0x036A96, dark: 0x8BE9FD),
            .string: .init(light: 0x846E15, dark: 0xF1FA8C),
            .number: .init(light: 0x644AC9, dark: 0xBD93F9),
            .comment: .init(light: 0x6C664B, dark: 0x8692B9),
            .function: .init(light: 0x14710A, dark: 0x50FA7B),
            .variable: .init(light: 0x1F1F1F, dark: 0xF8F8F2),
            .attribute: .init(light: 0xA34D14, dark: 0xFFB86C),
            .`operator`: .init(light: 0xA3144D, dark: 0xFF79C6),
            .punctuation: .init(light: 0xA3144D, dark: 0xFF79C6),
            .regex: .init(light: 0x846E15, dark: 0xF1FA8C),
            .constant: .init(light: 0x644AC9, dark: 0xBD93F9),
        ]
    )

    public static let nord = CodeScheme(
        id: "nord", title: "Nord", background: .init(light: 0xECEFF4, dark: 0x2E3440),
        foreground: .init(light: 0x2E3440, dark: 0xD8DEE9),
        gutter: .init(light: 0x4C566A, dark: 0x949FB3), caret: .init(light: 0x5E81AC, dark: 0x88C0D0),
        selection: .init(light: 0xD8DEE9, dark: 0x3B4252),
        diffAdd: .init(light: 0xA3BE8C, dark: 0xA3BE8C), diffDelete: .init(light: 0xBF616A, dark: 0xBF616A),
        tokens: [
            .keyword: .init(light: 0x4D6D95, dark: 0x81A1C1),
            .type: .init(light: 0x457372, dark: 0x8FBCBB),
            .string: .init(light: 0x587341, dark: 0xA3BE8C),
            .number: .init(light: 0x8A5C82, dark: 0xB793B0),
            .comment: .init(light: 0x4C566A, dark: 0x949FB3),
            .function: .init(light: 0x357385, dark: 0x88C0D0),
            .variable: .init(light: 0x2E3440, dark: 0xD8DEE9),
            .attribute: .init(light: 0xA85237, dark: 0xD28C76),
            .`operator`: .init(light: 0x496E93, dark: 0x81A1C1),
            .punctuation: .init(light: 0x496E93, dark: 0x81A1C1),
            .regex: .init(light: 0x587341, dark: 0xA3BE8C),
            .constant: .init(light: 0x8A5C82, dark: 0xB793B0),
        ]
    )

    public static let tokyoNight = CodeScheme(
        id: "tokyoNight", title: "Tokyo Night", background: .init(light: 0xE9EAEF, dark: 0x1A1B26),
        foreground: .init(light: 0x3760BF, dark: 0xC0CAF5),
        gutter: .init(light: 0x5B6597, dark: 0x7B83AC), caret: .init(light: 0x2E7DE9, dark: 0x7AA2F7),
        selection: .init(light: 0xD6D9E4, dark: 0x292E42),
        diffAdd: .init(light: 0x587539, dark: 0x9ECE6A), diffDelete: .init(light: 0xF52A65, dark: 0xF7768E),
        tokens: [
            .keyword: .init(light: 0x8635EE, dark: 0xBB9AF7),
            .type: .init(light: 0x006F94, dark: 0x2AC3DE),
            .string: .init(light: 0x547036, dark: 0x9ECE6A),
            .number: .init(light: 0x9F5300, dark: 0xFF9E64),
            .comment: .init(light: 0x5B6597, dark: 0x7B83AC),
            .function: .init(light: 0x1664CE, dark: 0x7AA2F7),
            .variable: .init(light: 0x3760BF, dark: 0xC0CAF5),
            .attribute: .init(light: 0x806239, dark: 0xE0AF68),
            .`operator`: .init(light: 0x006A83, dark: 0x89DDFF),
            .punctuation: .init(light: 0x006A83, dark: 0x89DDFF),
            .regex: .init(light: 0x547036, dark: 0x9ECE6A),
            .constant: .init(light: 0x9F5300, dark: 0xFF9E64),
        ]
    )

    public static let gruvbox = CodeScheme(
        id: "gruvbox", title: "Gruvbox", background: .init(light: 0xFBF1C7, dark: 0x282828),
        foreground: .init(light: 0x3C3836, dark: 0xEBDBB2),
        gutter: .init(light: 0x76695D, dark: 0x9C8E81), caret: .init(light: 0xAF3A03, dark: 0xFABD2F),
        selection: .init(light: 0xEBDBB2, dark: 0x3C3836),
        diffAdd: .init(light: 0x79740E, dark: 0xB8BB26), diffDelete: .init(light: 0x9D0006, dark: 0xFB4934),
        tokens: [
            .keyword: .init(light: 0x9D0006, dark: 0xFB5946),
            .type: .init(light: 0x956110, dark: 0xFABD2F),
            .string: .init(light: 0x726D0D, dark: 0xB8BB26),
            .number: .init(light: 0x8F3F71, dark: 0xD3869B),
            .comment: .init(light: 0x76695D, dark: 0x9C8E81),
            .function: .init(light: 0x3F7654, dark: 0x8EC07C),
            .variable: .init(light: 0x076678, dark: 0x83A598),
            .attribute: .init(light: 0xAF3A03, dark: 0xFE8019),
            .`operator`: .init(light: 0x3F7654, dark: 0x8EC07C),
            .punctuation: .init(light: 0x3F7654, dark: 0x8EC07C),
            .regex: .init(light: 0x726D0D, dark: 0xB8BB26),
            .constant: .init(light: 0x8F3F71, dark: 0xD3869B),
        ]
    )

    public static let solarized = CodeScheme(
        id: "solarized", title: "Solarized", background: .init(light: 0xFDF6E3, dark: 0x002B36),
        foreground: .init(light: 0x586E75, dark: 0x93A1A1),
        gutter: .init(light: 0x637272, dark: 0x7A939B), caret: .init(light: 0x268BD2, dark: 0x268BD2),
        selection: .init(light: 0xEEE8D5, dark: 0x073642),
        diffAdd: .init(light: 0x859900, dark: 0x859900), diffDelete: .init(light: 0xDC322F, dark: 0xDC322F),
        tokens: [
            .keyword: .init(light: 0x667500, dark: 0x859900),
            .type: .init(light: 0x8C6A00, dark: 0xB58900),
            .string: .init(light: 0x207B74, dark: 0x2AA198),
            .number: .init(light: 0xCB2C79, dark: 0xDE66A0),
            .comment: .init(light: 0x637272, dark: 0x7A939B),
            .function: .init(light: 0x2074AF, dark: 0x3295DA),
            .variable: .init(light: 0x6166C0, dark: 0x858ACE),
            .attribute: .init(light: 0xC24815, dark: 0xE96833),
            .`operator`: .init(light: 0x667500, dark: 0x859900),
            .punctuation: .init(light: 0x667500, dark: 0x859900),
            .regex: .init(light: 0x207B74, dark: 0x2AA198),
            .constant: .init(light: 0xCB2C79, dark: 0xDE66A0),
        ]
    )

    public static let rosePine = CodeScheme(
        id: "rosePine", title: "Rosé Pine", background: .init(light: 0xFAF4ED, dark: 0x191724),
        foreground: .init(light: 0x575279, dark: 0xE0DEF4),
        gutter: .init(light: 0x716B80, dark: 0x84809B), caret: .init(light: 0x907AA9, dark: 0xC4A7E7),
        selection: .init(light: 0xF2E9E1, dark: 0x26233A),
        diffAdd: .init(light: 0x286983, dark: 0x31748F), diffDelete: .init(light: 0xB4637A, dark: 0xEB6F92),
        tokens: [
            .keyword: .init(light: 0x286983, dark: 0x3B8DAD),
            .type: .init(light: 0x44757E, dark: 0x9CCFD8),
            .string: .init(light: 0x9B6010, dark: 0xF6C177),
            .number: .init(light: 0xC2423C, dark: 0xEBBCBA),
            .comment: .init(light: 0x716B80, dark: 0x84809B),
            .function: .init(light: 0xC2423C, dark: 0xEBBCBA),
            .variable: .init(light: 0x7D6399, dark: 0xC4A7E7),
            .attribute: .init(light: 0x7D6399, dark: 0xC4A7E7),
            .`operator`: .init(light: 0x6F6B89, dark: 0x908CAA),
            .punctuation: .init(light: 0x6F6B89, dark: 0x908CAA),
            .regex: .init(light: 0x9B6010, dark: 0xF6C177),
            .constant: .init(light: 0xC2423C, dark: 0xEBBCBA),
        ]
    )

    public static let github = CodeScheme(
        id: "github", title: "GitHub", background: .init(light: 0xFFFFFF, dark: 0x0D1117),
        foreground: .init(light: 0x1F2328, dark: 0xE6EDF3),
        gutter: .init(light: 0x6D7680, dark: 0x8B949E), caret: .init(light: 0x0969DA, dark: 0x4493F8),
        selection: .init(light: 0xEAEEF2, dark: 0x21262D),
        diffAdd: .init(light: 0x1A7F37, dark: 0x3FB950), diffDelete: .init(light: 0xCF222E, dark: 0xF85149),
        tokens: [
            .keyword: .init(light: 0xCF222E, dark: 0xFF7B72),
            .type: .init(light: 0x953800, dark: 0xFFA657),
            .string: .init(light: 0x0A3069, dark: 0xA5D6FF),
            .number: .init(light: 0x0550AE, dark: 0x79C0FF),
            .comment: .init(light: 0x6D7680, dark: 0x8B949E),
            .function: .init(light: 0x8250DF, dark: 0xD2A8FF),
            .variable: .init(light: 0x953800, dark: 0xFFA657),
            .attribute: .init(light: 0x116329, dark: 0x7EE787),
            .`operator`: .init(light: 0x1F2328, dark: 0xE6EDF3),
            .punctuation: .init(light: 0x1F2328, dark: 0xE6EDF3),
            .regex: .init(light: 0x0A3069, dark: 0xA5D6FF),
            .constant: .init(light: 0x0550AE, dark: 0x79C0FF),
        ]
    )

    public static let one = CodeScheme(
        id: "one", title: "One", background: .init(light: 0xFAFAFA, dark: 0x282C34),
        foreground: .init(light: 0x383A42, dark: 0xABB2BF),
        gutter: .init(light: 0x707179, dark: 0x8E95A2), caret: .init(light: 0x4078F2, dark: 0x61AFEF),
        selection: .init(light: 0xE5E5E6, dark: 0x2C313A),
        diffAdd: .init(light: 0x50A14F, dark: 0x98C379), diffDelete: .init(light: 0xE45649, dark: 0xE06C75),
        tokens: [
            .keyword: .init(light: 0xA626A4, dark: 0xC678DD),
            .type: .init(light: 0x986801, dark: 0xE5C07B),
            .string: .init(light: 0x3F7F3E, dark: 0x98C379),
            .number: .init(light: 0x986801, dark: 0xD19A66),
            .comment: .init(light: 0x707179, dark: 0x8E95A2),
            .function: .init(light: 0x2867F0, dark: 0x61AFEF),
            .variable: .init(light: 0xD72F20, dark: 0xE2747D),
            .attribute: .init(light: 0x986801, dark: 0xD19A66),
            .`operator`: .init(light: 0x0179AD, dark: 0x56B6C2),
            .punctuation: .init(light: 0x0179AD, dark: 0x56B6C2),
            .regex: .init(light: 0x3F7F3E, dark: 0x98C379),
            .constant: .init(light: 0x986801, dark: 0xD19A66),
        ]
    )

    /// A constant is a number as far as these schemes are concerned, and saying so is cheaper than
    /// keeping two copies of one pair in step.
    private static func builtin(id: String, title: String, background: PaletteInk.Pair) -> Self {
        Self(
            id: id, title: title, background: background,
            foreground: .init(light: 0x202124, dark: 0xECECF1),
            gutter: PaletteInk.textTertiary,
            caret: PaletteInk.accent,
            selection: .init(light: 0xC8DCF4, dark: 0x42454B),
            diffAdd: PaletteInk.diffPositive,
            diffDelete: PaletteInk.negative,
            tokens: [
                .keyword: PaletteInk.synKeyword, .type: PaletteInk.synType,
                .string: PaletteInk.synString, .number: PaletteInk.synNumber,
                .comment: PaletteInk.synComment, .function: PaletteInk.synFunction,
                .variable: PaletteInk.synVariable, .attribute: PaletteInk.synAttribute,
                .operator: PaletteInk.synOperator, .punctuation: PaletteInk.synOperator,
                .regex: PaletteInk.synString, .constant: PaletteInk.synNumber,
            ]
        )
    }
}
