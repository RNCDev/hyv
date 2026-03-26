import SwiftUI

struct MenuBarView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            // Status
            HStack {
                Image(systemName: appState.menuBarIcon)
                    .foregroundColor(statusColor)
                Text(appState.statusText)
                    .font(.headline)
                    .lineLimit(1)
            }
            .padding(.bottom, 4)

            Divider()

            // Controls
            HStack(spacing: 8) {
                if appState.status != .recording {
                    Button("Start Recording") {
                        // TODO: Implement in Phase 2
                    }
                    .disabled(appState.status == .error(""))
                    .buttonStyle(.borderedProminent)
                } else {
                    Button("Stop Recording") {
                        // TODO: Implement in Phase 2
                    }
                    .buttonStyle(.bordered)
                    .tint(.red)
                }
            }

            Divider()

            // Transcript preview
            Text("Transcript")
                .font(.subheadline)
                .foregroundColor(.secondary)

            if appState.transcriptLines.isEmpty {
                Text("No transcript yet.")
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .frame(maxWidth: .infinity, alignment: .center)
                    .padding(.vertical, 8)
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 4) {
                        ForEach(appState.transcriptLines.suffix(10), id: \.self) { line in
                            Text(line)
                                .font(.caption)
                                .textSelection(.enabled)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .frame(maxHeight: 150)
            }

            Divider()

            // Quit
            Button("Quit Hyv") {
                NSApplication.shared.terminate(nil)
            }
            .frame(maxWidth: .infinity, alignment: .trailing)
        }
        .padding()
        .frame(width: 300)
    }

    private var statusColor: Color {
        switch appState.status {
        case .idle: return .secondary
        case .meetingDetected: return .orange
        case .recording: return .green
        case .error: return .red
        }
    }
}
