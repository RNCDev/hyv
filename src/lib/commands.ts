import { invoke } from "@tauri-apps/api/core";

export type AppStatus =
  | { type: "ModelDownloading"; data: { progress: number; message: string } }
  | { type: "Idle" }
  | { type: "Recording" }
  | { type: "Processing"; data: { progress: number; message: string } }
  | { type: "Error"; data: { message: string } };

export async function getStatus(): Promise<AppStatus> {
  return invoke("get_status");
}

export async function startRecording(): Promise<void> {
  return invoke("start_recording");
}

export async function stopRecording(): Promise<void> {
  return invoke("stop_recording");
}

export async function getRecentTranscripts(): Promise<string[]> {
  return invoke("get_recent_transcripts");
}

export async function openTranscript(path: string): Promise<void> {
  return invoke("open_transcript", { path });
}
