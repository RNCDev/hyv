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

            Divider()

            // Transcript preview
            Text("Transcript")
                .font(.subheadline)
                .foregroundColor(.secondary)

            if appState.transcriptLines.isEmpty {
                Text(emptyStateText)
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .frame(maxWidth: .infinity, alignment: .center)
                    .padding(.vertical, 8)
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                        VStack(alignment: .leading, spacing: 4) {
                            ForEach(Array(appState.transcriptLines.suffix(10).enumerated()), id: \.offset) { index, line in
                                Text(line)
                                    .font(.caption)
                                    .textSelection(.enabled)
                                    .id(index)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .frame(maxHeight: 150)
                    .onChange(of: appState.transcriptLines.count) {
                        withAnimation {
                            proxy.scrollTo(appState.transcriptLines.suffix(10).count - 1, anchor: .bottom)
                        }
                    }
                }
            }

            Divider()

            // Quit
            Button("Quit Hyv") {
                NSApplication.shared.terminate(nil)
            }
            .frame(maxWidth: .infinity, alignment: .trailing)
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

    private var emptyStateText: String {
        switch appState.status {
        case .recording: return "Recording... transcript will appear after processing."
        case .processing: return "Processing audio..."
        default: return "No transcript yet."
        }
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
