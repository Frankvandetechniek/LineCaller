import { memo, useEffect, useRef, useState } from "react";
import type { LineView, SendableAudio } from "../lib/types";
import {
  AUDIO_BUTTONS, audioButtonAction, audioLabel, callerName, canBlockIncoming, canDrop, canLockAudio,
  formatDuration, isOnAir, linePhase, type AudioAction, type Phase,
} from "../lib/lineLogic";
import {
  AudioIcon, IconAlert, IconBell, IconBlock, IconClock, IconFlask, IconLock, IconMapPin, IconPhone,
  IconPhoneIncoming, IconPhoneOff, IconPhoneOutgoing, IconUnlock,
} from "./Icons";

export interface LineCardProps {
  index: number;
  view: LineView | null | undefined;
  label: string;
  myLocation: number;
  defaultAnswerAudio: SendableAudio;
  connected: boolean;
  now: number;
  /** Name of the control waiting for a status update, if any. */
  pending: string | null;
  simulator: boolean;
  onAudio: (index: number, action: AudioAction) => void;
  onDrop: (index: number) => void;
  onLockAudio: (index: number, locked: boolean) => void;
  onBlockIncoming: (index: number, blocked: boolean) => void;
  onSimulateCall: (index: number) => void;
}

const PHASE_TEXT: Record<Phase, string> = {
  unknown: "NO STATUS",
  stale: "STATUS UNKNOWN",
  elsewhere: "IN USE ELSEWHERE",
  idle: "FREE",
  ringing: "INCOMING CALL",
  calling: "CALLING OUT",
  connected: "CONNECTED",
  reserved: "RESERVED",
  undefined: "UNRECOGNISED",
};

