# Remote Light Protocol: implementation notes

LineCaller implements the **AVT Remote Light Protocol, version 1.0 (10.10.17)** for MAGIC THipPro.
This file records how each part of the spec maps to code, what was verified and how, and every
place where LineCaller had to make an assumption because the spec is silent.

The AVT THipPro Command Protocol was used only as background. Nothing from it is implemented.

## Spec-to-code checklist

Every row was checked byte for byte against the spec and is covered by a unit test in
`crates/rlp/src/message.rs` or `crates/rlp/src/frame.rs`.

| Spec section | Payload layout (hex code first) | Code | Used by LineCaller UI |
|---|---|---|---|
| §3 Packet | `FE` `01` `len` `payload…` `FD` | `frame::encode_packet`, `FrameDecoder` | all traffic |
| §4.2.1 Dial / Accept | `01` line audio location number… `00` | `Command::Dial` | keypad CALL; Answer (empty number) |
| §4.2.2 Drop | `02` line location | `Command::Drop` | DROP / REJECT |
| §4.2.3 Set Audio | `03` line audio location | `Command::SetAudio` | PRETALK / HOLD / ON-AIR A / ON-AIR B on a busy line |
| §4.2.4 Set Lock Audio | `04` line lock location | `Command::SetLockAudio` | small "Audio lock" toggle |
| §4.2.5 Set Lock Line | `05` line lock location | `Command::SetLockLine` | **LOCK** (blocks incoming calls) |
| §4.2.6 Status Request | `06` line | `Command::StatusRequest` | connect sync, Refresh, liveness probe |
| §4.2.7 Set Conference | `07` location on/off | `Command::SetConference` | encoded and tested, **not exposed** |
| §4.2.8 Set Next Caller | `08` audio location | `Command::SetNextCaller` | encoded and tested, **not exposed** |
| §4.3.1 Line Status | `40` line state audio location lockAudio lockLine phone`00` name`00` first`00` city`00` | `LineStatus::from_payload` | every line card |

Value mappings:

* **Line** is zero-based on the wire: `0` = Line 1. The Rust type `LineIndex` and the Tauri commands
  (`lineIndex`) keep it zero-based; only the UI adds one for display.
* **Audio**: `0` HOLD, `1` PRETALK, `2` ON AIR 1 (shown as **ON-AIR A**), `3` ON AIR 2 (shown as
  **ON-AIR B**, THipPro only), `4` UNKNOWN (status only, never sent).
* **Line State**: `0` Disconnected, `1` Reserved, `2` Ringing (incoming), `3` Calling (outgoing),
  `4` Connected.
* **Location**: `1…6` for MAGIC THipPro (spec §2: six parallel third-party connections). `0` means
  "none" and is never sent by LineCaller. Settings reject anything outside 1–6.

## Behaviour that follows directly from the spec

* **Accepting a call** uses Dial (`01`) with an empty, zero-terminated number. §4.2.1 says the
  number is ignored and the call accepted when a call is pending.
* **Line ownership** (§4.1): when a Line Status reports a location other than 0 or ours, the line
  belongs to another console. LineCaller shows "In use by location N", disables its controls and
  the backend refuses to send commands for it.
* **LOCK = lock line**: the visible LOCK button sends Set Lock Line (§4.2.5): incoming calls on that
  line are rejected with busy, outgoing calls stay possible. It can be used on a free line. Set Lock
  Audio (§4.2.4) is a separate, smaller "Audio lock" toggle. The two are never combined.
* **Status after reconnect**: on every (re)connect LineCaller sends a Status Request for each
  displayed line, since §4.3.1 only pushes status on change.

## Assumptions (not specified by the Remote Light Protocol)

Each assumption is isolated in one place so it can be changed without touching the rest.

| # | Topic | What LineCaller does | Where |
|---|---|---|---|
| A1 | Recovery from invalid bytes | The spec only says invalid packets must not be processed. The decoder drops bytes before a `FE`; on a bad version or end flag it drops just that `FE` and resynchronises on the next one, so one corrupt length byte cannot swallow the next valid packet. Buffer is bounded. | `crates/rlp/src/frame.rs` |
| A2 | Short or odd Line Status | The 7 fixed bytes are required. Missing trailing screening strings are treated as empty; the last string may lack its `00`. Unknown state values are kept and shown as "Unrecognised"; unknown audio values map to UNKNOWN. Line indices ≥ 16 are rejected. | `LineStatus::from_payload` |
| A3 | Character set | Spec says ASCII. Non-ASCII bytes in screening info are decoded as Latin-1 instead of rejecting the packet. Dialled numbers are limited to `0-9 * # +`, max 64 characters. | `message.rs` |
| A4 | No acknowledgement | The protocol has no ACK/NAK. A command counts as sent once written to the socket; the UI shows a pending indicator until the next Line Status for that line, and warns after 4 s if none arrives. | `ClientHandle::send`, `App.tsx` |
| A5 | Liveness | The protocol has no heartbeat. Optional (on by default, 20 s): when nothing has been received for the interval, LineCaller sends a Status Request for Line 1 and reconnects if nothing comes back within 5 s. TCP keep-alive is also enabled. Turn it off in Settings if a THipPro does not answer status requests. | `connection.rs` |
| A6 | Timeouts and back-off | Connect timeout 5 s, write timeout 3 s, reconnect back-off 0.5 / 1 / 2 / 5 / 10 / 15 s. | `connection.rs` |
| A7 | Port per location | The spec leaves ports to the THipPro configuration. The configuration guide's defaults are 10100, 10101, 10102… per remote device; LineCaller defaults to 10100 and lets the user set host, port and location independently. It does not derive the port from the location. | `settings.rs` |
| A8 | Busy-line guard | The backend refuses Dial on a line whose last status is not Disconnected and Set Audio on an idle or ringing line, so a second outgoing call can never be started on a busy line. | `src-tauri/src/commands.rs` |
| A9 | Call timer | The protocol carries no timestamps. Timers count from when LineCaller received the state change. | `store.rs` |

## What was verified, and how

* **Byte layouts**: unit tests compare every command and the Line Status against hand-written
  byte arrays taken from the spec tables.
* **Stream handling**: tests feed packets one byte at a time, several packets per read, payloads
  that contain `FE`/`FD`, leading garbage, wrong versions, wrong end flags and endless junk.
* **Connection manager**: integration tests (`crates/linecaller-core/tests/connection.rs`) run
  against scripted TCP peers: initial status sync, fragmented reads, invalid packets, peer
  disconnect and reconnect, stale marking, the liveness timeout, connection refused, and a full
  ring → answer → lock → drop flow against the simulator.
* **Desktop app**: the Tauri app was built and run on Linux against the built-in simulator; ring,
  answer, move to ON-AIR A, keypad dial and two-step drop were exercised through the real UI,
  commands and capabilities.

## Not verified against a live THipPro

No THipPro was available while building LineCaller. These points should be checked on a real
system before production use:

1. That the THipPro answers a Status Request for every line index up to the configured line count
   (and what it does for lines that do not exist on the unit).
2. Whether the THipPro sends Line Status unprompted after LineCaller's own commands
   (assumed, per §4.3.1 "any relevant information changes").
3. How the THipPro reacts to Set Audio on a Reserved or Calling line, and to Drop on a Ringing
   line (LineCaller allows both; the latter is labelled REJECT).
4. Whether the liveness probe (A5) is acceptable at the chosen interval.
5. Character encoding of screening names with accents (A3).
6. Behaviour when two LineCaller installations use the same location.

The simulator's hybrid behaviour (audio replacement, ownership, line lock) is a simplified reading
of the spec, not a model of a real THipPro.
