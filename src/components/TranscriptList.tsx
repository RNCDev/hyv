import { useState, useEffect } from "react";
import { getRecentTranscripts, openTranscript } from "../lib/commands";

export function TranscriptList() {
  const [transcripts, setTranscripts] = useState<string[]>([]);

  useEffect(() => {
    loadTranscripts();
    const interval = setInterval(loadTranscripts, 5000);
    return () => clearInterval(interval);
  }, []);

  const loadTranscripts = async () => {
    try {
      const list = await getRecentTranscripts();
      setTranscripts(list);
    } catch (err) {
      console.error("Failed to load transcripts:", err);
    }
  };

  if (transcripts.length === 0) {
    return (
      <div style={styles.empty}>
        <span style={styles.emptyText}>No transcripts yet</span>
      </div>
    );
  }

  return (
    <div style={styles.container}>
      <span style={styles.header}>Recent Transcripts</span>
      {transcripts.map((path) => {
        const name = path.split("/").pop() || path;
        return (
          <button
            key={path}
            onClick={() => openTranscript(path)}
            style={styles.item}
          >
            {name.replace("Hyv_Transcript_", "").replace(".txt", "")}
          </button>
        );
      })}
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  container: {
    display: "flex",
    flexDirection: "column",
    gap: "4px",
  },
  header: {
    fontSize: "11px",
    color: "#888",
    textTransform: "uppercase" as const,
    letterSpacing: "0.5px",
    marginBottom: "4px",
  },
  item: {
    display: "block",
    width: "100%",
    textAlign: "left" as const,
    padding: "8px 10px",
    background: "#16213e",
    border: "none",
    borderRadius: "6px",
    color: "#ccc",
    fontSize: "12px",
    cursor: "pointer",
  },
  empty: {
    padding: "16px",
    textAlign: "center" as const,
  },
  emptyText: {
    fontSize: "12px",
    color: "#666",
  },
};
