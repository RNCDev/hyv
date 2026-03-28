import { useState, useEffect, useCallback } from "react";
import { listen } from "@tauri-apps/api/event";
import { getStatus, type AppStatus } from "../lib/commands";

export function useAppState() {
  const [status, setStatus] = useState<AppStatus>({ type: "Idle" });
  const [recordingTime, setRecordingTime] = useState(0);

  useEffect(() => {
    // Get initial status
    getStatus().then(setStatus).catch(console.error);

    // Listen for status changes
    const unlisten = listen<{ status: AppStatus }>("status-changed", (event) => {
      setStatus(event.payload.status);
    });

    // Poll as fallback in case events are missed
    const poll = setInterval(() => {
      getStatus().then(setStatus).catch(console.error);
    }, 1000);

    return () => {
      unlisten.then((fn) => fn());
      clearInterval(poll);
    };
  }, []);

  // Recording timer
  useEffect(() => {
    if (status.type !== "Recording") {
      setRecordingTime(0);
      return;
    }

    const start = Date.now();
    const interval = setInterval(() => {
      setRecordingTime(Math.floor((Date.now() - start) / 1000));
    }, 1000);

    return () => clearInterval(interval);
  }, [status.type]);

  const formatTime = useCallback((secs: number): string => {
    const m = Math.floor(secs / 60);
    const s = secs % 60;
    return `${m}:${s.toString().padStart(2, "0")}`;
  }, []);

  return { status, recordingTime, formatTime };
}
