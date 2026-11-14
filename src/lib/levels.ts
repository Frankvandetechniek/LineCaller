// R/T level meters: latest levels from the backend plus one animation loop
// that draws every meter. Meters are drawn by writing styles straight to the
// DOM so 20 updates a second never re-render React components.

import type { LineLevels } from "./types";

/** Meter range in dBFS. */
export const METER_FLOOR = -60;
export const METER_CEIL = 0;
/** No levels for this long: meters dim ("no level data"). */
const STALE_MS = 700;
/** Ballistics: fast rise, falls at 24 dB/s; peak holds 1.2 s then falls at 18 dB/s. */
const RISE = 0.55;
const FALL_DB_PER_S = 24;
const PEAK_HOLD_MS = 1200;
const PEAK_FALL_DB_PER_S = 18;

let latest: LineLevels | null = null;
let latestAt = 0;

export function pushLevels(l: LineLevels) {
  latest = l;
  latestAt = performance.now();
}

interface MeterEl {
  root: HTMLElement;
  fill: HTMLElement;
  peak: HTMLElement;
  line: number;
  kind: "rx" | "tx";
  shown: number;
  peakDb: number;
  peakAt: number;
}

const meters = new Set<MeterEl>();
let raf = 0;
let lastFrame = 0;

const pct = (db: number) => Math.max(0, Math.min(1, (db - METER_FLOOR) / (METER_CEIL - METER_FLOOR))) * 100;

function frame(now: number) {
  const dt = lastFrame ? Math.min(0.1, (now - lastFrame) / 1000) : 0;
  lastFrame = now;
  const live = !!latest && now - latestAt < STALE_MS;
  for (const m of meters) {
    const target = live ? latest![m.kind][m.line] ?? -128 : -128;
    const t = Math.max(METER_FLOOR - 1, target);
    m.shown = t > m.shown ? m.shown + (t - m.shown) * RISE : Math.max(t, m.shown - FALL_DB_PER_S * dt);
    if (m.shown >= m.peakDb) { m.peakDb = m.shown; m.peakAt = now; }
    else if (now - m.peakAt > PEAK_HOLD_MS) m.peakDb = Math.max(METER_FLOOR - 1, m.peakDb - PEAK_FALL_DB_PER_S * dt);
    m.fill.style.clipPath = `inset(0 ${100 - pct(m.shown)}% 0 0)`;
    m.peak.style.left = `${pct(m.peakDb)}%`;
    m.peak.style.opacity = m.peakDb > METER_FLOOR ? "1" : "0";
    if (m.root.dataset.live !== String(live)) m.root.dataset.live = String(live);
  }
  raf = meters.size ? requestAnimationFrame(frame) : 0;
}

export function registerMeter(root: HTMLElement, line: number, kind: "rx" | "tx"): () => void {
  const fill = root.querySelector<HTMLElement>(".meter-fill")!;
  const peak = root.querySelector<HTMLElement>(".meter-peak")!;
  const m: MeterEl = { root, fill, peak, line, kind, shown: METER_FLOOR - 1, peakDb: METER_FLOOR - 1, peakAt: 0 };
  meters.add(m);
  if (!raf) { lastFrame = 0; raf = requestAnimationFrame(frame); }
  return () => { meters.delete(m); };
}
