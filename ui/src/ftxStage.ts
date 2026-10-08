// Where a WSJT-X instance is in a QSO, worked out from the message it is sending.
// WSJT-X doesn't report its QSO state over UDP, but the Tx message gives it away.

export type StageKind = "idle" | "cq" | "call" | "report" | "rreport" | "rrr" | "rr73" | "73" | "free";

export interface Stage {
  kind: StageKind;
  /** Plain words for the strip, e.g. "Calling K1ABC". */
  text: string;
  /** The station the message is addressed to, when there is one. */
  dx: string | null;
}

export interface StageInput {
  tx_message: string;
  dx_call: string;
  /** The instance's own call (de_call), else the logging call. */
  mycall: string;
  tx_enabled: boolean;
  transmitting: boolean;
}

const bare = (c: string) => c.replace(/[<>]/g, "").toUpperCase();

/** Same station, allowing for a prefix or suffix on either call (PJ4/K1ABC, K1ABC/P). */
function sameCall(a: string, b: string): boolean {
  const x = bare(a);
  const y = bare(b);
  if (!x || !y) return false;
  if (x === y) return true;
  const base = (c: string) => c.split("/").reduce((l, p) => (p.length > l.length ? p : l), "");
  return base(x) === base(y);
}

/** Looks like a callsign: letters and digits, maybe a /, with at least one of each. */
const isCall = (c: string) => /^[A-Z0-9/]{3,13}$/.test(bare(c)) && /[0-9]/.test(c) && /[A-Z]/.test(bare(c));

const GRID = /^[A-R]{2}[0-9]{2}([A-X]{2})?$/;
const REPORT = /^[+-][0-9]{2}$/;

export function qsoStage(s: StageInput): Stage {
  const msg = s.tx_message.trim().toUpperCase();
  if ((!s.tx_enabled && !s.transmitting) || !msg) return { kind: "idle", text: "Idle", dx: null };
  const t = msg.split(/\s+/);
  const free: Stage = { kind: "free", text: "Sending free text", dx: null };
  if (t[0] === "CQ") {
    // CQ [target] MYCALL [grid]
    return t.slice(1).some((w) => sameCall(w, s.mycall)) || !s.mycall ? { kind: "cq", text: "Calling CQ", dx: null } : free;
  }
  if (t.length < 2 || t[0] === "<...>") return free;
  const [to, from, ...rest] = t;
  if (!isCall(to) || !isCall(from) || (s.mycall && !sameCall(from, s.mycall))) return free;
  const dx = bare(to);
  const end = rest.join(" ");
  if (rest.length === 0 || (rest.length === 1 && GRID.test(rest[0]) && rest[0] !== "RR73")) return { kind: "call", text: `Calling ${dx}`, dx };
  if (rest.length === 1 && REPORT.test(end)) return { kind: "report", text: "Sending report", dx };
  if (rest.length === 1 && end.startsWith("R") && REPORT.test(end.slice(1))) return { kind: "rreport", text: "Sending R+report", dx };
  if (rest.length === 2 && rest[0] === "R" && GRID.test(rest[1])) return { kind: "rreport", text: "Sending R+grid", dx };
  if (end === "RRR") return { kind: "rrr", text: "Sending RRR", dx };
  if (end === "RR73") return { kind: "rr73", text: "Sending RR73", dx };
  if (end === "73") return { kind: "73", text: "Sending 73", dx };
  return { kind: "free", text: "Sending free text", dx: sameCall(to, s.dx_call) ? dx : null };
}
