// Code entry shown before the Settings window opens. The code itself and the
// check live in lib/settingsLock.ts.

import { useEffect, useRef, useState } from "react";
import { SETTINGS_PIN_LENGTH, checkSettingsPin } from "../lib/settingsLock";
import { IconBackspace, IconLock, IconX } from "./Icons";
import { useDialog } from "./useDialog";

const DIGITS = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

export function SettingsLock(p: { onUnlock: () => void; onClose: () => void }) {
  const [entry, setEntry] = useState("");
  const [wrong, setWrong] = useState(0); // counts failed attempts; >0 shows the error
  const dialogRef = useDialog(p.onClose);
  const firstKey = useRef<HTMLButtonElement>(null);
  const unlock = useRef(p.onUnlock);
  unlock.current = p.onUnlock;

  useEffect(() => { firstKey.current?.focus(); }, []);

  // Check as soon as all digits are in; a wrong code clears the entry.
  useEffect(() => {
    if (entry.length < SETTINGS_PIN_LENGTH) return;
    if (checkSettingsPin(entry)) { unlock.current(); return; }
    setWrong((n) => n + 1);
    setEntry("");
  }, [entry]);

  const press = (d: string) => setEntry((e) => (e.length < SETTINGS_PIN_LENGTH ? e + d : e));
  const back = () => setEntry((e) => e.slice(0, -1));

  // Typing digits works wherever focus is inside the dialog.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      if (/^[0-9]$/.test(e.key)) { e.preventDefault(); press(e.key); }
      else if (e.key === "Backspace") { e.preventDefault(); back(); }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="scrim" onMouseDown={(e) => { if (e.target === e.currentTarget) p.onClose(); }}>
      <div ref={dialogRef} className="dialog pin" role="dialog" aria-modal="true" aria-labelledby="pin-title" aria-describedby="pin-hint">
        <header className="dialog-head">
          <div>
            <div className="eyebrow"><IconLock size={14} /> Settings locked</div>
            <h2 id="pin-title">Enter code</h2>
          </div>
          <button type="button" className="icon-btn" onClick={p.onClose} aria-label="Cancel"><IconX /></button>
        </header>

        <p id="pin-hint" className="muted pin-hint">Enter the {SETTINGS_PIN_LENGTH}-digit code to open Settings.</p>

        <div className="pin-dots" data-wrong={wrong || undefined} key={wrong} aria-hidden="true">
          {Array.from({ length: SETTINGS_PIN_LENGTH }, (_, i) => (
            <span key={i} className="pin-dot" data-filled={i < entry.length || undefined} />
          ))}
        </div>
        <p className="sr-only" aria-live="polite">{entry.length} of {SETTINGS_PIN_LENGTH} digits entered</p>
        <p className="pin-error" role="alert">{wrong > 0 ? "Wrong code. Try again." : ""}</p>

        <div className="keys" role="group" aria-label="Code keypad">
          {DIGITS.map((d, i) => (
            <button key={d} ref={i === 0 ? firstKey : undefined} type="button" className="key" onClick={() => press(d)}>
              <span className="key-main">{d}</span>
            </button>
          ))}
          <span aria-hidden="true" />
          <button type="button" className="key" onClick={() => press("0")}><span className="key-main">0</span></button>
          <button type="button" className="key" onClick={back} disabled={!entry} aria-label="Delete last digit"><IconBackspace /></button>
        </div>
      </div>
    </div>
  );
}
