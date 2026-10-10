import Foundation

public enum AppLockFailure {
    public static func message(for error: Error, path: String) -> String {
        if let error = error as? POSIXError, error.code == .EWOULDBLOCK {
            return "Swarm could not take its app lock at \(path). Another Swarm may hold this folder, so this run shows no app notices."
        }
        return ErrorText.sentence("Swarm could not start app notices: \(error.localizedDescription)")
    }
}
