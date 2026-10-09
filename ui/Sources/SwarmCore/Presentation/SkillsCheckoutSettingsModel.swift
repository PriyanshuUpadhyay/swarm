import Foundation
import Observation

@MainActor
public protocol SkillsCheckoutSettingsSource {
    var checkoutPath: String? { get }
    var loadError: String? { get }
    func setCheckout(_ path: String?) async throws -> String?
}

@MainActor @Observable
public final class SkillsCheckoutSettingsModel {
    public var path: String
    public private(set) var savedPath: String?
    public private(set) var error: String?
    public private(set) var saving = false
    @ObservationIgnored private let source: any SkillsCheckoutSettingsSource

    public init(source: any SkillsCheckoutSettingsSource) {
        self.source = source
        savedPath = source.checkoutPath
        path = source.checkoutPath ?? ""
        error = source.loadError
    }

    public var canSave: Bool { !saving && source.loadError == nil && !path.isEmpty }
    public var canClear: Bool { !saving && source.loadError == nil && savedPath != nil }

    public func refresh() {
        if path == savedPath ?? "" { path = source.checkoutPath ?? "" }
        savedPath = source.checkoutPath
        if let loadError = source.loadError { error = loadError }
    }

    public func save() async {
        guard canSave else { return }
        await update(path)
    }

    public func clear() async {
        guard canClear else { return }
        await update(nil)
    }

    private func update(_ requestedPath: String?) async {
        saving = true
        defer { saving = false }
        do {
            let canonicalPath = try await source.setCheckout(requestedPath)
            savedPath = canonicalPath
            path = canonicalPath ?? ""
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}