function PhaseIcon({ phase }: { phase: Phase }) {
  switch (phase) {
    case "ringing": return <IconBell size={15} />;
    case "calling": return <IconPhoneOutgoing size={15} />;
    case "connected": return <IconPhone size={15} />;
    case "reserved": return <IconLock size={15} />;
    case "elsewhere": return <IconBlock size={15} />;
    case "stale": case "undefined": return <IconAlert size={15} />;
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

function formatNumber(n: string): string {
  return n || "";
}

function LineCardImpl(p: LineCardProps) {
  const { index, view, myLocation, connected, now } = p;
  const phase = linePhase(view, myLocation);
  const st = view?.status;
  const onAir = isOnAir(view) && phase !== "elsewhere";
  const name = callerName(view);
  const number = st?.screening.phoneNumber ?? "";
  const city = st?.screening.city ?? "";
  const busy = phase === "ringing" || phase === "calling" || phase === "connected" || phase === "reserved";
  const showCaller = busy || phase === "elsewhere" || (phase === "stale" && st?.state.kind !== "disconnected");

  // Two-step DROP: first press arms, second confirms, auto-disarms after 4 s.
  const [armed, setArmed] = useState(false);
  const dropRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!armed) return;
    const t = setTimeout(() => setArmed(false), 4000);
    return () => clearTimeout(t);
  }, [armed]);
  useEffect(() => { if (!busy) setArmed(false); }, [busy]);

  const dropEnabled = canDrop(view, myLocation, connected);
  const lockEnabled = canLockAudio(view, myLocation, connected);
  const blockEnabled = canBlockIncoming(view, myLocation, connected);
  const lineNo = index + 1;
  const titleId = `line-${index}-title`;

  return (
    <article
      className="line"
      data-phase={phase}
      data-onair={onAir || undefined}
      data-armed={armed || undefined}
      aria-labelledby={titleId}
      aria-busy={p.pending ? true : undefined}
    >
      {onAir && (
        <div className="onair-bar" aria-hidden="true">
          <IconPhone size={12} /> ON AIR · {audioLabel(st!.audio)}
        </div>
      )}
      <header className="line-head">
        <div className="line-num" aria-hidden="true">{lineNo}</div>
        <div className="line-title">
          <h2 id={titleId}>
            Line {lineNo}
            {p.label && <span className="sr-only">, {p.label}</span>}
          </h2>
          {p.label && <div className="line-label" aria-hidden="true">{p.label}</div>}
          <div className="line-flags">
            {st?.audioLocked && !view?.stale && <span className="flag flag-lock"><IconLock size={12} /> Audio locked</span>}
            {st?.lineLocked && !view?.stale && <span className="flag flag-block"><IconBlock size={12} /> Incoming blocked</span>}
          </div>
        </div>
        <div className="badge" data-phase={phase} role="status">
          <PhaseIcon phase={phase} />
          <span>{phase === "elsewhere" ? `LOCATION ${st!.location}` : PHASE_TEXT[phase]}</span>
        </div>
      </header>

      <div className="caller">
        {showCaller ? (
          <>
            <div className="caller-number" title={number}>
              {phase === "ringing" && <IconPhoneIncoming size={20} />}
              {phase === "calling" && <IconPhoneOutgoing size={20} />}
              <span>{formatNumber(number) || name || "Number withheld"}</span>
            </div>
            <div className="caller-meta">
              {name && number && <span><span className="sr-only">Name: </span>{name}</span>}
              {city && <span className="meta-city"><IconMapPin size={13} />{city}</span>}
              {!name && !city && <span className="muted">No screening information</span>}
            </div>
            <div className="caller-timer" aria-label={`${PHASE_TEXT[phase].toLowerCase()} for ${formatDuration(now - (view?.stateSinceMs ?? now))}`}>
              <IconClock size={13} />
              <span>{formatDuration(now - (view?.stateSinceMs ?? now))}</span>
              {phase !== "elsewhere" && st && st.state.kind !== "ringing" && (
                <span className="caller-audio">
                  <AudioIcon audio={st.audio} size={13} /> {audioLabel(st.audio)}
                </span>
              )}
            </div>
          </>
        ) : (
          <div className="caller-empty">
            {phase === "idle" && <><span>Line free</span><span className="muted">Pick a destination to dial out</span></>}
            {phase === "unknown" && <><span>Waiting for status…</span><span className="muted">{connected ? "Requested from THipPro" : "Not connected"}</span></>}
            {phase === "stale" && <><span>Status unknown</span><span className="muted">Reconnecting to THipPro</span></>}
            {phase === "undefined" && <><span>Unrecognised line state</span><span className="muted">Value {st?.state.kind === "undefined" ? st.state.value : "?"}</span></>}
          </div>
        )}
        {phase === "elsewhere" && (
          <p className="elsewhere-note">Controlled by Remote Light location {st!.location}. Only that location can change this line.</p>
        )}
      </div>

      {phase === "ringing" && (
        <button
          type="button"
          className="answer-btn"
          disabled={!connected}
          onClick={() => p.onAudio(index, { type: "accept", audio: p.defaultAnswerAudio })}
        >
          <IconPhoneIncoming size={20} />
          <span>Answer to <strong>{audioLabel(p.defaultAnswerAudio)}</strong></span>
        </button>
      )}

      <div className="row-label" id={`line-${index}-audio`}>{rowLabel(phase)}</div>
      <div className="audio-row" role="group" aria-labelledby={`line-${index}-audio ${titleId}`}>
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
              <AudioIcon audio={b.audio} size={16} />
              <span className="audio-text">{b.label}</span>
              {current && <span className="sr-only"> (current)</span>}
            </button>
          );
        })}
      </div>

      <footer className="line-foot">
        <button
          type="button"
          className="toggle-btn lock-btn"
          aria-pressed={!!st?.audioLocked && !view?.stale}
          disabled={!lockEnabled}
          data-pending={p.pending === "lock" || undefined}
          title="LOCK keeps this line on its current audio when other lines are put on air (Set Lock Audio)"
          onClick={() => p.onLockAudio(index, !st?.audioLocked)}
        >
          {st?.audioLocked ? <IconLock size={16} /> : <IconUnlock size={16} />}
          <span>{st?.audioLocked ? "LOCKED" : "LOCK"}</span>
        </button>
        <button
          ref={dropRef}
          type="button"
          className="drop-btn"
          data-armed={armed || undefined}
          disabled={!dropEnabled}
          data-pending={p.pending === "drop" || undefined}
          aria-describedby={armed ? `line-${index}-dropnote` : undefined}
          onClick={() => {
            if (!armed) { setArmed(true); return; }
            setArmed(false);
            p.onDrop(index);
          }}
          onKeyDown={(e) => { if (e.key === "Escape" && armed) { e.stopPropagation(); setArmed(false); } }}
          onBlur={() => setArmed(false)}
        >
          <IconPhoneOff size={16} />
          <span>{armed ? (phase === "ringing" ? "Confirm reject" : "Confirm drop") : (phase === "ringing" ? "REJECT" : "DROP")}</span>
        </button>
      </footer>
      <div className="line-extra">
        <button
          type="button"
          className="block-btn"
          aria-pressed={!!st?.lineLocked && !view?.stale}
          disabled={!blockEnabled}
          data-pending={p.pending === "block" || undefined}
          title="Reject new incoming calls on this line with busy (Set Lock Line). Outgoing calls stay possible."
          onClick={() => p.onBlockIncoming(index, !st?.lineLocked)}
        >
          <IconBlock size={14} />
          <span>{st?.lineLocked ? "Incoming calls blocked" : "Block incoming calls"}</span>
        </button>
        <span className="spacer" />
        {p.simulator && phase === "idle" && (
          <button type="button" className="ghost-btn sim-btn" onClick={() => p.onSimulateCall(index)} title="Simulator only: make this line ring">
            <IconFlask size={14} /><span>Ring</span>
          </button>
        )}
      </div>
      {armed && (
        <div className="drop-note" id={`line-${index}-dropnote`} role="alert">
          Press again to {phase === "ringing" ? "reject the call" : "end the call"} on line {lineNo}. Esc cancels.
        </div>
      )}
    </article>
  );
}

export const LineCard = memo(LineCardImpl);
