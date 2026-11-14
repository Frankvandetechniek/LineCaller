// Browser-only preview backend, used when the UI is opened with `npm run dev`
// in a normal browser (no Tauri). It fakes a few lines so the interface can
// be designed and reviewed. It does NOT use the Remote Light Protocol: the
// real simulator lives in Rust (crates/linecaller-core/src/simulator.rs) and
// is what the desktop app's simulator mode uses.

import type { Backend } from "./api";
import type { Bootstrap, ConnectionStatus, LineStatus, LineView, RecentCall, SendableAudio, Settings } from "./types";
import { MAX_LINES } from "./types";

const DEFAULT_SETTINGS: Settings = {
  host: "",
  port: 10100,
  location: 1,
  lineCount: 8,
  theme: "dark",
  density: "comfortable",
  lineLabels: ["Studio hotline", "Request line", "", "", "Reporter", "", "", "", "", "", "", "", "", "", "", ""],
  defaultAnswerAudio: "pretalk",
  autoConnect: true,
  simulator: true,
  livenessCheck: true,
  livenessIntervalSecs: 20,
  stripOutline: true,
  conferenceMode: false,
  lineColors: Array.from({ length: 16 }, () => ""),
};

const empty = { phoneNumber: "", name: "", firstName: "", city: "" };

export function previewBackend(): Backend {
  let settings: Settings = { ...DEFAULT_SETTINGS };
  try {
    const saved = localStorage.getItem("linecaller-preview-settings");
    if (saved) settings = { ...settings, ...JSON.parse(saved) };
  } catch { /* ignore */ }
  let recents: RecentCall[] = [
    { number: "+31356771234", lineNumber: 2, atMs: Date.now() - 600_000 },
    { number: "0201234567", lineNumber: 1, atMs: Date.now() - 3_600_000 },
  ];
  const now = Date.now();
  const lines: LineView[] = Array.from({ length: MAX_LINES }, (_, i) => ({
    status: { line: i, state: { kind: "disconnected" }, audio: "hold", location: 0, audioLocked: false, lineLocked: false, screening: { ...empty } },
    updatedAtMs: now, stateSinceMs: now, stale: false,
  }));
  const seed = (i: number, s: Partial<LineStatus>, ago = 0) => {
    lines[i] = { ...lines[i], status: { ...lines[i].status, ...s }, stateSinceMs: now - ago };
  };
  seed(0, { state: { kind: "connected" }, audio: "onAir1", location: 1, screening: { phoneNumber: "+31612345678", name: "de Vries", firstName: "Anouk", city: "Hilversum" } }, 192_000);
  seed(1, { state: { kind: "ringing" }, screening: { phoneNumber: "+31201234567", name: "Jansen", firstName: "Mark", city: "Amsterdam" } }, 6_000);
  seed(2, { state: { kind: "connected" }, audio: "pretalk", location: 1, screening: { phoneNumber: "0302345678", name: "", firstName: "", city: "Utrecht" } }, 41_000);
  seed(3, { state: { kind: "calling" }, audio: "hold", location: 1, screening: { ...empty, phoneNumber: "0356001122" } }, 3_000);
  seed(5, { state: { kind: "connected" }, audio: "pretalk", location: 6, screening: { phoneNumber: "+31356771234", name: "Smit", firstName: "Eva", city: "Bussum" } }, 300_000);
  seed(6, { lineLocked: true });

  let connection: ConnectionStatus = { phase: "connected", sinceMs: now, endpoint: "browser preview", lastError: null };
  let h: Parameters<Backend["subscribe"]>[0] | null = null;

  const update = (i: number, s: Partial<LineStatus>) => {
    const prev = lines[i];
    const status = { ...prev.status, ...s };
    const t = Date.now();
    lines[i] = { status, updatedAtMs: t, stateSinceMs: status.state.kind === prev.status.state.kind ? prev.stateSinceMs : t, stale: false };
    const v = lines[i];
    setTimeout(() => h?.line(v), 120);
  };
  const boot = (): Bootstrap => ({ settings, connection, lines: lines.map((l) => ({ ...l })), recents, connectRequested: true, version: "preview" });
  const reset = (i: number) => update(i, { state: { kind: "disconnected" }, audio: "hold", location: 0, audioLocked: false, screening: { ...empty } });
  const room = (target: number, audio: SendableAudio) => {
    if (audio === "hold") return;
    lines.forEach((l, i) => {
      const s = l.status;
      // Like the desktop backend: without conference mode a line already on the
      // target bus goes to HOLD; it is never dropped.
      if (i !== target && s.location === settings.location && s.audio === audio && !s.audioLocked && s.state.kind === "connected") {
        if (audio === "pretalk" || !settings.conferenceMode) update(i, { audio: "hold" });
      }
    });
  };

  return {
    kind: "preview",
    async bootstrap() { return boot(); },
    async saveSettings(s) {
      settings = { ...s };
      try { localStorage.setItem("linecaller-preview-settings", JSON.stringify(settings)); } catch { /* ignore */ }
      return boot();
    },
    async connect() { connection = { ...connection, phase: "connected", sinceMs: Date.now() }; h?.connection(connection); },
    async disconnect() { connection = { phase: "offline", endpoint: "", lastError: null }; h?.connection(connection); },
    async dial(i, audio, number) {
      room(i, audio);
      update(i, { state: { kind: "calling" }, audio, location: settings.location, screening: { ...empty, phoneNumber: number } });
      setTimeout(() => update(i, { state: { kind: "connected" } }), 2500);
      recents = [{ number, lineNumber: i + 1, atMs: Date.now() }, ...recents.filter((r) => r.number !== number)].slice(0, 20);
      return recents;
    },
    async acceptCall(i, audio) { room(i, audio); update(i, { state: { kind: "connected" }, audio, location: settings.location }); },
    async setAudio(i, audio) { room(i, audio); update(i, { audio, location: settings.location }); },
    async dropLine(i) { reset(i); },
    async setIncomingBlock(i, locked) { update(i, { lineLocked: locked }); },
    async refreshLines() {},
    async clearRecents() { recents = []; return recents; },
    async simulateIncomingCall(i) {
      update(i, { state: { kind: "ringing" }, location: 0, screening: { phoneNumber: "+3243211234", name: "Peeters", firstName: "Lotte", city: "Antwerpen" } });
    },
    async exportConfig(s) {
      const body = JSON.stringify({ format: "linecaller-config", formatVersion: 1, exportedAt: new Date().toISOString(), appVersion: "preview", settings: s }, null, 2) + "\n";
      const a = document.createElement("a");
      a.href = URL.createObjectURL(new Blob([body], { type: "application/json" }));
      a.download = "linecaller-config.json";
      a.click();
      setTimeout(() => URL.revokeObjectURL(a.href), 1000);
      return "linecaller-config.json (browser download)";
    },
    importConfig() {
      return new Promise<Settings | null>((resolve, reject) => {
        const input = document.createElement("input");
        input.type = "file";
        input.accept = ".json,application/json";
        input.onchange = async () => {
          const f = input.files?.[0];
          if (!f) return resolve(null);
          try {
            const parsed = JSON.parse(await f.text());
            if (parsed?.format !== "linecaller-config") throw new Error("This is not a LineCaller configuration file.");
            resolve({ ...DEFAULT_SETTINGS, ...parsed.settings });
          } catch (e) {
            reject({ code: "importFailed", message: e instanceof Error ? e.message : String(e) });
          }
        };
        input.click();
      });
    },
    async subscribe(handlers) { h = handlers; return () => { h = null; }; },
  };
}
