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
                    .lineLimit(2)
            }
            .padding(.bottom, 4)

            // Warnings
            if !AppConfig.shared.hasValidApiKey {
                Label("No Cohere API key", systemImage: "exclamationmark.triangle.fill")
                    .font(.caption)
                    .foregroundColor(.orange)
            }
            if !AppConfig.shared.hasValidHFToken {
                Label("No HuggingFace token", systemImage: "exclamationmark.triangle.fill")
                    .font(.caption)
                    .foregroundColor(.orange)
            }

            Divider()

            // Controls
            HStack(spacing: 8) {
                if appState.status == .recording {
                    Button("Stop Recording") {
                        appState.stopRecording()
                    }
                    .buttonStyle(.bordered)
                    .tint(.red)

                    if let start = appState.recordingStartTime {
                        Text(start, style: .timer)
                            .font(.caption)
                            .foregroundColor(.secondary)
                    }
                } else if case .processing = appState.status {
                    ProgressView()
                        .controlSize(.small)
                    Text("Processing...")
                        .font(.caption)
                        .foregroundColor(.secondary)
                } else {
                    Button("Start Recording") {
                        appState.startRecording()
                    }
                    .disabled(!canStartRecording)
                    .buttonStyle(.borderedProminent)
                }
            }

            // Transcript file path + open button
            if let path = appState.currentTranscriptPath {
                HStack {
                    Text(URL(fileURLWithPath: path).lastPathComponent)
                        .font(.caption2)
                        .foregroundColor(.secondary)
                        .lineLimit(1)

                    Spacer()

                    if canOpenTranscript {
                        Button("Open") {
                            appState.openTranscript()
                        }
                        .font(.caption)
                        .buttonStyle(.borderless)
                    }
                }
            }

            // Recent transcripts
            if !recentTranscripts.isEmpty {
                Divider()

                Text("Recent Transcripts")
                    .font(.subheadline)
                    .foregroundColor(.secondary)

                ForEach(recentTranscripts, id: \.path) { url in
                    HStack {
                        Image(systemName: "doc.text")
                            .font(.caption2)
                            .foregroundColor(.secondary)
                        Text(url.lastPathComponent)
                            .font(.caption)
                            .lineLimit(1)
                        Spacer()
                        Button("Open") {
                            NSWorkspace.shared.open(url)
                        }
                        .font(.caption)
                        .buttonStyle(.borderless)
                    }
                }
            }

            Divider()

            // Version + Quit
            HStack {
                Text("v\(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0.0")")
                    .font(.caption2)
                    .foregroundColor(.secondary)

                Spacer()

                Button("Quit Hyv") {
                    NSApplication.shared.terminate(nil)
                }
            }
        }
        .padding()
        .frame(width: 320)
    }

    private var canStartRecording: Bool {
        switch appState.status {
        case .idle, .meetingDetected: return true
        default: return false
        }
    }

    private var canOpenTranscript: Bool {
        switch appState.status {
        case .idle, .meetingDetected, .error: return true
        default: return false
        }
    }

    private var recentTranscripts: [URL] {
        let desktop = FileManager.default.urls(for: .desktopDirectory, in: .userDomainMask).first!
        let files = (try? FileManager.default.contentsOfDirectory(
            at: desktop,
            includingPropertiesForKeys: [.contentModificationDateKey],
            options: .skipsHiddenFiles
        )) ?? []

        return files
            .filter { $0.lastPathComponent.hasPrefix("Hyv_Transcript_") && $0.pathExtension == "txt" }
            .sorted {
                let d1 = (try? $0.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
                let d2 = (try? $1.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
                return d1 > d2
            }
            .prefix(5)
            .map { $0 }
    }

    private var statusColor: Color {
        switch appState.status {
        case .idle: return .secondary
        case .meetingDetected: return .orange
        case .recording: return .green
        case .processing: return .blue
        case .error: return .red
        }
    }
}
