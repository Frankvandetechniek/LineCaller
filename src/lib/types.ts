// Shapes of the data the Rust backend sends. Keep in sync with
// crates/rlp/src/message.rs and crates/linecaller-core/src/*.rs.

export type Audio = "hold" | "pretalk" | "onAir1" | "onAir2" | "unknown";
export type SendableAudio = Exclude<Audio, "unknown">;

export type LineState =
  | { kind: "disconnected" }
  | { kind: "reserved" }
  | { kind: "ringing" }
  | { kind: "calling" }
  | { kind: "connected" }
  | { kind: "undefined"; value: number };

export interface Screening {
  phoneNumber: string;
  name: string;
  firstName: string;
  city: string;
}

export interface LineStatus {
  /** Zero-based line index, exactly as on the wire. Line 1 = 0. */
  line: number;
  state: LineState;
  audio: Audio;
  /** Remote Light location that occupies the line; 0 = none. */
  location: number;
  audioLocked: boolean;
  lineLocked: boolean;
  screening: Screening;
}

export interface LineView {
  status: LineStatus;
  updatedAtMs: number;
  stateSinceMs: number;
  stale: boolean;
}

export type ConnectionStatus = {
  endpoint: string;
  lastError: string | null;
} & (
  | { phase: "offline" }
  | { phase: "connecting"; attempt: number }
  | { phase: "connected"; sinceMs: number }
  | { phase: "retrying"; attempt: number; retryAtMs: number }
);

export type Theme = "dark" | "light" | "system";
export type Density = "comfortable" | "compact";

export interface Settings {
  host: string;
  port: number;
  location: number;
  lineCount: number;
  theme: Theme;
  density: Density;
  lineLabels: string[];
  defaultAnswerAudio: SendableAudio;
  autoConnect: boolean;
  simulator: boolean;
  livenessCheck: boolean;
  livenessIntervalSecs: number;
  /** Preference: outline the whole strip in the status colour, or only colour the status line on top. */
  stripOutline: boolean;
  /** Preference: Remote Light conference mode for ON-AIR A/B. Off: a line already on the bus goes to HOLD. */
  conferenceMode: boolean;
  /** Strip colour per line (index 0 = Line 1): "" = default, or "#rrggbb". */
  lineColors: string[];
}

export interface RecentCall {
  number: string;
  lineNumber: number;
  atMs: number;
}

export interface Bootstrap {
  settings: Settings;
  connection: ConnectionStatus;
  lines: (LineView | null)[];
  recents: RecentCall[];
  connectRequested: boolean;
  version: string;
}

export interface AppError {
  code: string;
  message: string;
  field?: string;
}

export const MAX_LINES = 16;
export const MAX_LOCATIONS = 6;
