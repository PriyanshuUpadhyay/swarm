import Foundation
import Darwin
import Testing
@testable import SwarmCore

@Suite("App lock failure")
struct AppLockFailureTests {
    @Test("Only POSIX lock contention names another Swarm")
    func contention() {
        #expect(AppLockFailure.message(for: POSIXError(.EWOULDBLOCK), path: "/fixture/app.lock")
            == "Swarm could not take its app lock at /fixture/app.lock. Another Swarm may hold this folder, so this run shows no app notices.")
    }

    @Test("Initialization, missing home, permissions, and unrelated errors keep their own text")
    func otherFailures() {
        let failures: [Error] = [
            SwarmProfileError.failed("swarm init failed"),
            SwarmProfileError.unavailable("swarm is missing"),
            OwnerChoicesError.emptyHome,
            POSIXError(.EACCES),
            POSIXError(.EIO),
            NSError(domain: "UnrelatedFailure", code: Int(EWOULDBLOCK), userInfo: [NSLocalizedDescriptionKey: "An unrelated error"]),
        ]
        for failure in failures {
            #expect(AppLockFailure.message(for: failure, path: "/fixture/app.lock")
                == "Swarm could not start app notices: \(failure.localizedDescription).")
        }
    }
}
