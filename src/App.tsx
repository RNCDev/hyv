import { useState, useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useAppState } from "./hooks/useAppState";
import { StatusIndicator } from "./components/StatusIndicator";
import { RecordingControls } from "./components/RecordingControls";
import { TranscriptList } from "./components/TranscriptList";

function App() {
  const { status, recordingTime, formatTime } = useAppState();
  const [version, setVersion] = useState("");

  useEffect(() => {
    getVersion().then(setVersion);
  }, []);

  return (
    <div style={styles.container}>
      <div style={styles.header}>
        <span style={styles.logo}>Hyv</span>
        <span style={styles.version}>{version ? `v${version}` : ""}</span>
      </div>

      <StatusIndicator status={status} recordingTime={recordingTime} formatTime={formatTime} />
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
