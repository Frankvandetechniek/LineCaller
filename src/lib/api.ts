// The only module that talks to the backend. Everything protocol related
// happens in Rust; the UI calls these commands and listens to three events.

import type { AppError, Bootstrap, ConnectionStatus, LineView, RecentCall, SendableAudio, Settings } from "./types";

export const EV_CONNECTION = "lc://connection";
export const EV_LINE = "lc://line";
export const EV_WARNING = "lc://warning";

export interface Backend {
  readonly kind: "tauri" | "preview";
  bootstrap(): Promise<Bootstrap>;
  saveSettings(settings: Settings): Promise<Bootstrap>;
  connect(): Promise<void>;
  disconnect(): Promise<void>;
  dial(lineIndex: number, audio: SendableAudio, number: string): Promise<RecentCall[]>;
  acceptCall(lineIndex: number, audio: SendableAudio): Promise<void>;
  setAudio(lineIndex: number, audio: SendableAudio): Promise<void>;
  dropLine(lineIndex: number): Promise<void>;
  setAudioLock(lineIndex: number, locked: boolean): Promise<void>;
  setIncomingBlock(lineIndex: number, locked: boolean): Promise<void>;
  refreshLines(): Promise<void>;
  clearRecents(): Promise<RecentCall[]>;
  simulateIncomingCall(lineIndex: number): Promise<void>;
  /** Save settings to a readable .json file; resolves to the path, or null if cancelled. */
  exportConfig(settings: Settings): Promise<string | null>;
  /** Read a .json configuration file; resolves to validated settings (not yet saved), or null if cancelled. */
  importConfig(): Promise<Settings | null>;
  subscribe(handlers: {
    connection: (s: ConnectionStatus) => void;
    line: (v: LineView) => void;
    warning: (msg: string) => void;
  }): Promise<() => void>;
}

export function toAppError(e: unknown): AppError {
  if (e && typeof e === "object" && "message" in e) {
    const o = e as { code?: string; message: string; field?: string };
    return { code: o.code ?? "error", message: String(o.message), field: o.field };
  }
  return { code: "error", message: String(e) };
}

async function tauriBackend(): Promise<Backend> {
  const { invoke } = await import("@tauri-apps/api/core");
  const { listen } = await import("@tauri-apps/api/event");
  const call = async <T>(cmd: string, args?: Record<string, unknown>): Promise<T> => {
    try {
      return await invoke<T>(cmd, args);
    } catch (e) {
      throw toAppError(e);
    }
  };
  return {
    kind: "tauri",
    bootstrap: () => call("get_bootstrap"),
    saveSettings: (settings) => call("save_settings", { settings }),
    connect: () => call("connect"),
    disconnect: () => call("disconnect"),
    dial: (lineIndex, audio, number) => call("dial", { lineIndex, audio, number }),
    acceptCall: (lineIndex, audio) => call("accept_call", { lineIndex, audio }),
    setAudio: (lineIndex, audio) => call("set_audio", { lineIndex, audio }),
    dropLine: (lineIndex) => call("drop_line", { lineIndex }),
    setAudioLock: (lineIndex, locked) => call("set_audio_lock", { lineIndex, locked }),
    setIncomingBlock: (lineIndex, locked) => call("set_incoming_block", { lineIndex, locked }),
    refreshLines: () => call("refresh_lines"),
    clearRecents: () => call("clear_recents"),
    simulateIncomingCall: (lineIndex) => call("simulate_incoming_call", { lineIndex }),
    exportConfig: (settings) => call("export_config", { settings }),
    importConfig: () => call("import_config"),
    async subscribe(h) {
      const offs = await Promise.all([
        listen<ConnectionStatus>(EV_CONNECTION, (e) => h.connection(e.payload)),
        listen<LineView>(EV_LINE, (e) => h.line(e.payload)),
        listen<string>(EV_WARNING, (e) => h.warning(e.payload)),
      ]);
      return () => offs.forEach((off) => off());
    },
  };
}

/** Tauri when running inside the app; a browser-only preview otherwise. */
export async function createBackend(): Promise<Backend> {
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    return tauriBackend();
  }
  const { previewBackend } = await import("./preview");
  return previewBackend();
}
