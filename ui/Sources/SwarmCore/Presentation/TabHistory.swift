import Foundation

public enum TabHistory {
    /// History runs oldest to newest. Keep the last visit to each open chat.
    public static func flip(history: [String], open: [String], current: String?, step: Int) -> String? {
        let openKeys = Set(open)
        var seen = Set<String>()
        let recent = history.reversed().filter { openKeys.contains($0) && seen.insert($0).inserted }.reversed()
        let keys = Array(recent)
        guard !keys.isEmpty else { return nil }
        guard let current, let index = keys.firstIndex(of: current) else {
            return step < 0 ? keys.last : keys.first
        }
        return keys[(index + step % keys.count + keys.count) % keys.count]
    }
}
