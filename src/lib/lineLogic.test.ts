import { describe, expect, it } from "vitest";
import { audioButtonAction, canDrop, canLockAudio, canLockLine, formatDuration, linePhase, sanitizeNumber } from "./lineLogic";
import type { LineState, LineView } from "./types";

function view(kind: LineState["kind"], extra: Partial<LineView["status"]> = {}, stale = false): LineView {
  return {
    status: {
      line: 0,
      state: kind === "undefined" ? { kind, value: 9 } : ({ kind } as LineState),
      audio: "hold",
      location: 0,
      audioLocked: false,
      lineLocked: false,
      screening: { phoneNumber: "", name: "", firstName: "", city: "" },
      ...extra,
    },
    updatedAtMs: 0,
    stateSinceMs: 0,
    stale,
  };
}

describe("audio buttons", () => {
  it("open the keypad on an idle line with the chosen destination", () => {
    expect(audioButtonAction(view("disconnected"), "onAir2", 1, true)).toEqual({ type: "openKeypad", audio: "onAir2" });
  });
  it("accept a ringing call", () => {
    expect(audioButtonAction(view("ringing"), "pretalk", 1, true)).toEqual({ type: "accept", audio: "pretalk" });
  });
  it("change audio on a busy line and never open the keypad", () => {
    for (const k of ["connected", "calling", "reserved"] as const) {
      expect(audioButtonAction(view(k, { location: 1 }), "onAir1", 1, true)).toEqual({ type: "setAudio", audio: "onAir1" });
    }
  });
  it("do nothing for the current destination", () => {
    expect(audioButtonAction(view("connected", { audio: "pretalk", location: 1 }), "pretalk", 1, true).type).toBe("none");
  });
  it("are blocked for lines owned by another location, stale lines or when offline", () => {
    expect(audioButtonAction(view("connected", { location: 3 }), "hold", 1, true).type).toBe("none");
    expect(audioButtonAction(view("disconnected", {}, true), "hold", 1, true).type).toBe("none");
    expect(audioButtonAction(view("disconnected"), "hold", 1, false).type).toBe("none");
    expect(audioButtonAction(undefined, "hold", 1, true).type).toBe("none");
  });
});

describe("line rules", () => {
  it("phase reflects ownership", () => {
    expect(linePhase(view("connected", { location: 2 }), 1)).toBe("elsewhere");
    expect(linePhase(view("connected", { location: 1 }), 1)).toBe("connected");
    expect(linePhase(view("undefined"), 1)).toBe("undefined");
  });
  it("drop and lock availability", () => {
    expect(canDrop(view("disconnected"), 1, true)).toBe(false);
    expect(canDrop(view("ringing"), 1, true)).toBe(true);
    expect(canLockAudio(view("ringing"), 1, true)).toBe(false);
    expect(canLockAudio(view("connected", { location: 1 }), 1, true)).toBe(true);
    // LOCK (block incoming) works on a free line, not on one owned elsewhere or when stale
    expect(canLockLine(view("disconnected"), 1, true)).toBe(true);
    expect(canLockLine(view("connected", { location: 4 }), 1, true)).toBe(false);
    expect(canLockLine(view("disconnected", {}, true), 1, true)).toBe(false);
  });
  it("formats numbers and durations", () => {
    expect(sanitizeNumber(" +31 (20) 555-12*#a")).toBe("+312055512*#");
    expect(formatDuration(65_000)).toBe("1:05");
    expect(formatDuration(3_725_000)).toBe("1:02:05");
  });
});
