import { useEffect, useState } from "react";
import {
  listModels,
  getActiveModel,
  setActiveModel,
  type ModelInfo,
} from "../lib/commands";

const MODEL_LABELS: Record<string, { label: string; note: string }> = {
  medium: { label: "Medium", note: "1.5 GB · fastest" },
  "large-v3-turbo": { label: "Large v3 Turbo", note: "1.6 GB · more accurate" },
  "distil-large-v3": { label: "Distil Large v3", note: "1.5 GB · best tradeoff" },
  "large-v3": { label: "Large v3", note: "3.1 GB · most accurate" },
  "cohere-transcribe": { label: "Cohere Transcribe", note: "4.1 GB · ONNX · experimental" },
};

interface Props {
  disabled: boolean;
}

export function ModelSelector({ disabled }: Props) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [active, setActive] = useState<string>("cohere-transcribe");

  useEffect(() => {
    Promise.all([listModels(), getActiveModel()]).then(([all, current]) => {
      setModels(all);
      setActive(current.name);
    });
  }, []);

  const handleChange = async (e: React.ChangeEvent<HTMLSelectElement>) => {
    const name = e.target.value;
    await setActiveModel(name);
    setActive(name);
  };

  if (models.length === 0) return null;

  const meta = MODEL_LABELS[active];

  return (
    <div style={styles.container}>
      <label style={styles.label}>Model</label>
      <select
        value={active}
        onChange={handleChange}
        disabled={disabled}
        style={{ ...styles.select, opacity: disabled ? 0.5 : 1 }}
      >
        {models.map((m) => (
          <option key={m.name} value={m.name}>
            {MODEL_LABELS[m.name]?.label ?? m.name}
          </option>
        ))}
      </select>
      {meta && <span style={styles.note}>{meta.note}</span>}
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  container: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
  },
  label: {
    fontSize: "12px",
    color: "#888",
    flexShrink: 0,
  },
  select: {
    flex: 1,
    background: "#222",
    color: "#eee",
    border: "1px solid #444",
    borderRadius: "6px",
    padding: "4px 8px",
    fontSize: "12px",
    cursor: "pointer",
  },
  note: {
    fontSize: "10px",
    color: "#666",
    flexShrink: 0,
  },
};
