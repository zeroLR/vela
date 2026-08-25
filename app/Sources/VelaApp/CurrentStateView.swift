import SwiftUI
import VelaIPC

/// Answers "What am I working on?", "What is blocked?", and "What should I do next?"
/// from workspace files only, so an answer never comes from conversation memory.
struct CurrentStateView: View {
    let state: WorkspaceCurrentState?
    let entryLimit: Int
    let reload: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("Current work state").font(.caption.bold())
                if let state {
                    Text(freshness(state))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button("Refresh", action: reload)
                    .buttonStyle(.link)
                    .font(.caption)
            }

            if let state {
                answer("Working on", state.workingOn)
                answer("Blocked", state.blockers)
                answer("Next", state.nextUp)
                if state.openTaskCount > 0 {
                    Text("\(state.openTaskCount) task file(s) in tasks/")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                if state.truncated {
                    Text("Some entries were omitted by a size bound; open STATUS.md for all of them.")
                        .font(.caption)
                        .foregroundStyle(.orange)
                }
            } else {
                Text("Open a workspace to answer current-state questions.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private func answer(_ label: String, _ entries: [WorkspaceStateEntry]) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(label)
                .font(.caption.bold())
                .frame(width: 76, alignment: .leading)
            if entries.isEmpty {
                Text("Nothing recorded")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(Array(entries.prefix(entryLimit).enumerated()), id: \.offset) { _, entry in
                        HStack(alignment: .firstTextBaseline, spacing: 4) {
                            Text(entry.text)
                                .font(.caption)
                                .fixedSize(horizontal: false, vertical: true)
                            if entry.captureID != nil {
                                Text("captured")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                    if entries.count > entryLimit {
                        Text("+\(entries.count - entryLimit) more in STATUS.md")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
    }

    /// Stale state is misleading state, so the answer always carries its own age.
    private func freshness(_ state: WorkspaceCurrentState) -> String {
        guard state.statusUpdatedAtMilliseconds > 0 else { return "· STATUS.md age unknown" }
        let updated = Date(
            timeIntervalSince1970: Double(state.statusUpdatedAtMilliseconds) / 1000
        )
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        return "· STATUS.md \(formatter.localizedString(for: updated, relativeTo: Date()))"
    }
}
