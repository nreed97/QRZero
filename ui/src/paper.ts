import { OQRS, OQRS_DATE } from "./confirmations";
import type { Fields, Qso } from "./types";

const today = () => new Date().toISOString().slice(0, 10).replace(/-/g, "");

/** Bulk paper-QSL actions on selected QSOs, as the ADIF fields each one sets. */
export const OQRS_ACTIONS: { key: string; label: string; fields: () => Fields }[] = [
  { key: "oqrs", label: "Mark OQRS requested", fields: () => ({ [OQRS]: "Y", [OQRS_DATE]: today() }) },
  { key: "oqrs-clear", label: "Clear OQRS request", fields: () => ({ [OQRS]: "", [OQRS_DATE]: "" }) },
];

export const PAPER_ACTIONS: { key: string; label: string; fields: () => Fields }[] = [
  { key: "queue", label: "Queue a card to send", fields: () => ({ QSL_SENT: "Q" }) },
  { key: "sent-b", label: "Card sent via bureau", fields: () => ({ QSL_SENT: "Y", QSLSDATE: today(), QSL_SENT_VIA: "B" }) },
  { key: "sent-d", label: "Card sent direct", fields: () => ({ QSL_SENT: "Y", QSLSDATE: today(), QSL_SENT_VIA: "D" }) },
  { key: "rcvd-b", label: "Card received via bureau", fields: () => ({ QSL_RCVD: "Y", QSLRDATE: today(), QSL_RCVD_VIA: "B" }) },
  { key: "rcvd-d", label: "Card received direct", fields: () => ({ QSL_RCVD: "Y", QSLRDATE: today(), QSL_RCVD_VIA: "D" }) },
  { key: "none", label: "Not sending a card", fields: () => ({ QSL_SENT: "N" }) },
];

/** Field text for a QSO as it goes on a card or label. */
export const qf = (q: Qso, k: string) => q.fields[k] ?? "";
export const cardDate = (d: string) => (d.length === 8 ? `${d.slice(0, 4)}-${d.slice(4, 6)}-${d.slice(6, 8)}` : d);
export const cardTime = (t: string) => t.slice(0, 4);
export const cardFreq = (q: Qso) => (qf(q, "FREQ") ? Number(qf(q, "FREQ")).toFixed(3) : qf(q, "BAND"));
export const cardMode = (q: Qso) => qf(q, "SUBMODE") || qf(q, "MODE");

export type GroupBy = "none" | "bureau" | "manager";

export interface Pile { key: string; title: string; qsos: Qso[] }

/** Splits the queue into piles for a mailing: by the entity's outgoing bureau (cards with a QSL manager get their own pile per manager), or by manager. */
export function piles(queue: Qso[], by: GroupBy): Pile[] {
  if (by === "none") return [{ key: "", title: "", qsos: queue }];
  const map = new Map<string, Pile>();
  for (const q of queue) {
    const via = qf(q, "QSL_VIA").trim().toUpperCase();
    let key: string, title: string;
    if (by === "manager") {
      key = via ? `1|${via}` : "2|";
      title = via ? `Manager ${via}` : "No manager (bureau or direct)";
    } else if (via) {
      key = `2|${via}`;
      title = `Via manager ${via}`;
    } else {
      const country = qf(q, "COUNTRY").trim() || (qf(q, "DXCC") ? `DXCC ${qf(q, "DXCC")}` : "");
      key = country ? `1|${country.toUpperCase()}` : "3|";
      title = country ? `Bureau: ${country}` : "Bureau: country unknown";
    }
    const p = map.get(key) ?? { key, title, qsos: [] };
    p.qsos.push(q);
    map.set(key, p);
  }
  const out = [...map.values()].sort((a, b) => a.key.localeCompare(b.key));
  for (const p of out) p.qsos.sort((a, b) => qf(a, "CALL").localeCompare(qf(b, "CALL")));
  return out;
}

export const pileStations = (p: Pile) => new Set(p.qsos.map((q) => qf(q, "CALL"))).size;
