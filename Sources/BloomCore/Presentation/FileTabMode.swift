import Foundation

/// What a file tab shows: the rendered document, the source read only, or the source in an editor.
///
/// Stored as the two choices it is made of rather than as one of the three values, because they
/// are asked separately. ⌘E toggles editing and nothing else, so leaving the editor goes back to
/// whichever of preview and source the reader had before it, and a file that has no preview
/// ignores the preference it would otherwise carry.
public enum FileTabMode: Sendable, Equatable, CaseIterable {
    case preview
    case source
    case edit

    /// A file without a preview keeps the old words, View and Edit, since "Source" only means
    /// something beside a rendered alternative.
    public func title(hasPreview: Bool) -> String {
        switch self {
        case .preview: "Preview"
        case .source: hasPreview ? "Source" : "View"
        case .edit: "Edit"
        }
    }

    public static func choices(hasPreview: Bool, canEdit: Bool) -> [Self] {
        (hasPreview ? [.preview] : []) + [.source] + (canEdit ? [.edit] : [])
    }

    public static func current(prefersEditing: Bool, prefersPreview: Bool, hasPreview: Bool, canEdit: Bool) -> Self {
        if prefersEditing && canEdit { return .edit }
        return hasPreview && prefersPreview ? .preview : .source
    }

    /// The two preferences after picking this mode. Picking Edit leaves the preview preference
    /// alone, which is what brings a reader back to the preview when they stop editing.
    public func preferences(prefersPreview: Bool) -> (prefersEditing: Bool, prefersPreview: Bool) {
        switch self {
        case .preview: (false, true)
        case .source: (false, false)
        case .edit: (true, prefersPreview)
        }
    }
}
