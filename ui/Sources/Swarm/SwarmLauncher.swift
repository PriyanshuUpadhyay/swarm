import Foundation

/// Development probes must exit before SwiftUI constructs AppModel or opens a database.
/// Normal launches still enter through SwiftUI's App.main implementation.
@main
enum SwarmLauncher {
    @MainActor
    static func main() async {
        Log.launchStep("main")
        #if DEBUG
        if WelcomeLayoutProbe.isRequested { WelcomeLayoutProbe.runAndExit() }
        if ReviewRunProbe.isRequested { ReviewRunProbe.runAndExit() }
        #endif
        SwarmApp.main()
    }
}
