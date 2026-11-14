// One telephone line drawn as a vertical channel strip, like a fader module on
// a mixing console: number and state at the top, a "scribble strip" display
// with the caller and the audio destination buttons; at the bottom ANSWER,
// DROP/REJECT and a half-height LOCK.

import { memo, useEffect, useState, type CSSProperties } from "react";
import type { LineView, SendableAudio } from "../lib/types";
import {
  AUDIO_BUTTONS, audioButtonAction, audioLabel, callerName, canDrop, canLockLine,
  formatDuration, isOnAir, linePhase, type AudioAction, type Phase,
} from "../lib/lineLogic";
import { LevelMeters } from "./LevelMeters";
import {
  AudioIcon, IconAlert, IconBell, IconBlock, IconFlask, IconLock, IconMapPin, IconPhone,
  IconPhoneIncoming, IconPhoneOff, IconPhoneOutgoing, IconUnlock,
} from "./Icons";

export interface ChannelStripProps {
  index: number;
  view: LineView | null | undefined;
  label: string;
  /** Strip colour ("#rrggbb") or "" for the default. */
  color: string;
  myLocation: number;
  defaultAnswerAudio: SendableAudio;
  connected: boolean;
  now: number;
  /** Name of the control waiting for a status update, if any. */
  pending: string | null;
  simulator: boolean;
  onAudio: (index: number, action: AudioAction) => void;
  onDrop: (index: number) => void;
  /** LOCK button: Set Lock Line (block incoming calls). */
  onLockLine: (index: number, locked: boolean) => void;
  onSimulateCall: (index: number) => void;
}

const PHASE_TEXT: Record<Phase, string> = {
  unknown: "NO STATUS",
  stale: "UNKNOWN",
  elsewhere: "IN USE",
  idle: "FREE",
  ringing: "INCOMING",
  calling: "CALLING",
  connected: "CONNECTED",
  reserved: "RESERVED",
  undefined: "UNRECOGNISED",
};

const PHASE_LONG: Record<Phase, string> = {
  unknown: "no status yet",
  stale: "status unknown, not connected",
  elsewhere: "in use by another location",
  idle: "free",
  ringing: "incoming call",
  calling: "outgoing call, calling",
  connected: "connected",
  reserved: "reserved",
  undefined: "unrecognised state",
};

function PhaseIcon({ phase }: { phase: Phase }) {
  switch (phase) {
    case "ringing": return <IconBell size={14} />;
    case "calling": return <IconPhoneOutgoing size={14} />;
    case "connected": return <IconPhone size={14} />;
    case "reserved": return <IconLock size={14} />;
    case "elsewhere": return <IconBlock size={14} />;
    case "stale": case "undefined": return <IconAlert size={14} />;
    default: return <span className="dot" aria-hidden="true" />;
  }
}

function rowLabel(phase: Phase): string {
  switch (phase) {
    case "idle": return "Dial out to";
    case "ringing": return "Answer onto";
    default: return "Audio";
  }
}

