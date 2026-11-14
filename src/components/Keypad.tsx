import { useEffect, useRef, useState } from "react";
import type { RecentCall, SendableAudio } from "../lib/types";
import { AUDIO_BUTTONS, MAX_DIAL_LEN, audioLabel, sanitizeNumber, timeAgo } from "../lib/lineLogic";
import { AudioIcon, IconBackspace, IconClock, IconPhoneOutgoing, IconX } from "./Icons";
import { useDialog } from "./useDialog";

const KEYS: [string, string][] = [
  ["1", ""], ["2", "ABC"], ["3", "DEF"],
  ["4", "GHI"], ["5", "JKL"], ["6", "MNO"],
  ["7", "PQRS"], ["8", "TUV"], ["9", "WXYZ"],
  ["*", ""], ["0", "+"], ["#", ""],
];

export interface KeypadProps {
  lineIndex: number;
  lineLabel: string;
  initialAudio: SendableAudio;
  recents: RecentCall[];
  now: number;
  busy: boolean;
  busyReason: string;
  onCall: (lineIndex: number, audio: SendableAudio, number: string) => void;
  onClose: () => void;
  onClearRecents: () => void;
}

export function Keypad(p: KeypadProps) {
  const [number, setNumber] = useState("");
  const [audio, setAudio] = useState<SendableAudio>(p.initialAudio);
  const [showRecents, setShowRecents] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const dialogRef = useDialog(p.onClose);
  const lineNo = p.lineIndex + 1;

  useEffect(() => { inputRef.current?.focus(); }, []);

  const press = (k: string) => {
    setNumber((n) => sanitizeNumber(n + k));
    inputRef.current?.focus();
  };
  const call = () => {
    if (number && !p.busy) p.onCall(p.lineIndex, audio, number);
  };

  return (
    <div className="scrim" onMouseDown={(e) => { if (e.target === e.currentTarget) p.onClose(); }}>
      <div ref={dialogRef} className="dialog keypad" role="dialog" aria-modal="true" aria-labelledby="keypad-title">
        <header className="dialog-head">
          <div>
            <div className="eyebrow"><IconPhoneOutgoing size={14} /> Outgoing call</div>
            <h2 id="keypad-title">Dial out on Line {lineNo}{p.lineLabel && <span className="muted"> · {p.lineLabel}</span>}</h2>
          </div>
          <button type="button" className="icon-btn" onClick={p.onClose} aria-label="Close keypad"><IconX /></button>
        </header>

        <fieldset className="dest">
          <legend>Connect to</legend>
          <div className="seg">
            {AUDIO_BUTTONS.map((b) => (
              <label key={b.audio} className="seg-opt" data-audio={b.audio}>
                <input type="radio" name="dest" value={b.audio} checked={audio === b.audio} onChange={() => setAudio(b.audio)} />
                <span><AudioIcon audio={b.audio} size={14} />{b.label}</span>
              </label>
            ))}
          </div>
        </fieldset>

        {p.busy && p.busyReason && <p className="keypad-warning" role="alert">{p.busyReason} CALL is disabled.</p>}

        {!showRecents ? (
          <>
            <div className="number-display">
              <label htmlFor="dial-number" className="sr-only">Number to call</label>
              <input
                id="dial-number"
                ref={inputRef}
                value={number}
                inputMode="tel"
                autoComplete="off"
                spellCheck={false}
                maxLength={MAX_DIAL_LEN}
                placeholder="Enter number"
                onChange={(e) => setNumber(sanitizeNumber(e.target.value))}
                onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); call(); } }}
              />
              <button type="button" className="icon-btn" aria-label="Delete last digit" disabled={!number}
                onClick={() => { setNumber((n) => n.slice(0, -1)); inputRef.current?.focus(); }}>
                <IconBackspace />
              </button>
            </div>
            <div className="keys" role="group" aria-label="Dial pad">
              {KEYS.map(([k, sub]) => (
                <button key={k} type="button" className="key" onClick={() => press(k)} aria-label={k === "*" ? "star" : k === "#" ? "hash" : k}>
                  <span className="key-main">{k}</span>
                  <span className="key-sub" aria-hidden="true">{sub}</span>
                </button>
              ))}
            </div>
          </>
        ) : (
          <div className="recents" role="region" aria-label="Recent numbers">
            {p.recents.length === 0 ? (
              <p className="muted empty">No numbers dialled from this LineCaller yet.</p>
            ) : (
              <ul>
                {p.recents.map((r) => (
                  <li key={r.number}>
                    <button type="button" className="recent" onClick={() => { setNumber(r.number); setShowRecents(false); setTimeout(() => inputRef.current?.focus()); }}>
                      <span className="recent-number">{r.number}</span>
                      <span className="recent-meta">Line {r.lineNumber} · {timeAgo(r.atMs, p.now)}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
            {p.recents.length > 0 && (
              <button type="button" className="ghost-btn" onClick={p.onClearRecents}>Clear recents</button>
            )}
          </div>
        )}

        <div className="keypad-actions">
          <button type="button" className="secondary-btn" aria-pressed={showRecents} onClick={() => setShowRecents((s) => !s)}>
            <IconClock size={16} /> {showRecents ? "KEYPAD" : "RECENTS"}
          </button>
          <button type="button" className="secondary-btn" onClick={() => { setNumber(""); inputRef.current?.focus(); }} disabled={!number}>
            CLEAR
          </button>
          <button type="button" className="call-btn" onClick={call} disabled={!number || p.busy}>
            <IconPhoneOutgoing size={18} />
            <span>CALL <span className="call-dest">→ {audioLabel(audio)}</span></span>
          </button>
        </div>
      </div>
    </div>
  );
}
