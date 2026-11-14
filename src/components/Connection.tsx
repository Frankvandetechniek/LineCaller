import type { ConnectionStatus } from "../lib/types";
import { IconAlert, IconPlug } from "./Icons";

function describe(s: ConnectionStatus, now: number): { text: string; tone: "ok" | "busy" | "bad" | "off" } {
  switch (s.phase) {
    case "connected": return { text: "Connected", tone: "ok" };
    case "connecting": return { text: s.attempt > 1 ? `Connecting (attempt ${s.attempt})` : "Connecting…", tone: "busy" };
    case "retrying": {
      const secs = Math.max(0, Math.ceil((s.retryAtMs - now) / 1000));
      return { text: secs > 0 ? `Reconnecting in ${secs}s` : "Reconnecting…", tone: "bad" };
    }
    default: return { text: "Offline", tone: "off" };
  }
}

export function ConnectionPill({ status, location, now, simulator }: { status: ConnectionStatus; location: number; now: number; simulator: boolean }) {
  const d = describe(status, now);
  const where = simulator ? "Simulator" : status.endpoint || "No host";
  return (
    <div className="conn-pill" data-tone={d.tone} role="status" aria-live="polite" title={status.lastError ?? where}>
      <span className="conn-dot" aria-hidden="true" />
      <span className="conn-text">{d.text}</span>
      <span className="conn-meta">{where} · Loc {location}</span>
    </div>
  );
}

export function ConnectionBanner(p: {
  status: ConnectionStatus;
  now: number;
  hostMissing: boolean;
  onConnect: () => void;
  onDisconnect: () => void;
  onOpenSettings: () => void;
}) {
  const s = p.status;
  if (s.phase === "connected") return null;
  if (p.hostMissing) {
    return (
      <div className="banner" data-tone="off" role="region" aria-label="Connection">
        <IconPlug size={18} />
        <div className="banner-text"><strong>No THipPro configured.</strong> Enter the host, port and location in Settings, or turn on simulator mode.</div>
        <button type="button" className="primary-btn" onClick={p.onOpenSettings}>Open settings</button>
      </div>
    );
  }
  const d = describe(s, p.now);
  return (
    <div className="banner" data-tone={d.tone} role="region" aria-label="Connection">
      <IconAlert size={18} />
      <div className="banner-text">
        <strong>{s.phase === "offline" ? "Not connected to the THipPro." : `${d.text}.`}</strong>{" "}
        {s.lastError ? <span>{s.lastError}.</span> : s.phase === "offline" ? <span>Line controls are disabled until LineCaller is connected.</span> : <span>Line controls are disabled until the link is back.</span>}
      </div>
      {s.phase === "offline" || s.phase === "retrying"
        ? <button type="button" className="primary-btn" onClick={p.onConnect}>{s.phase === "offline" ? "Connect" : "Retry now"}</button>
        : null}
      {s.phase !== "offline" && <button type="button" className="secondary-btn" onClick={p.onDisconnect}>Stop</button>}
    </div>
  );
}
