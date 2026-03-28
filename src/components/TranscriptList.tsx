import { useState, useEffect } from "react";
import { getRecentTranscripts, openTranscript, deleteTranscript } from "../lib/commands";

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

  const handleDelete = async (e: React.MouseEvent, path: string) => {
    e.stopPropagation();
    try {
      await deleteTranscript(path);
      setTranscripts((prev) => prev.filter((p) => p !== path));
    } catch (err) {
      console.error("Failed to delete transcript:", err);
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
          <div key={path} style={styles.row}>
            <button
              onClick={() => openTranscript(path)}
              style={styles.item}
            >
              {name.replace("Hyv_Transcript_", "").replace(".txt", "")}
            </button>
            <button
              onClick={(e) => handleDelete(e, path)}
              style={styles.deleteBtn}
              title="Delete"
            >
              ✕
            </button>
          </div>
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
  row: {
    display: "flex",
    alignItems: "center",
    gap: "4px",
  },
  item: {
    flex: 1,
    textAlign: "left" as const,
    padding: "8px 10px",
    background: "#16213e",
    border: "none",
    borderRadius: "6px",
    color: "#ccc",
    fontSize: "12px",
    cursor: "pointer",
    minWidth: 0,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap" as const,
  },
  deleteBtn: {
    flexShrink: 0,
    width: "28px",
    height: "28px",
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    background: "transparent",
    border: "none",
    borderRadius: "6px",
    color: "#666",
    fontSize: "11px",
    cursor: "pointer",
    padding: 0,
  },
};
