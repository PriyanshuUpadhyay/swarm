import Foundation

/// Finds a provider log by its chair id or session start time.
public enum ChairLogDiscovery {
    public static func path(
        provider: String, chairID: String?, cwd: String, createdAt: Int,
        homes: [URL]
    ) -> URL? {
        let manager = FileManager.default
        var seen: Set<String> = []
        let roots = homes.map(\.standardizedFileURL).filter { seen.insert($0.path).inserted }

        if let chairID {
            for home in roots {
                if let match = candidates(provider: provider, home: home, manager: manager)
                    .sorted(by: { $0.path < $1.path })
                    .first(where: { candidate in
                        switch provider {
                        case "claude": candidate.lastPathComponent == "\(chairID).jsonl"
                        case "codex": candidate.lastPathComponent.hasSuffix("-\(chairID).jsonl")
                        default: false
                        }
                    }) {
                    return match
                }
            }
            return nil
        }

        var closest: (path: URL, distance: TimeInterval)?

        for home in roots {
            for candidate in candidates(provider: provider, home: home, manager: manager) {
                guard let record = firstRecord(in: candidate),
                      record.cwd == URL(fileURLWithPath: cwd).standardizedFileURL.path else { continue }
                let delay = record.date.timeIntervalSince1970 - TimeInterval(createdAt)
                let distance = abs(delay)
                guard distance <= 10 * 60 else { continue }
                if closest == nil || distance < closest!.distance
                    || (distance == closest!.distance && candidate.path > closest!.path.path) {
                    closest = (candidate, distance)
                }
            }
        }
        return closest?.path
    }

    static func homes(provider: String, accountHomes: [String], userHome: URL) -> [URL] {
        accountHomes.map(URL.init(fileURLWithPath:)) + [
            userHome.appendingPathComponent(provider == "codex" ? ".codex" : ".claude")
        ]
    }

    private static func candidates(
        provider: String, home: URL, manager: FileManager
    ) -> [URL] {
        switch provider {
        case "claude":
            let projects = home.appendingPathComponent("projects", isDirectory: true)
            let directories = (try? manager.contentsOfDirectory(
                at: projects, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles]
            )) ?? []
            return directories.flatMap { directory in
                (try? manager.contentsOfDirectory(
                    at: directory, includingPropertiesForKeys: nil,
                    options: [.skipsHiddenFiles]
                )) ?? []
            }.filter { $0.pathExtension == "jsonl" }
        case "codex":
            let sessions = home.appendingPathComponent("sessions", isDirectory: true)
            let files = manager.enumerator(
                at: sessions, includingPropertiesForKeys: [.isRegularFileKey],
                options: [.skipsHiddenFiles, .skipsPackageDescendants]
            )
            return (files?.allObjects as? [URL] ?? []).filter {
                $0.lastPathComponent.hasPrefix("rollout-") && $0.pathExtension == "jsonl"
            }
        default:
            return []
        }
    }

    private static func firstRecord(in path: URL) -> (cwd: String, date: Date)? {
        guard let file = try? FileHandle(forReadingFrom: path) else { return nil }
        defer { try? file.close() }
        guard let data = try? file.read(upToCount: 65_536) else { return nil }
        let formatter = ISO8601DateFormatter()
        for line in data.split(separator: 10) {
            guard let value = try? JSONSerialization.jsonObject(with: Data(line)) as? [String: Any],
                  let timestamp = value["timestamp"] as? String,
                  let cwd = ((value["payload"] as? [String: Any])?["cwd"] as? String)
                    ?? (value["cwd"] as? String) else { continue }
            formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
            let date = formatter.date(from: timestamp)
            formatter.formatOptions = [.withInternetDateTime]
            guard let date = date ?? formatter.date(from: timestamp) else { continue }
            return (URL(fileURLWithPath: cwd).standardizedFileURL.path, date)
        }
        return nil
    }
}