function ChannelStripImpl(p: ChannelStripProps) {
  const { index, view, myLocation, connected, now } = p;
  const phase = linePhase(view, myLocation);
  const st = view?.status;
  const fresh = !!view && !view.stale;
  const onAir = isOnAir(view) && phase !== "elsewhere";
  const name = callerName(view);
  const number = st?.screening.phoneNumber ?? "";
  const city = st?.screening.city ?? "";
  const busy = phase === "ringing" || phase === "calling" || phase === "connected" || phase === "reserved";
  const showCaller = busy || phase === "elsewhere" || (phase === "stale" && st?.state.kind !== "disconnected");
  const lineLocked = fresh && !!st?.lineLocked;
  const lineNo = index + 1;
  const titleId = `line-${index}-title`;
  const elapsed = formatDuration(now - (view?.stateSinceMs ?? now));

  // Two-step DROP: first press arms, second confirms, auto-disarms after 4 s.
  // REJECT on a ringing line acts on the first press.
  const [armed, setArmed] = useState(false);
  useEffect(() => {
    if (!armed) return;
    const t = setTimeout(() => setArmed(false), 4000);
    return () => clearTimeout(t);
  }, [armed]);
  useEffect(() => { if (!busy) setArmed(false); }, [busy]);

  return (
    <section
      className="strip"
      id={`strip-${index}`}
      data-phase={phase}
      data-onair={onAir || undefined}
      data-armed={armed || undefined}
      data-locked={lineLocked || undefined}
      data-colored={p.color ? true : undefined}
      style={p.color ? ({ ["--line-color" as string]: p.color } as CSSProperties) : undefined}
      aria-labelledby={titleId}
      aria-busy={p.pending ? true : undefined}
    >
      {/* ---- head: number, label, state ---- */}
      <header className="strip-head">
        <div className="strip-tally" aria-hidden="true">{onAir ? `ON AIR ${st!.audio === "onAir1" ? "A" : "B"}` : " "}</div>
        <div className="strip-num" aria-hidden="true">{lineNo}</div>
        <h2 id={titleId} className="strip-title">
          <span className="sr-only">Line {lineNo}{p.label ? `, ${p.label}` : ""}: {PHASE_LONG[phase]}</span>
          <span className="strip-label" aria-hidden="true" title={p.label || `Line ${lineNo}`}>{p.label || `Line ${lineNo}`}</span>
        </h2>
        <div className="badge" data-phase={phase} aria-hidden="true">
          <PhaseIcon phase={phase} />
          <span>{phase === "elsewhere" ? `LOC ${st!.location}` : PHASE_TEXT[phase]}</span>
        </div>
      </header>

      {/* ---- scribble strip ---- */}
      <div className="scribble">
        {showCaller ? (
          <>
            <div className="scr-number" title={number || name}>
              <span>{number || name || "Withheld"}</span>
            </div>
            {name && number && <div className="scr-name" title={name}>{name}</div>}
            {city && <div className="scr-city" title={city}><IconMapPin size={11} />{city}</div>}
            {!name && !city && <div className="scr-dim">No screening info</div>}
            <div className="scr-timer">
              <span aria-label={`for ${elapsed}`}>{elapsed}</span>
              {phase !== "elsewhere" && st && st.state.kind !== "ringing" && (
                <span className="scr-audio"><AudioIcon audio={st.audio} size={11} />{audioLabel(st.audio)}</span>
              )}
            </div>
          </>
        ) : (
          <div className="scr-empty">
            {phase === "idle" && <><span>Free</span><span className="scr-dim">{lineLocked ? "Incoming blocked" : "Ready"}</span></>}
            {phase === "unknown" && <><span>Waiting…</span><span className="scr-dim">{connected ? "Status requested" : "Not connected"}</span></>}
            {phase === "stale" && <><span>Unknown</span><span className="scr-dim">Reconnecting</span></>}
            {phase === "undefined" && <><span>State {st?.state.kind === "undefined" ? st.state.value : "?"}</span><span className="scr-dim">Unrecognised</span></>}
          </div>
        )}
      </div>

      {/* ---- R/T level meters ---- */}
      <LevelMeters line={index} />

      {/* ---- audio destinations ---- */}
      <div className="row-label" id={`line-${index}-audio`}>{rowLabel(phase)}</div>
      <div className="audio-stack" role="group" aria-labelledby={`line-${index}-audio ${titleId}`}>
        {AUDIO_BUTTONS.map((b) => {
          const action = audioButtonAction(view, b.audio, myLocation, connected);
          const current = busy && st?.audio === b.audio && phase !== "ringing";
          const disabled = action.type === "none" && !current;
          const hint =
            action.type === "openKeypad" ? `Dial out on line ${lineNo} to ${b.label}` :
            action.type === "accept" ? `Answer line ${lineNo} to ${b.label}` :
            action.type === "setAudio" ? `Move line ${lineNo} to ${b.label}` :
            action.reason;
          return (
            <button
              key={b.audio}
              type="button"
              className="audio-btn"
              data-audio={b.audio}
              data-current={current || undefined}
              data-pending={p.pending === b.audio || undefined}
              aria-pressed={busy && phase !== "ringing" ? current : undefined}
              aria-disabled={disabled || undefined}
              title={hint}
              onClick={() => { if (action.type !== "none") p.onAudio(index, action); }}
            >
              <span className="led" aria-hidden="true" />
              <span className="audio-text">{b.label}</span>
              {current && <span className="sr-only"> (current)</span>}
            </button>
          );
        })}
      </div>

      <div className="strip-spacer" />

      {p.simulator && phase === "idle" && !lineLocked && (
        <button type="button" className="sim-btn" onClick={() => p.onSimulateCall(index)} title="Simulator only: make this line ring">
          <IconFlask size={12} /> Ring
        </button>
      )}

      {/* ---- ANSWER right above REJECT ---- */}
      <button
        type="button"
        className="answer-btn"
        data-ringing={phase === "ringing" || undefined}
        disabled={phase !== "ringing" || !connected}
        onClick={() => p.onAudio(index, { type: "accept", audio: p.defaultAnswerAudio })}
        aria-label={`Answer line ${lineNo} to ${audioLabel(p.defaultAnswerAudio)}`}
        title={phase === "ringing" ? `Answer straight onto ${audioLabel(p.defaultAnswerAudio)}` : "No incoming call"}
      >
        <IconPhoneIncoming size={16} />
        <span>ANSWER</span>
      </button>

      {/* ---- DROP / REJECT ---- */}
      <button
        type="button"
        className="drop-btn"
        data-armed={armed || undefined}
        disabled={!canDrop(view, myLocation, connected)}
        data-pending={p.pending === "drop" || undefined}
        aria-describedby={armed ? `line-${index}-dropnote` : undefined}
        aria-label={armed ? `Confirm: ${phase === "ringing" ? "reject call" : "drop call"} on line ${lineNo}` : `${phase === "ringing" ? "Reject" : "Drop"} line ${lineNo}`}
        onClick={() => {
          // An incoming call is rejected at once; a call in progress needs a confirming second press.
          if (phase === "ringing") { setArmed(false); p.onDrop(index); return; }
          if (!armed) { setArmed(true); return; }
          setArmed(false);
          p.onDrop(index);
        }}
        onKeyDown={(e) => { if (e.key === "Escape" && armed) { e.stopPropagation(); setArmed(false); } }}
        onBlur={() => setArmed(false)}
      >
        <IconPhoneOff size={16} />
        <span>{armed ? "CONFIRM" : phase === "ringing" ? "REJECT" : "DROP"}</span>
      </button>
      {armed && (
        <div className="drop-note" id={`line-${index}-dropnote`} role="alert">
          Press again to {phase === "ringing" ? "reject" : "drop"}. Esc cancels.
        </div>
      )}

      {/* ---- LOCK = block incoming calls, last on the strip ---- */}
      <button
        type="button"
        className="lock-btn"
        aria-pressed={lineLocked}
        disabled={!canLockLine(view, myLocation, connected)}
        data-pending={p.pending === "lock" || undefined}
        aria-label={lineLocked ? `Line ${lineNo} locked, incoming calls blocked. Unlock` : `Lock line ${lineNo}: block incoming calls`}
        title={lineLocked
          ? "Line locked: incoming calls are rejected with busy. Outgoing calls are still possible. Press to unlock."
          : "Lock this line: reject incoming calls with busy (outgoing calls stay possible)"}
        onClick={() => p.onLockLine(index, !st?.lineLocked)}
      >
        {lineLocked ? <IconLock size={13} /> : <IconUnlock size={13} />}
        <span>{lineLocked ? "LOCKED" : "LOCK"}</span>
      </button>
    </section>
  );
}

export const ChannelStrip = memo(ChannelStripImpl);
