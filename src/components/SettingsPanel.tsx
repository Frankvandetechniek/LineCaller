import { useState } from "react";
import type { AppError, Settings } from "../lib/types";
import { MAX_LINES, MAX_LOCATIONS } from "../lib/types";
import { AUDIO_BUTTONS } from "../lib/lineLogic";
import { IconX } from "./Icons";
import { LineColorPicker } from "./LineColorPicker";
import { useDialog } from "./useDialog";

export interface SettingsPanelProps {
  settings: Settings;
  version: string;
  previewMode: boolean;
  recentsCount: number;
  onSave: (s: Settings) => Promise<AppError | null>;
  onClose: () => void;
  onClearRecents: () => void;
  onExport: (s: Settings) => Promise<string | null>;
  onImport: () => Promise<Settings | null>;
}

export function SettingsPanel(p: SettingsPanelProps) {
  const [draft, setDraft] = useState<Settings>(() => ({
    ...p.settings,
    lineLabels: Array.from({ length: MAX_LINES }, (_, i) => p.settings.lineLabels[i] ?? ""),
    lineColors: Array.from({ length: MAX_LINES }, (_, i) => p.settings.lineColors?.[i] ?? ""),
  }));
  const [error, setError] = useState<AppError | null>(null);
  const [saving, setSaving] = useState(false);
  const [fileNote, setFileNote] = useState<{ kind: "ok" | "error"; text: string } | null>(null);
  // Only one file dialog at a time (a double click would otherwise open two).
  const [fileBusy, setFileBusy] = useState(false);

  const doExport = async () => {
    if (fileBusy) return;
    setFileBusy(true);
    setFileNote(null);
    try {
      const path = await p.onExport(draft);
      if (path) setFileNote({ kind: "ok", text: `Configuration exported to ${path}` });
    } catch (e) {
      const err = e as AppError;
      if (err.field) setError(err);
      setFileNote({ kind: "error", text: err.message });
    } finally {
      setFileBusy(false);
    }
  };
  const doImport = async () => {
    if (fileBusy) return;
    setFileBusy(true);
    setFileNote(null);
    try {
      const imported = await p.onImport();
      if (!imported) return;
      setDraft({
        ...imported,
        lineLabels: Array.from({ length: MAX_LINES }, (_, i) => imported.lineLabels?.[i] ?? ""),
        lineColors: Array.from({ length: MAX_LINES }, (_, i) => imported.lineColors?.[i] ?? ""),
      });
      setError(null);
      setFileNote({ kind: "ok", text: "Configuration loaded into this form. Check the Remote Light location for this computer, then press Save settings to apply it." });
    } catch (e) {
      setFileNote({ kind: "error", text: (e as AppError).message });
    } finally {
      setFileBusy(false);
    }
  };
  const dialogRef = useDialog(p.onClose);
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setDraft((d) => ({ ...d, [k]: v }));
  const fieldError = (f: string) => (error?.field === f ? error.message : null);

  const save = async () => {
    setSaving(true);
    const err = await p.onSave(draft);
    setSaving(false);
    if (err) setError(err); else p.onClose();
  };

  return (
    <div className="scrim" onMouseDown={(e) => { if (e.target === e.currentTarget) p.onClose(); }}>
      <div ref={dialogRef} className="dialog settings" role="dialog" aria-modal="true" aria-labelledby="settings-title">
        <header className="dialog-head">
          <div>
            <div className="eyebrow">LineCaller preferences</div>
            <h2 id="settings-title">Settings</h2>
          </div>
          <button type="button" className="icon-btn" onClick={p.onClose} aria-label="Close settings"><IconX /></button>
        </header>

        <form className="settings-body" onSubmit={(e) => { e.preventDefault(); void save(); }} noValidate>
          <section>
            <h3>THipPro connection</h3>
            <label className="switch-row">
              <input type="checkbox" role="switch" checked={draft.simulator} onChange={(e) => set("simulator", e.target.checked)} />
              <span>
                <strong>Simulator mode</strong>
                <small>Use the built-in THipPro simulator. No real calls are placed. Turn off to connect to a real system.</small>
              </span>
            </label>
            <div className="grid-2">
              <Field label="THipPro host or IP" error={fieldError("host")} hint={draft.simulator ? "Not used in simulator mode" : "For example 192.168.1.50"}>
                <input type="text" value={draft.host} disabled={draft.simulator} autoComplete="off" spellCheck={false}
                  onChange={(e) => set("host", e.target.value)} aria-invalid={!!fieldError("host")} />
              </Field>
              <Field label="Remote Light port" error={fieldError("port")} hint="THipPro default 10100 + (location − 1)">
                <input type="number" min={1} max={65535} value={draft.port} disabled={draft.simulator}
                  onChange={(e) => set("port", Number(e.target.value))} aria-invalid={!!fieldError("port")} />
              </Field>
            </div>
            <div className="grid-2">
              <Field label="Remote Light location" error={fieldError("location")} hint="This installation's location (1–6), as configured on the THipPro">
                <select value={draft.location} onChange={(e) => set("location", Number(e.target.value))}>
                  {Array.from({ length: MAX_LOCATIONS }, (_, i) => i + 1).map((n) => <option key={n} value={n}>Location {n}</option>)}
                </select>
              </Field>
              <fieldset className="field">
                <legend>Liveness check</legend>
                <div className="inline">
                  <input type="checkbox" checked={draft.livenessCheck} onChange={(e) => set("livenessCheck", e.target.checked)} aria-label="Enable liveness check" />
                  <input type="number" min={5} max={300} value={draft.livenessIntervalSecs} disabled={!draft.livenessCheck}
                    onChange={(e) => set("livenessIntervalSecs", Number(e.target.value))} aria-label="Liveness interval in seconds" />
                  <span className="muted">seconds</span>
                </div>
                {fieldError("livenessIntervalSecs")
                  ? <span className="field-error" role="alert">{fieldError("livenessIntervalSecs")}</span>
                  : <span className="field-hint">Send a status request when the link has been quiet; reconnect if there is no reply</span>}
              </fieldset>
            </div>
            <label className="switch-row">
              <input type="checkbox" role="switch" checked={draft.autoConnect} onChange={(e) => set("autoConnect", e.target.checked)} />
              <span><strong>Connect on start-up</strong></span>
            </label>
          </section>

          <section>
            <h3>Display</h3>
            <div className="grid-2">
              <Field label="Lines to show" error={fieldError("lineCount")} hint="Lines 1 to N are shown (max 16)">
                <select value={draft.lineCount} onChange={(e) => set("lineCount", Number(e.target.value))}>
                  {Array.from({ length: MAX_LINES }, (_, i) => i + 1).map((n) => <option key={n} value={n}>{n} {n === 1 ? "line" : "lines"}</option>)}
                </select>
              </Field>
              <Field label="Answer incoming calls to" error={fieldError("defaultAnswerAudio")} hint="Used by the Answer button">
                <select value={draft.defaultAnswerAudio} onChange={(e) => set("defaultAnswerAudio", e.target.value as Settings["defaultAnswerAudio"])}>
                  {AUDIO_BUTTONS.map((b) => <option key={b.audio} value={b.audio}>{b.label}</option>)}
                </select>
              </Field>
            </div>
            <div className="grid-2">
              <fieldset className="field">
                <legend>Appearance</legend>
                <div className="seg">
                  {(["dark", "light", "system"] as const).map((t) => (
                    <label key={t} className="seg-opt">
                      <input type="radio" name="theme" checked={draft.theme === t} onChange={() => set("theme", t)} />
                      <span>{t === "dark" ? "Dark" : t === "light" ? "Light" : "System"}</span>
                    </label>
                  ))}
                </div>
              </fieldset>
              <fieldset className="field">
                <legend>Density</legend>
                <div className="seg">
                  {(["comfortable", "compact"] as const).map((d) => (
                    <label key={d} className="seg-opt">
                      <input type="radio" name="density" checked={draft.density === d} onChange={() => set("density", d)} />
                      <span>{d === "comfortable" ? "Comfortable" : "Compact"}</span>
                    </label>
                  ))}
                </div>
              </fieldset>
            </div>
          </section>

          <section>
            <h3>Line labels</h3>
            <p className="muted small">Optional names shown next to the line number, for example "Request line". Max 24 characters. Click the colour box to give a strip its own colour.</p>
            {fieldError("lineLabels") && <p className="field-error" role="alert">{fieldError("lineLabels")}</p>}
            {fieldError("lineColors") && <p className="field-error" role="alert">{fieldError("lineColors")}</p>}
            <div className="labels-grid">
              {Array.from({ length: draft.lineCount }, (_, i) => (
                <label key={i} className="label-input">
                  <span>Line {i + 1}</span>
                  <input type="text" maxLength={24} value={draft.lineLabels[i]} placeholder="—"
                    onChange={(e) => { const l = [...draft.lineLabels]; l[i] = e.target.value; set("lineLabels", l); }} />
                  <LineColorPicker lineNumber={i + 1} value={draft.lineColors[i]}
                    onChange={(v) => { const c = [...draft.lineColors]; c[i] = v; set("lineColors", c); }} />
                </label>
              ))}
            </div>
          </section>

          <section>
            <h3>Preferences</h3>
            <label className="switch-row">
              <input type="checkbox" role="switch" checked={draft.stripOutline} onChange={(e) => set("stripOutline", e.target.checked)} />
              <span>
                <strong>Outline the whole channel strip</strong>
                <span className="field-hint">On: a ringing, on-air or calling line gets a coloured outline around the entire strip. Off: only the coloured status line at the top of the strip shows it.</span>
              </span>
            </label>
            <label className="switch-row">
              <input type="checkbox" role="switch" checked={draft.conferenceMode} onChange={(e) => set("conferenceMode", e.target.checked)} />
              <span>
                <strong>Conference mode for ON-AIR A/B</strong>
                <span className="field-hint">On: two or more lines can be on ON-AIR A (or ON-AIR B) at the same time, in conference. Off: putting a line on air moves the line already there to HOLD. Calls are never dropped.</span>
              </span>
            </label>
            <label className="switch-row">
              <input type="checkbox" role="switch" checked={draft.levelMeters} onChange={(e) => set("levelMeters", e.target.checked)} />
              <span>
                <strong>Level meters from the THipPro</strong>
                <span className="field-hint">Shows the real receive (R) and transmit (T) level of each line. LineCaller opens a second, read-only connection to the THipPro's control port and only listens: it uses one of the THipPro's licensed PC connections. In simulator mode the meters always show simulated levels.</span>
              </span>
            </label>
            {draft.levelMeters && (
              <div className="grid-3">
                <Field label="Control port" error={fieldError("levelPort")} hint="THipPro default 10000">
                  <input type="number" min={1} max={65535} value={draft.levelPort}
                    onChange={(e) => set("levelPort", Number(e.target.value))} aria-invalid={!!fieldError("levelPort")} />
                </Field>
                <Field label="Connect as" error={fieldError("levelConnectionType")} hint="Connection type the THipPro allows">
                  <select value={draft.levelConnectionType} onChange={(e) => set("levelConnectionType", Number(e.target.value))}>
                    <option value={2}>Screener</option>
                    <option value={1}>MAGIC Touch</option>
                    <option value={0}>Maintenance</option>
                  </select>
                </Field>
                <Field label="Password" error={fieldError("levelPassword")} hint="Only if the THipPro has one. Not exported.">
                  <input type="password" maxLength={20} value={draft.levelPassword} autoComplete="off"
                    onChange={(e) => set("levelPassword", e.target.value)} aria-invalid={!!fieldError("levelPassword")} />
                </Field>
              </div>
            )}
          </section>

          <section>
            <h3>Configuration file</h3>
            <p className="muted small">
              Copy this LineCaller's setup to another computer. Export writes a readable .json file with the settings shown in this
              window (connection, location, lines, labels, appearance). Import loads a file into this form; nothing changes until you press Save settings.
            </p>
            <div className="inline">
              <button type="button" className="secondary-btn" disabled={fileBusy} onClick={() => void doImport()}>Import configuration…</button>
              <button type="button" className="secondary-btn" disabled={fileBusy} onClick={() => void doExport()}>Export configuration…</button>
            </div>
            {fileNote && (
              <p className={fileNote.kind === "error" ? "field-error" : "file-ok"} role={fileNote.kind === "error" ? "alert" : "status"}>{fileNote.text}</p>
            )}
          </section>

          <section>
            <h3>Recent numbers</h3>
            <div className="inline">
              <span className="muted">{p.recentsCount} of 20 stored on this computer.</span>
              <button type="button" className="ghost-btn" disabled={p.recentsCount === 0} onClick={p.onClearRecents}>Clear recents</button>
            </div>
          </section>

          <section className="about">
            <p className="muted small">
              LineCaller {p.version}{p.previewMode ? " (browser preview: settings are not sent to a THipPro)" : ""}. Uses the AVT Remote Light Protocol v1 for line control and status only. LineCaller never changes the THipPro's own configuration.
            </p>
          </section>

          {error && !error.field && <p className="field-error" role="alert">{error.message}</p>}
          <div className="dialog-actions">
            <button type="button" className="secondary-btn" onClick={p.onClose}>Cancel</button>
            <button type="submit" className="primary-btn" disabled={saving}>{saving ? "Saving…" : "Save settings"}</button>
          </div>
        </form>
      </div>
    </div>
  );
}

function Field({ label, hint, error, children }: { label: string; hint?: string; error: string | null; children: React.ReactNode }) {
  return (
    <label className="field" data-invalid={error ? true : undefined}>
      <span className="field-label">{label}</span>
      {children}
      {error ? <span className="field-error" role="alert">{error}</span> : hint ? <span className="field-hint">{hint}</span> : null}
    </label>
  );
}
