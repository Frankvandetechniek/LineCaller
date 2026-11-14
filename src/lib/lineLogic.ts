// Pure rules for what each control does in each line state. Kept free of
// React so they can be unit tested (lineLogic.test.ts).

import type { Audio, LineView, SendableAudio } from "./types";

export const AUDIO_BUTTONS: { audio: SendableAudio; label: string; short: string }[] = [
  { audio: "pretalk", label: "PRETALK", short: "PRE" },
  { audio: "hold", label: "HOLD", short: "HOLD" },
  { audio: "onAir1", label: "ON-AIR A", short: "AIR A" },
  { audio: "onAir2", label: "ON-AIR B", short: "AIR B" },
];

export function audioLabel(a: Audio): string {
  switch (a) {
    case "pretalk": return "PRETALK";
    case "hold": return "HOLD";
    case "onAir1": return "ON-AIR A";
    case "onAir2": return "ON-AIR B";
    default: return "UNKNOWN";
  }
}

export type Phase =
  | "unknown"      // no status yet
  | "stale"        // link down, last known status shown
  | "elsewhere"    // occupied by another Remote Light location
  | "idle"
  | "ringing"
  | "calling"
  | "connected"
  | "reserved"
  | "undefined";

export function linePhase(view: LineView | null | undefined, myLocation: number): Phase {
  if (!view) return "unknown";
  if (view.stale) return "stale";
  const s = view.status;
  if (s.location !== 0 && s.location !== myLocation) return "elsewhere";
  switch (s.state.kind) {
    case "disconnected": return "idle";
    case "ringing": return "ringing";
    case "calling": return "calling";
    case "connected": return "connected";
    case "reserved": return "reserved";
    default: return "undefined";
  }
}

export function isOnAir(view: LineView | null | undefined): boolean {
  return !!view && !view.stale && view.status.state.kind !== "disconnected" &&
    (view.status.audio === "onAir1" || view.status.audio === "onAir2");
}

export type AudioAction =
  | { type: "openKeypad"; audio: SendableAudio }
  | { type: "accept"; audio: SendableAudio }
  | { type: "setAudio"; audio: SendableAudio }
  | { type: "none"; reason: string };

/** What pressing PRETALK / HOLD / ON-AIR A / ON-AIR B does right now. */
export function audioButtonAction(
  view: LineView | null | undefined,
  audio: SendableAudio,
  myLocation: number,
  connected: boolean,
): AudioAction {
  if (!connected) return { type: "none", reason: "Not connected to the THipPro" };
  const phase = linePhase(view, myLocation);
  switch (phase) {
    case "unknown": return { type: "none", reason: "Waiting for line status" };
    case "stale": return { type: "none", reason: "Line status is out of date" };
    case "elsewhere": return { type: "none", reason: `In use by location ${view!.status.location}` };
    case "idle": return { type: "openKeypad", audio };
    case "ringing": return { type: "accept", audio };
    case "calling":
    case "connected":
    case "reserved":
      if (view!.status.audio === audio) return { type: "none", reason: `Already on ${audioLabel(audio)}` };
      return { type: "setAudio", audio };
    default: return { type: "none", reason: "Line is in an unrecognised state" };
  }
}

export function canDrop(view: LineView | null | undefined, myLocation: number, connected: boolean): boolean {
  const p = linePhase(view, myLocation);
  return connected && (p === "ringing" || p === "calling" || p === "connected" || p === "reserved");
}

export function canLockAudio(view: LineView | null | undefined, myLocation: number, connected: boolean): boolean {
  const p = linePhase(view, myLocation);
  return connected && (p === "calling" || p === "connected" || p === "reserved");
}

export function canBlockIncoming(view: LineView | null | undefined, myLocation: number, connected: boolean): boolean {
  const p = linePhase(view, myLocation);
  return connected && p !== "unknown" && p !== "stale" && p !== "elsewhere";
}

export function callerName(view: LineView | null | undefined): string {
  if (!view) return "";
  const { firstName, name } = view.status.screening;
  return [firstName, name].filter(Boolean).join(" ");
}

/** Allowed characters in the dial keypad (mirrors the backend rule). */
export const DIAL_CHARS = /^[0-9*#+]*$/;
export const MAX_DIAL_LEN = 64;

export function sanitizeNumber(input: string): string {
  return input.replace(/[^0-9*#+]/g, "").slice(0, MAX_DIAL_LEN);
}

export function formatDuration(ms: number): string {
  const t = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(t / 3600);
  const m = Math.floor((t % 3600) / 60);
  const s = t % 60;
  const mm = String(m).padStart(h ? 2 : 1, "0");
  const ss = String(s).padStart(2, "0");
  return h ? `${h}:${mm}:${ss}` : `${mm}:${ss}`;
}

export function timeAgo(atMs: number, now: number): string {
  const s = Math.max(0, Math.round((now - atMs) / 1000));
  if (s < 60) return "just now";
  const m = Math.round(s / 60);
  if (m < 60) return `${m} min ago`;
  const h = Math.round(m / 60);
  if (h < 24) return `${h} h ago`;
  return new Date(atMs).toLocaleDateString();
}
