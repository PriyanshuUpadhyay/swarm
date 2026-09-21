import Foundation
import SwarmCore

/// Whether a stored row is worth a line at all.
///
/// Rate-limit events already feed the menu bar's quota panel, so repeating them between messages
/// adds noise without adding information. Hook payloads can run to hundreds of kilobytes and are
/// skipped by sniffing the first bytes rather than decoding data that will not be shown.
///
/// Stream deltas and the CLI's own bookkeeping are known records, not unknown ones: a delta is
/// repeated whole by the message it belongs to, and a changed command list or task list says
/// nothing to the reader. `OpaqueRecord` stays the answer for a record nothing knows.
enum TranscriptNoise {
    private static let probeLength = 256
    private static let markers = [
        "\"hook_", "\"type\":\"stream_event\"",
        "\"subtype\":\"commands_changed\"", "\"subtype\":\"background_tasks_changed\"",
    ].map { Data($0.utf8) }

    static func isHidden(_ row: TranscriptRow) -> Bool {
        if row.kind == .notice { return true }
        guard row.kind == .system else { return false }
        let probe = row.payload.prefix(probeLength)
        return markers.contains { probe.range(of: $0) != nil }
    }
}
