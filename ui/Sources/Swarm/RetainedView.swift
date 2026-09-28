import SwiftUI

extension View {
    /// Keep local view state without leaving hidden controls available to input or accessibility.
    func retainedVisibility(_ visible: Bool) -> some View {
        opacity(visible ? 1 : 0)
            .allowsHitTesting(visible)
            .accessibilityElement(children: .contain)
            .accessibilityHidden(!visible)
    }
}
