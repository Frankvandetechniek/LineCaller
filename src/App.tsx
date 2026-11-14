import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createBackend, toAppError, type Backend } from "./lib/api";
import type { AppError, Bootstrap, ConnectionStatus, LineView, RecentCall, SendableAudio, Settings } from "./lib/types";
import { audioLabel, callerName, linePhase, type AudioAction } from "./lib/lineLogic";
import { ChannelStrip } from "./components/ChannelStrip";
import { Keypad } from "./components/Keypad";
import { SettingsPanel } from "./components/SettingsPanel";
import { ConnectionPill, ConnectionBanner } from "./components/Connection";
import { Toasts, type Toast } from "./components/Toasts";
import { IconFlask, IconRefresh, IconSettings } from "./components/Icons";

type Pending = Record<number, { control: string; at: number }>;
const PENDING_TIMEOUT_MS = 4000;

export default function App() {
  const [backend, setBackend] = useState<Backend | null>(null);
  const [boot, setBoot] = useState<Bootstrap | null>(null);
  const [fatal, setFatal] = useState<string | null>(null);
  const [connection, setConnection] = useState<ConnectionStatus | null>(null);
  const [lines, setLines] = useState<(LineView | null)[]>([]);
  const [recents, setRecents] = useState<RecentCall[]>([]);
  const [pending, setPending] = useState<Pending>({});
  const [toasts, setToasts] = useState<Toast[]>([]);
  const [keypad, setKeypad] = useState<{ lineIndex: number; audio: SendableAudio } | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  const [announce, setAnnounce] = useState("");
  const toastId = useRef(0);
  const settingsRef = useRef<Settings | null>(null);

  const settings = boot?.settings ?? null;
  settingsRef.current = settings;

  const toast = useCallback((kind: Toast["kind"], text: string) => {
    const id = ++toastId.current;
    setToasts((t) => [...t.slice(-3), { id, kind, text }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), kind === "error" ? 8000 : 5000);
  }, []);

  const applyBoot = useCallback((b: Bootstrap) => {
    setBoot(b);
    setConnection(b.connection);
    // A line event can arrive while the bootstrap snapshot is in flight; keep whichever is newer.
    setLines((prev) => b.lines.map((v, i) => {
      const p = prev[i];
      if (!v) return p ?? null;
      if (p && p.updatedAtMs > v.updatedAtMs) return p;
      return v;
    }));
    setRecents(b.recents);
  }, []);

  // Start-up: pick backend, load state, subscribe to events.
  useEffect(() => {
    let unsub: (() => void) | undefined;
    let cancelled = false;
    (async () => {
      try {
        const be = await createBackend();
        unsub = await be.subscribe({
          connection: (s) => setConnection(s),
          line: (v) => {
            setLines((prev) => {
              const next = prev.slice();
              const before = prev[v.status.line];
              next[v.status.line] = v;
              const s = settingsRef.current;
              if (s && v.status.line < s.lineCount && !v.stale && v.status.state.kind === "ringing" &&
                  before?.status.state.kind !== "ringing") {
                const who = callerName(v) || v.status.screening.phoneNumber || "unknown caller";
                setAnnounce(`Incoming call on line ${v.status.line + 1}: ${who}${v.status.screening.city ? `, ${v.status.screening.city}` : ""}`);
              }
              return next;
            });
            if (!v.stale) {
              setPending((p) => {
                if (!(v.status.line in p)) return p;
                const n = { ...p };
                delete n[v.status.line];
                return n;
              });
            }
          },
          warning: (m) => toast("warn", m),
        });
        if (cancelled) { unsub(); return; }
        setBackend(be);
        applyBoot(await be.bootstrap());
      } catch (e) {
        setFatal(toAppError(e).message);
      }
    })();
    return () => { cancelled = true; unsub?.(); };
  }, [applyBoot, toast]);

  // Clock for call timers and pending timeouts.
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);

  useEffect(() => {
    const expired = Object.entries(pending).filter(([, v]) => now - v.at > PENDING_TIMEOUT_MS);
    if (expired.length === 0) return;
    setPending((p) => {
      const n = { ...p };
      for (const [k] of expired) delete n[Number(k)];
      return n;
    });
    for (const [k] of expired) toast("warn", `No status update from the THipPro for line ${Number(k) + 1} yet. The command may have been ignored.`);
  }, [now, pending, toast]);

  // Theme and density.
  useEffect(() => {
    const root = document.documentElement;
    const theme = settings?.theme ?? "dark";
    const mq = window.matchMedia("(prefers-color-scheme: light)");
    const apply = () => { root.dataset.theme = theme === "system" ? (mq.matches ? "light" : "dark") : theme; };
    apply();
    root.dataset.density = settings?.density ?? "comfortable";
    root.dataset.stripOutline = settings?.stripOutline === false ? "off" : "on";
    if (theme !== "system") return;
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [settings?.theme, settings?.density, settings?.stripOutline]);

  const connected = connection?.phase === "connected";
  const myLocation = settings?.location ?? 1;

  const run = useCallback(async (lineIndex: number | null, control: string, fn: () => Promise<unknown>) => {
    if (lineIndex !== null) setPending((p) => ({ ...p, [lineIndex]: { control, at: Date.now() } }));
    try {
      await fn();
      return true;
    } catch (e) {
      const err = toAppError(e);
      if (lineIndex !== null) setPending((p) => { const n = { ...p }; delete n[lineIndex]; return n; });
      toast("error", err.message);
      return false;
    }
  }, [toast]);

  const onAudio = useCallback((i: number, a: AudioAction) => {
    if (!backend) return;
    switch (a.type) {
      case "openKeypad": setKeypad({ lineIndex: i, audio: a.audio }); break;
      case "accept": void run(i, a.audio, () => backend.acceptCall(i, a.audio)); break;
      case "setAudio": void run(i, a.audio, () => backend.setAudio(i, a.audio)); break;
    }
  }, [backend, run]);

  const onDrop = useCallback((i: number) => { if (backend) void run(i, "drop", () => backend.dropLine(i)); }, [backend, run]);
  // LOCK = Set Lock Line (block incoming calls).
  const onLockLine = useCallback((i: number, l: boolean) => { if (backend) void run(i, "lock", () => backend.setIncomingBlock(i, l)); }, [backend, run]);
  const onSimulate = useCallback((i: number) => { if (backend) void run(null, "sim", () => backend.simulateIncomingCall(i)); }, [backend, run]);

  const onCall = useCallback(async (i: number, audio: SendableAudio, number: string) => {
    if (!backend) return;
    const ok = await run(i, audio, async () => setRecents(await backend.dial(i, audio, number)));
    if (ok) {
      setKeypad(null);
      toast("info", `Calling ${number} on line ${i + 1} → ${audioLabel(audio)}`);
    }
  }, [backend, run, toast]);

  const onClearRecents = useCallback(async () => {
    if (backend) setRecents(await backend.clearRecents());
  }, [backend]);

  const onSaveSettings = useCallback(async (s: Settings): Promise<AppError | null> => {
    if (!backend) return null;
    try {
      applyBoot(await backend.saveSettings(s));
      toast("info", "Settings saved");
      return null;
    } catch (e) {
      return toAppError(e);
    }
  }, [backend, applyBoot, toast]);

  const shown = settings?.lineCount ?? 0;
  const visible = useMemo(() => Array.from({ length: shown }, (_, i) => lines[i] ?? null), [lines, shown]);
  const keypadLine = keypad ? visible[keypad.lineIndex] : null;
  const keypadBusy = !!keypad && (!connected || linePhase(keypadLine, myLocation) !== "idle");

  if (fatal) {
    return <main className="fatal"><h1>LineCaller could not start</h1><p>{fatal}</p></main>;
  }
  if (!boot || !settings || !connection || !backend) {
    return <main className="loading" aria-busy="true"><div className="spinner" /> Starting LineCaller…</main>;
  }

  return (
    <div className="app">
      <header className="topbar">
        <div className="brand">
          <img src="/logo.svg" alt="" width={28} height={28} />
          <span className="brand-name">LineCaller</span>
          {settings.simulator && <span className="sim-tag" title="Connected to the built-in THipPro simulator. No real calls are placed."><IconFlask size={13} /> SIMULATOR</span>}
          {backend.kind === "preview" && <span className="sim-tag" title="Running in a browser without the desktop backend">BROWSER PREVIEW</span>}
        </div>
        <div className="top-actions">
          <ConnectionPill status={connection} location={settings.location} now={now} simulator={settings.simulator} />
          <button type="button" className="icon-btn" title="Request status of all lines" aria-label="Refresh line status"
            disabled={!connected} onClick={() => void run(null, "refresh", () => backend.refreshLines())}>
            <IconRefresh />
          </button>
          <button type="button" className="icon-btn" aria-label="Settings" title="Settings" onClick={() => setSettingsOpen(true)}>
            <IconSettings />
          </button>
        </div>
      </header>

      <ConnectionBanner
        status={connection}
        now={now}
        hostMissing={!settings.simulator && !settings.host}
        onConnect={() => void run(null, "connect", () => backend.connect())}
        onDisconnect={() => void run(null, "disconnect", () => backend.disconnect())}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      <main className="console" aria-label="Lines">
        <div className="console-rack" data-count={shown} style={{ ["--strips" as string]: shown }}>
          {visible.map((v, i) => (
            <ChannelStrip
              key={i}
              index={i}
              view={v}
              label={settings.lineLabels[i] ?? ""}
              myLocation={myLocation}
              defaultAnswerAudio={settings.defaultAnswerAudio}
              connected={connected}
              now={now}
              pending={pending[i]?.control ?? null}
              simulator={settings.simulator}
              onAudio={onAudio}
              onDrop={onDrop}
              onLockLine={onLockLine}
              onSimulateCall={onSimulate}
            />
          ))}
        </div>
      </main>

      <div className="sr-only" aria-live="assertive" role="status">{announce}</div>
      <Toasts toasts={toasts} onDismiss={(id) => setToasts((t) => t.filter((x) => x.id !== id))} />

      {keypad && (
        <Keypad
          lineIndex={keypad.lineIndex}
          lineLabel={settings.lineLabels[keypad.lineIndex] ?? ""}
          initialAudio={keypad.audio}
          recents={recents}
          now={now}
          busy={keypadBusy}
          busyReason={!connected ? "Not connected to the THipPro." : keypadBusy ? `Line ${keypad.lineIndex + 1} is no longer free.` : ""}
          onCall={onCall}
          onClose={() => setKeypad(null)}
          onClearRecents={onClearRecents}
        />
      )}
      {settingsOpen && (
        <SettingsPanel
          settings={settings}
          version={boot.version}
          previewMode={backend.kind === "preview"}
          recentsCount={recents.length}
          onSave={onSaveSettings}
          onClose={() => setSettingsOpen(false)}
          onClearRecents={onClearRecents}
          onExport={(s) => backend.exportConfig(s)}
          onImport={() => backend.importConfig()}
        />
      )}
    </div>
  );
}
