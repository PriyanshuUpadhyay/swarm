import Foundation

/// A pet that the owner installed for the Codex TUI, `$CODEX_HOME/pets/<id>/pet.json`. The format,
/// its defaults and the path rule follow codex-rs `tui/src/pets/model.rs`: one sprite sheet in a
/// grid of frames, and named animations as lists of frame indices.
public struct CodexPetManifest: Equatable, Sendable {
    public struct Frame: Equatable, Sendable {
        public let width: Int
        public let height: Int
        public let columns: Int
        public let rows: Int
    }

    /// Frame indices into the sheet, row by row, and their rate.
    public struct Animation: Equatable, Sendable {
        public let frames: [Int]
        public let fps: Double

        public init(frames: [Int], fps: Double) {
            self.frames = frames
            self.fps = fps
        }
    }

    public let name: String
    /// Relative to the pet's folder, and inside it.
    public let sheetPath: String
    public let frame: Frame
    public let animations: [MascotState: Animation]

    struct Invalid: Error, CustomStringConvertible {
        let description: String
    }

    private struct File: Decodable {
        struct FrameSpec: Decodable { let width: Int, height: Int, columns: Int, rows: Int }
        struct Spec: Decodable { let frames: [Int]?, fps: Double? }
        let id: String?
        let displayName: String?
        let spritesheetPath: String?
        let frame: FrameSpec?
        let animations: [String: Spec]?
    }

    /// Codex's own animation for each HUD state: its row and frame count by default, or the
    /// manifest's own frames. Codex has no sleep, so the pet sleeps on its first idle frame.
    private static let defaults: [(MascotState, String, Animation)] = [
        // Codex holds these six idle frames 6.6 s in all.
        (.idle, "idle", Animation(frames: Array(0...5), fps: 0.91)),
        (.working, "running", Animation(frames: Array(56...61), fps: 8)),
        (.waiting, "waiting", Animation(frames: Array(48...53), fps: 6.67)),
    ]

    public static func parse(_ data: Data, folder: String) throws -> CodexPetManifest {
        let file = try JSONDecoder().decode(File.self, from: data)
        let name = [file.displayName, file.id].compactMap { $0?.trimmingCharacters(in: .whitespaces) }
            .first { !$0.isEmpty } ?? folder
        let path = file.spritesheetPath?.trimmingCharacters(in: .whitespaces).nilIfEmpty ?? "spritesheet.webp"
        // Like Codex, a manifest reaches only files inside its own folder.
        guard !path.hasPrefix("/"), !path.hasPrefix("~"), !path.split(separator: "/").contains("..") else {
            throw Invalid(description: "spritesheet path \(path) leaves the pet folder")
        }
        let spec = file.frame ?? .init(width: 192, height: 208, columns: 8, rows: 9)
        guard (1...4096).contains(spec.width), (1...4096).contains(spec.height),
              (1...64).contains(spec.columns), (1...64).contains(spec.rows) else {
            throw Invalid(description: "bad frame size")
        }
        let count = spec.columns * spec.rows
        var animations: [MascotState: Animation] = [:]
        for (state, key, fallback) in defaults {
            if let custom = file.animations?[key] {
                guard let frames = custom.frames, !frames.isEmpty, frames.allSatisfy({ (0..<count).contains($0) }) else {
                    throw Invalid(description: "animation \(key) has no frames or one outside the sheet")
                }
                let fps = custom.fps.flatMap { $0.isFinite && $0 > 0 ? $0 : nil } ?? 8
                animations[state] = Animation(frames: frames, fps: min(fps, 8))
            } else if fallback.frames.allSatisfy({ $0 < count }) {
                animations[state] = fallback
            }
        }
        // A sheet too small for a default row falls back to the idle frames.
        let idle = animations[.idle] ?? Animation(frames: [0], fps: 1)
        animations[.idle] = idle
        for state in [MascotState.working, .waiting] where animations[state] == nil { animations[state] = idle }
        animations[.sleeping] = Animation(frames: [idle.frames[0]], fps: 1)
        return CodexPetManifest(name: name, sheetPath: path,
                                frame: Frame(width: spec.width, height: spec.height, columns: spec.columns, rows: spec.rows),
                                animations: animations)
    }
}

private extension String {
    var nilIfEmpty: String? { isEmpty ? nil : self }
}
