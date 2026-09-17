import SwiftUI

/// A disclosure group that opens from anywhere on its label row, not only from the chevron.
///
/// The system style on macOS toggles from the chevron alone, so "Advanced configuration" on the
/// Agents pane read as a row you click and then did nothing when you clicked its title. This keeps
/// the system look by handing the group straight back to `.automatic`, which is also what stops
/// the style from applying itself again inside its own body, and only widens what takes the click.
struct WholeRowDisclosureStyle: DisclosureGroupStyle {
    func makeBody(configuration: Configuration) -> some View {
        DisclosureGroup(isExpanded: configuration.$isExpanded) {
            configuration.content
        } label: {
            configuration.label
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
                .onTapGesture {
                    withAnimation { configuration.isExpanded.toggle() }
                }
        }
        .disclosureGroupStyle(.automatic)
    }
}

extension DisclosureGroupStyle where Self == WholeRowDisclosureStyle {
    static var wholeRow: WholeRowDisclosureStyle { WholeRowDisclosureStyle() }
}
