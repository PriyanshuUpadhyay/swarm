import SwiftUI
import SwarmCore

struct HomeView: View {
    let recentWork: [RecentWorkRow]
    let firstRunSteps: [FirstRunStep]
    let sessionsError: String?
    let startChatDisabledReason: String?
    let onOpenProject: () -> Void
    let onCreateProject: () -> Void
    let onRunSetup: () -> Void
    let onStartChat: () -> Void
    let onSelectChat: (String) -> Void

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: DesignTokens.Spacing.xl) {
                HStack {
                    Button("Import Project…", action: onOpenProject)
                    Button("Create Project…", action: onCreateProject)
                }
                if let sessionsError {
                    Text("Chats unavailable: \(sessionsError)").foregroundStyle(.red)
                        .textSelection(.enabled)
                }
                if !firstRunSteps.isEmpty {
                    VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
                        Text("Get started").font(.title2.bold()).accessibilityAddTraits(.isHeader)
                        ForEach(firstRunSteps) { step in
                            HStack(spacing: DesignTokens.Spacing.m) {
                                Image(systemName: step.done ? "checkmark.circle.fill" : "circle")
                                    .foregroundStyle(step.done ? .green : .secondary)
                                    .accessibilityLabel(step.done ? "Done" : "To do")
                                stepButton(step)
                            }
                        }
                    }
                }
                VStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
                    Text("Recent work").font(.largeTitle.bold()).accessibilityAddTraits(.isHeader)
                    if recentWork.isEmpty {
                        Text("Your recent chats appear here.").foregroundStyle(.secondary)
                    } else {
                        ForEach(recentWork) { row in
                            Button { onSelectChat(row.id.rawValue) } label: {
                                HStack(spacing: DesignTokens.Spacing.m) {
                                    stateGlyph(row)
                                        .frame(width: DesignTokens.Size.glyphSlot)
                                    VStack(alignment: .leading, spacing: DesignTokens.Spacing.xxs) {
                                        Text(row.title).fontWeight(.medium).lineLimit(1)
                                        Text(row.workspace).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                                    }
                                    Spacer()
                                    Text(row.age).font(.caption).foregroundStyle(.secondary)
                                }
                                .frame(maxWidth: .infinity, minHeight: DesignTokens.Size.row, alignment: .leading)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .help(row.workspacePath)
                            Divider()
                        }
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(DesignTokens.Spacing.xl)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    @ViewBuilder
    private func stepButton(_ step: FirstRunStep) -> some View {
        switch step.id {
        case .importProject:
            Button(step.title, action: onOpenProject)
        case .runSetup:
            Button(step.title, action: onRunSetup)
        case .startChat:
            Button(step.title, action: onStartChat)
                .disabled(startChatDisabledReason != nil)
                .help(startChatDisabledReason ?? "Start a chat")
            if let startChatDisabledReason {
                Text(startChatDisabledReason).font(.caption).foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private func stateGlyph(_ row: RecentWorkRow) -> some View {
        if let status = row.status {
            StatusGlyph(status: status)
        } else {
            switch row.state {
            case .live:
                Image(systemName: "circle.fill").foregroundStyle(.green).accessibilityLabel("Live")
            case .ended:
                StatusGlyph(status: .ended)
            case .noChair:
                Image(systemName: "questionmark.circle").foregroundStyle(.secondary).accessibilityLabel("No chair")
            }
        }
    }
}
