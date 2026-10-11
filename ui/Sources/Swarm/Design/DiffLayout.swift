import SwiftUI

private struct SplitDiffKey: EnvironmentKey {
    static var defaultValue: Binding<Bool> { .constant(false) }
}

extension EnvironmentValues {
    var splitDiff: Binding<Bool> {
        get { self[SplitDiffKey.self] }
        set { self[SplitDiffKey.self] = newValue }
    }
}
