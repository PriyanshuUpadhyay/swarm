import SwiftUI

/// A spinner that shows only if the wait passes 300 ms, so quick work never flashes one.
/// Main-path content shows its last known state instead of this.
struct DelayedProgress: View {
    let label: String?
    @State private var visible = false

    init(_ label: String? = nil) { self.label = label }

    var body: some View {
        Group {
            if visible {
                if let label { ProgressView(label) } else { ProgressView() }
            }
        }
        .controlSize(.small)
        .task {
            try? await Task.sleep(for: .milliseconds(300))
            visible = true
        }
    }
}
