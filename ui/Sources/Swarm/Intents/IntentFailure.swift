import AppIntents
import Foundation

/// What an intent says when it cannot do the thing. Shortcuts shows this sentence verbatim, so
/// each one names what was being attempted rather than reporting a failure in the abstract.
enum IntentFailure: Error, CustomLocalizedStringResourceConvertible {
    case unknownProject
    case unknownWorkspace
    case appNeverAppeared

    var localizedStringResource: LocalizedStringResource {
        switch self {
        case .unknownProject:
            "That project is no longer one of Swarm's. Add its folder in Swarm and try again."
        case .unknownWorkspace:
            "That workspace no longer exists in Swarm."
        case .appNeverAppeared:
            "Swarm did not finish opening, so there was nowhere to send the request."
        }
    }
}
