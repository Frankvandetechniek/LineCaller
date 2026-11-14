// Two slim dBFS meters per strip: R = level received from the caller,
// T = level sent to the caller. Drawn by the shared loop in lib/levels.ts.

import { memo, useEffect, useRef } from "react";
import { registerMeter } from "../lib/levels";

function Meter({ line, kind }: { line: number; kind: "rx" | "tx" }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => (ref.current ? registerMeter(ref.current, line, kind) : undefined), [line, kind]);
  const name = kind === "rx" ? "Receiving level" : "Transmitting level";
  return (
    <div className="meter" ref={ref} data-live="false" title={`${name} (dBFS)`}>
      <span className="meter-tag" aria-hidden="true">{kind === "rx" ? "R" : "T"}</span>
      <div className="meter-track">
        <div className="meter-fill" />
        <div className="meter-peak" />
      </div>
    </div>
  );
}

export const LevelMeters = memo(function LevelMeters({ line }: { line: number }) {
  return (
    <div className="meters" aria-hidden="true">
      <Meter line={line} kind="rx" />
      <Meter line={line} kind="tx" />
    </div>
  );
});
