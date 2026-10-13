import Foundation

/// This actor runs the synchronous document and checkout APIs away from the UI thread.
public actor SkillsFileSource: SkillsPageSource {
    private let root: URL?
    private let protectedRoots: [URL]

    public init(root: URL? = SwarmHome.dataFolder?.appendingPathComponent("skills"),
                protectedRoots: [URL] = SkillCheckout.defaultProtectedRoots) {
        self.root = root
        self.protectedRoots = protectedRoots
    }

    public func inventory() async throws -> SkillInventory {
        let root = try bundleRoot()
        return try SkillInventory(manifest: Data(contentsOf: root.appendingPathComponent("manifest.json")), root: root)
    }

    public func load(key: String, checkoutPath: String?, inventory: SkillInventory) async throws -> SkillDocument {
        guard inventory.entries.contains(where: { $0.key == key }) else {
            throw SkillSaveError.path(path: key, reason: "The skill is not in the bundled inventory.")
        }
        let isKit = key.hasPrefix("kit/skills/") && key.split(separator: "/").count == 4
        var state = SkillDocumentState.bundledReadOnly(reason: isKit ? "Set a swarm checkout to edit this skill." : "Only kit skills can be edited in the checkout.")
        if isKit, let checkoutPath {
            do { return try SkillCheckout(path: checkoutPath, inventory: inventory, protectedRoots: protectedRoots).load(key: key) }
            catch { state = .missingCheckoutSource(reason: error.localizedDescription) }
        }
        let root = try bundleRoot().resolvingSymlinksInPath().standardizedFileURL
        let target = root.appendingPathComponent(key).resolvingSymlinksInPath().standardizedFileURL
        guard target.path.hasPrefix(root.path + "/") else {
            throw SkillSaveError.path(path: target.path, reason: "The bundled skill escapes its source folder.")
        }
        return SkillDocument.parse(data: try Data(contentsOf: target), key: key, sourceURL: target, state: state)
    }

    public func save(document: SkillDocument, candidate: Data, checkoutPath: String, inventory: SkillInventory) async throws -> SkillDocument {
        let checkout = try SkillCheckout(path: checkoutPath, inventory: inventory, protectedRoots: protectedRoots)
        let target = try checkout.targetURL(key: document.key)
        guard target == document.sourceURL else {
            throw SkillSaveError.path(path: target.path, reason: "The draft belongs to a different source file. Reload before saving.")
        }
        _ = try checkout.save(key: document.key, candidate: candidate, expectedRevision: document.revision)
        // Rebuild spans from the bytes just saved. A later read failure cannot turn a completed
        // write into a reported failed save with the old revision.
        return SkillDocument.parse(data: candidate, key: document.key, sourceURL: target)
    }

    private func bundleRoot() throws -> URL {
        guard let root else { throw SkillSaveError.path(path: "SWARM_HOME", reason: "The build's skills home is unavailable.") }
        return root
    }
}
