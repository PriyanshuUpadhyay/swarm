import Foundation

/// Tests use claimed temporary homes; the CLI owns the real marker.
func claimedChoicesFolder(_ folder: URL) throws -> URL {
    try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
    try Data("swarm\n".utf8).write(to: folder.appendingPathComponent("swarm-home"))
    return folder
}
