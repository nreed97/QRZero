import { OQRS, confirmedBy } from "../confirmations";
import { cardDate, qf } from "../paper";
import type { Qso } from "../types";

/** The first non-empty value of a field, newest QSO first. */
const first = (qsos: Qso[], k: string) => qsos.map((q) => qf(q, k)).find((v) => v) ?? "";

/** What happened to the card, e.g. "sent B 2026-09-30 / rcvd D 2026-10-02". */
export function cardStatus(q: Qso): string {
  const part = (dir: "SENT" | "RCVD", label: string) => {
    const v = qf(q, `QSL_${dir}`);
    if (!v || v === "N") return "";
    const via = qf(q, `QSL_${dir}_VIA`);
    const date = qf(q, dir === "SENT" ? "QSLSDATE" : "QSLRDATE");
    const word = ({ Y: label, Q: "queued", R: "requested", I: "ignored" } as Record<string, string>)[v] ?? `${label} ${v}`;
    return [word, via, date && cardDate(date)].filter(Boolean).join(" ");
  };
  const oqrs = qf(q, OQRS) === "Y" ? "OQRS" : "";
  return [part("SENT", "sent"), part("RCVD", "rcvd"), oqrs].filter(Boolean).join(" / ");
}

/** Label / value lines of the contact card; empty values are left out. */
export function Contact({ rows, call }: { rows: Qso[]; call: string }) {
  const g = (k: string) => first(rows, k);
  const place = [g("QTH"), g("STATE"), g("CNTY")].filter(Boolean).join(", ");
  const zones = [g("CQZ") && `CQ ${g("CQZ")}`, g("ITU") && `ITU ${g("ITU")}`].filter(Boolean).join(", ");
  const confirmed = new Set(rows.flatMap((q) => confirmedBy(q.fields).map(([, n]) => n)));
  const sent = rows.filter((q) => qf(q, "QSL_SENT") === "Y").length;
  const lines: [string, string][] = [
    ["Name", g("NAME")],
    ["Address", g("ADDRESS")],
    ["QTH", place],
    ["Country", g("COUNTRY")],
    ["Grid", g("GRIDSQUARE")],
    ["Zones", zones],
    ["QSL via", g("QSL_VIA")],
    ["Email", g("EMAIL")],
    ["Cards", `${sent} of ${rows.length} sent`],
    ["Confirmed", [...confirmed].join(", ") || "not yet"],
  ];
  return (
    <div className="ql-card">
      <h3 className="mono">{call}</h3>
      <dl>
        {lines.filter(([, v]) => v).map(([k, v]) => (
          <div key={k}>
            <dt>{k}</dt>
            <dd className={k === "QSL via" || k === "Grid" ? "mono" : ""}>{v}</dd>
          </div>
        ))}
      </dl>
    </div>
  );
}
