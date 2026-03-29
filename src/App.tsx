import { useAppState } from "./hooks/useAppState";
import { ModelSelector } from "./components/ModelSelector";
import { RecordingControls } from "./components/RecordingControls";
import { StatusIndicator } from "./components/StatusIndicator";
import { TranscriptList } from "./components/TranscriptList";

declare const __APP_VERSION__: string;

function App() {
  const { status, recordingTime, formatTime } = useAppState();

  return (
    <div style={styles.container}>
      <div style={styles.header}>
        <span style={styles.logo}>Hyv</span>
        <span style={styles.version}>v{__APP_VERSION__}</span>
      </div>

      <StatusIndicator status={status} recordingTime={recordingTime} formatTime={formatTime} />
      <ModelSelector disabled={status.type !== "Idle"} />
      <RecordingControls status={status} recordingTime={recordingTime} formatTime={formatTime} />

      <div style={styles.divider} />

      <TranscriptList />

      <div style={styles.footer}>
        <span style={styles.footerText}>Local Whisper transcription</span>
      </div>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  container: {
    padding: "16px",
    display: "flex",
    flexDirection: "column",
    gap: "12px",
    height: "100vh",
  },
  header: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
  },
  logo: {
    fontSize: "18px",
    fontWeight: 700,
    letterSpacing: "-0.5px",
  },
  version: {
    fontSize: "11px",
    color: "#888",
  },
  divider: {
    height: "1px",
    background: "#333",
  },
  footer: {
    marginTop: "auto",
    textAlign: "center" as const,
  },
  footerText: {
    fontSize: "10px",
    color: "#666",
  },
};

export default App;
