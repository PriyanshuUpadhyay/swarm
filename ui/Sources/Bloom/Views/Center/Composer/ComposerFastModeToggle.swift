import SwiftUI
import BloomCore

/// Fast mode, as a bolt beside the settings button, amber while it is on.
///
/// It was a switch at the foot of the settings panel, which put the one setting people flip per
/// turn behind a click and a popover. It is also the one setting a preset does not hold: it
/// combines with whichever preset is in force, so it sits outside the menu that lists them.
///
/// Whether it is drawn at all is `ComposerControls.fastModeAvailability`: always on Claude Code,
/// on Codex once the server says the model has a fast tier, and never on a backend that sends
/// nothing for it.
struct ComposerFastModeToggle: View {
    var controls: ComposerControls
    var codexSpeed: CodexSpeed?
    var codexSpeedFailed: Bool
    var onChange: @MainActor (Bool) -> Void

    var body: some View {
        let availability = controls.fastModeAvailability(
            codexSpeed: codexSpeed, codexSpeedFailed: codexSpeedFailed
        )
        if availability != .unavailable {
            let isOn = controls.isFast(codexSpeed: codexSpeed)
            Button {
                onChange(!isOn)
            } label: {
                ComposerControlLabel(
                    systemImage: isOn ? "bolt.fill" : "bolt",
                    text: nil,
                    tint: isOn ? Palette.warning : Palette.textSecondary
                )
                // An amber wash rather than the grey selection fill the other controls use, so
                // "on" reads as the same amber as the bolt rather than as a pressed button.
                .background {
                    if isOn {
                        RoundedRectangle(cornerRadius: Metrics.cornerSmall)
                            .fill(Palette.warning.opacity(0.16))
                    }
                }
            }
            .buttonStyle(.plain)
            .disabled(availability == .loading)
            .accessibilityLabel("Fast mode")
            .accessibilityAddTraits(.isToggle)
            .accessibilityValue(isOn ? "On" : "Off")
            .help(help(availability: availability))
        }
    }

    private func help(availability: FastModeAvailability) -> String {
        if availability == .loading { return "Checking whether this model has fast mode" }
        return controls.agentKind == .codex
            ? "Fast mode: faster replies use more of your Codex allowance"
            : "Fast mode: disable thinking for faster replies"
    }
}
