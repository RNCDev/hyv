import { startRecording, stopRecording, type AppStatus } from "../lib/commands";

interface Props {
  status: AppStatus;
  recordingTime: number;
  formatTime: (secs: number) => string;
}

export function RecordingControls({ status }: Props) {
  const isIdle = status.type === "Idle";
  const isRecording = status.type === "Recording";
  const isProcessing = status.type === "Processing";
  const isDownloading = status.type === "ModelDownloading";

  const handleClick = async () => {
    try {
      if (isIdle) {
        await startRecording();
      } else if (isRecording) {
        await stopRecording();
      }
    } catch (err) {
      console.error("Action failed:", err);
    }
  };

  const progress = isProcessing ? status.data.progress : isDownloading ? status.data.progress : 0;

  return (
    <div style={styles.container}>
      {(isProcessing || isDownloading) && (
        <div style={styles.progressBar}>
          <div
            style={{
              ...styles.progressFill,
              width: `${Math.min(progress, 100)}%`,
            }}
          />
        </div>
      )}

      <button
        onClick={handleClick}
        disabled={!isIdle && !isRecording}
        style={{
          ...styles.button,
          background: isRecording ? "#ef4444" : isIdle ? "#4ade80" : "#555",
          cursor: isIdle || isRecording ? "pointer" : "default",
          opacity: isIdle || isRecording ? 1 : 0.5,
        }}
      >
        {isRecording ? "Stop Recording" : isIdle ? "Start Recording" : "Processing..."}
      </button>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  container: {
    display: "flex",
    flexDirection: "column",
    gap: "8px",
  },
  button: {
    width: "100%",
    padding: "12px",
    border: "none",
    borderRadius: "8px",
    color: "#000",
    fontSize: "14px",
    fontWeight: 600,
    transition: "all 0.15s ease",
  },
  progressBar: {
    height: "4px",
    background: "#333",
    borderRadius: "2px",
    overflow: "hidden",
  },
  progressFill: {
    height: "100%",
    background: "#3b82f6",
    borderRadius: "2px",
    transition: "width 0.3s ease",
  },
};
