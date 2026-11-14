// Colour swatch for one line in Settings → Line labels. Clicking the swatch
// opens a small overlay with preset colours, a custom colour picker and
// "Default" (no colour).

import { useEffect, useRef, useState } from "react";

export const LINE_COLOR_PRESETS = [
  "#e5484d", "#f76b15", "#ffc53d", "#46a758", "#12a594", "#00a2c7",
  "#3e63dd", "#6e56cf", "#d6409f", "#8d8d8d", "#a18072", "#ffffff",
];

export function LineColorPicker(p: { lineNumber: number; value: string; onChange: (v: string) => void }) {
  const [open, setOpen] = useState(false);
  const wrap = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (wrap.current && !wrap.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const pick = (v: string) => {
    p.onChange(v);
    setOpen(false);
    button.current?.focus();
  };
  const label = p.value ? p.value.toUpperCase() : "default";

  return (
    <div className="color-pick" ref={wrap}>
      <button
        ref={button}
        type="button"
        className="color-swatch"
        data-empty={!p.value || undefined}
        style={p.value ? { background: p.value } : undefined}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={`Colour for line ${p.lineNumber}: ${label}`}
        title={`Strip colour: ${label}`}
        onClick={() => setOpen((o) => !o)}
      />
      {open && (
        <div
          className="color-pop"
          role="dialog"
          aria-label={`Colour for line ${p.lineNumber}`}
          onKeyDown={(e) => {
            if (e.key === "Escape") { e.stopPropagation(); setOpen(false); button.current?.focus(); }
          }}
        >
          <div className="color-grid">
            {LINE_COLOR_PRESETS.map((c) => (
              <button
                key={c}
                type="button"
                className="color-cell"
                style={{ background: c }}
                aria-label={c.toUpperCase()}
                aria-pressed={p.value === c}
                onClick={() => pick(c)}
                autoFocus={p.value === c || (!p.value && c === LINE_COLOR_PRESETS[0])}
              />
            ))}
          </div>
          <div className="color-actions">
            <label className="color-custom">
              <input
                type="color"
                value={p.value || "#3e63dd"}
                onChange={(e) => p.onChange(e.target.value.toLowerCase())}
              />
              <span>Custom…</span>
            </label>
            <button type="button" className="secondary-btn small" onClick={() => pick("")} disabled={!p.value}>
              Default
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
