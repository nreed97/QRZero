import type { Fields } from "./types";

const today = () => new Date().toISOString().slice(0, 10).replace(/-/g, "");

/** Bulk paper-QSL actions on selected QSOs, as the ADIF fields each one sets. */
export const PAPER_ACTIONS: { key: string; label: string; fields: () => Fields }[] = [
  { key: "queue", label: "Queue a card to send", fields: () => ({ QSL_SENT: "Q" }) },
  { key: "sent-b", label: "Card sent via bureau", fields: () => ({ QSL_SENT: "Y", QSLSDATE: today(), QSL_SENT_VIA: "B" }) },
  { key: "sent-d", label: "Card sent direct", fields: () => ({ QSL_SENT: "Y", QSLSDATE: today(), QSL_SENT_VIA: "D" }) },
  { key: "rcvd-b", label: "Card received via bureau", fields: () => ({ QSL_RCVD: "Y", QSLRDATE: today(), QSL_RCVD_VIA: "B" }) },
  { key: "rcvd-d", label: "Card received direct", fields: () => ({ QSL_RCVD: "Y", QSLRDATE: today(), QSL_RCVD_VIA: "D" }) },
  { key: "none", label: "Not sending a card", fields: () => ({ QSL_SENT: "N" }) },
];
