import SwiftUI
import SwarmCore

/// Every preset as a small picture of the window it paints, so a theme is chosen by looking at it
/// rather than by its name.
struct ThemeGallery: View {
    @Binding var selection: ColourTheme

    private let columns = [GridItem(.adaptive(minimum: 132, maximum: 180), spacing: Metrics.gutter)]

    var body: some View {
        LazyVGrid(columns: columns, spacing: Metrics.gutter) {
            ForEach(ColourTheme.allCases) { theme in
                Button { selection = theme } label: {
                    ThemeSwatch(theme: theme, isSelected: theme == selection)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(theme.title)
                .accessibilityAddTraits(theme == selection ? [.isButton, .isSelected] : .isButton)
            }
        }
        .padding(.vertical, Metrics.spacingSmall)
    }
}

/// A sidebar, a page, one message of each kind and the accent, in the theme's own colours for the
/// appearance the window is in.
private struct ThemeSwatch: View {
    var theme: ColourTheme
    var isSelected: Bool

    var body: some View {
        let surfaces = theme.surfaces
        VStack(alignment: .leading, spacing: Metrics.spacing) {
            HStack(spacing: 0) {
                VStack(alignment: .leading, spacing: 3) {
                    ForEach(0..<3, id: \.self) { index in
                        Capsule()
                            .fill(index == 0 ? Palette.dynamic(surfaces.selected) : .clear)
                            .overlay(alignment: .leading) {
                                Capsule().fill(Palette.textSecondary.opacity(0.5))
                                    .frame(width: 22, height: 3)
                                    .padding(.leading, 4)
                            }
                            .frame(height: 8)
                    }
                    Spacer(minLength: 0)
                }
                .padding(4)
                .frame(width: 44)
                .background(Palette.dynamic(surfaces.sunken))

                Rectangle().fill(Palette.dynamic(surfaces.border)).frame(width: 1)

                VStack(alignment: .trailing, spacing: 4) {
                    RoundedRectangle(cornerRadius: 3)
                        .fill(Palette.dynamic(surfaces.bubble ?? PaletteInk.accentFill))
                        .frame(width: 44, height: 10)
                    VStack(alignment: .leading, spacing: 3) {
                        Capsule().fill(Palette.textPrimary.opacity(0.7)).frame(width: 58, height: 3)
                        Capsule().fill(Palette.textPrimary.opacity(0.7)).frame(width: 40, height: 3)
                        Capsule().fill(Palette.dynamic(surfaces.readable(surfaces.accent ?? PaletteInk.accent)))
                            .frame(width: 24, height: 3)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    Spacer(minLength: 0)
                    RoundedRectangle(cornerRadius: 3)
                        .fill(Palette.dynamic(surfaces.raised))
                        .overlay { RoundedRectangle(cornerRadius: 3).strokeBorder(Palette.dynamic(surfaces.border)) }
                        .frame(height: 10)
                }
                .padding(6)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Palette.dynamic(surfaces.surface))
            }
            .frame(height: 72)
            .clipShape(RoundedRectangle(cornerRadius: Metrics.corner, style: .continuous))
            .overlay {
                RoundedRectangle(cornerRadius: Metrics.corner, style: .continuous)
                    .strokeBorder(
                        isSelected ? Palette.controlAccent : Palette.border,
                        lineWidth: isSelected ? 2 : Metrics.hairline
                    )
            }

            Text(theme.title)
                .font(Typo.label)
                .foregroundStyle(isSelected ? Palette.textPrimary : Palette.textSecondary)
        }
        .contentShape(Rectangle())
    }
}

/// One colour of the selected theme, changed for the appearance the window is in.
///
/// The other appearance keeps the preset's value, so a person who changes the dark background
/// does not also repaint the light one.
struct ThemeColourRow: View {
    var role: ThemeColourRole
    @Bindable var preference: ColourThemePreference
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        HStack {
            ColorPicker(role.title, selection: colour, supportsOpacity: false)
            if isChangedHere {
                Button("Reset") { reset() }
                    .buttonStyle(.borderless)
                    .font(Typo.caption)
            }
        }
    }

    private var isDark: Bool { colorScheme == .dark }

    private var preset: PaletteInk.Pair { role.value(in: preference.choice.surfaces) }

    /// Whether this appearance's half differs from the preset. The other half is not this row's.
    private var isChangedHere: Bool {
        preference.isChanged(role) && preference.colour(role).member(dark: isDark) != preset.member(dark: isDark)
    }

    /// Puts back this appearance's half only, and drops the override once both halves match.
    private func reset() {
        let current = preference.colour(role)
        let restored = isDark
            ? PaletteInk.Pair(light: current.light, dark: preset.dark)
            : PaletteInk.Pair(light: preset.light, dark: current.dark)
        preference.setColour(role, to: restored == preset ? nil : restored)
    }

    private var colour: Binding<Color> {
        Binding(
            get: {
                Color(nsColor: NSColor(rgb: preference.colour(role).member(dark: isDark)))
            },
            set: { picked in
                guard let hex = picked.hexString, let value = UInt32(hex, radix: 16) else { return }
                let current = preference.colour(role)
                preference.setColour(role, to: isDark
                    ? PaletteInk.Pair(light: current.light, dark: value)
                    : PaletteInk.Pair(light: value, dark: current.dark))
            }
        )
    }
}
