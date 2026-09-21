import AppKit

/// Where a background poll waits for somebody to come back.
///
/// A poll that only asks GitHub costs a `gh` subprocess per pass, and two of them ran through the
/// night for panels nobody could see: 445 processes an hour, 797ms each, against an app in the
/// background. Waiting here turns that into one pass when the app is in front and nothing at all
/// when it is not.
///
/// Deliberately not a timer or a flag somebody has to remember to set. A poll loop puts this at
/// the top of its `while`, and that is the whole of its participation.
@MainActor
enum AppActivity {
    /// How often the answer is asked for again while the app is behind something else.
    ///
    /// **Asked rather than waited for, and the difference is a race at launch.** Awaiting
    /// `didBecomeActive` means reading `isActive`, finding it false, and only then subscribing;
    /// an app that becomes active in between waits for the next time somebody switches away and
    /// back, which on a Mac left alone is never. This reads a `Bool` every two seconds instead,
    /// which beside one `gh` process is nothing at all.
    private static let recheck = Duration.seconds(2)

    /// Returns at once while the app is in front, and otherwise once it is in front again.
    ///
    /// Returns rather than throwing on cancellation, because the caller's `while !Task.isCancelled`
    /// is already the loop's one exit and a second one would be a second rule.
    static func waitUntilActive() async {
        while !Task.isCancelled, !NSApplication.shared.isActive {
            do { try await Task.sleep(for: recheck) } catch { return }
        }
    }
}
