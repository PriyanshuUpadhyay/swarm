public enum GitInitPolicy {
    public static func shouldAsk(path: String, choices: OwnerChoices) -> Bool {
        !choices.plainFolders.contains(path)
    }
}
