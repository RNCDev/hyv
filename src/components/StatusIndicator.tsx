import type { AppStatus } from "../lib/commands";

interface Props {
  status: AppStatus;
  recordingTime: number;
  formatTime: (secs: number) => string;
}

export function StatusIndicator({ status, recordingTime, formatTime }: Props) {
  const { icon, text, color } = getStatusDisplay(status, recordingTime, formatTime);

  return (
    <div style={styles.container}>
      <span style={{ ...styles.icon, color }}>{icon}</span>
      <span style={{ ...styles.text, color }}>{text}</span>
    </div>
  );
}

function getStatusDisplay(
  status: AppStatus,
  recordingTime: number,
  formatTime: (secs: number) => string,
): { icon: string; text: string; color: string } {
  switch (status.type) {
    case "Idle":
      return { icon: "●", text: "Ready to record", color: "#4ade80" };
    case "Recording":
      return { icon: "◉", text: `Recording ${formatTime(recordingTime)}`, color: "#ef4444" };
    case "Processing":
      return { icon: "⟳", text: status.data.message, color: "#f59e0b" };
    case "ModelDownloading":
      return { icon: "↓", text: status.data.message, color: "#3b82f6" };
    case "Error":
      return { icon: "⚠", text: status.data.message, color: "#ef4444" };
  }
}

const styles: Record<string, React.CSSProperties> = {
  container: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    padding: "8px 12px",
    background: "#16213e",
    borderRadius: "8px",
  },
  icon: {
    fontSize: "16px",
  },
  text: {
    fontSize: "13px",
    fontWeight: 500,
  },
};
