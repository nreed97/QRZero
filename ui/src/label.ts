import { api } from "./api";
import { cardDate, cardFreq, cardMode, cardTime, qf } from "./paper";
import type { Qso } from "./types";
import { download } from "./util";

/** Where and how the QSL-card label is printed. Kept in the log database. */
export interface LabelSettings {
  printer: string;
  /** Tape width in mm: a DK continuous roll. */
  tape: number;
  /** Label length in mm. */
  length: number;
  /** Optional line under a rule at the bottom, e.g. RIG, ANT or power. */
  foot: string;
}

export const LABEL_DEFAULTS: LabelSettings = { printer: "", tape: 50, length: 101.6, foot: "" };

/** Continuous DK tapes the QL-700 takes: width in mm, dots across. */
export const TAPES: { mm: number; dots: number; name: string }[] = [
  { mm: 50, dots: 554, name: "DK-22223 (50 mm)" },
  { mm: 62, dots: 696, name: "DK-22205 (62 mm)" },
  { mm: 54, dots: 590, name: "DK-22211 (54 mm)" },
  { mm: 38, dots: 413, name: "DK-22225 (38 mm)" },
  { mm: 29, dots: 306, name: "DK-22210 (29 mm)" },
];

const DPI = 300;
const PAD_X = 36;
/** Blank tape the printer feeds before and after each label. */
const FEED_DOTS = 35;
const MONO = "Consolas, 'DejaVu Sans Mono', 'Liberation Mono', 'Courier New', monospace";

const COLUMNS: [string, number][] = [["DATE", 0.26], ["UTC", 0.17], ["MHz", 0.2], ["MODE", 0.19], ["RST", 0.18]];

const tapeDots = (s: LabelSettings) => TAPES.find((t) => t.mm === s.tape)?.dots ?? 554;
const lengthDots = (s: LabelSettings) => Math.round((s.length / 25.4) * DPI) - 2 * FEED_DOTS;

/** How many QSO lines fit on one label: 4 on the card's fill-in block, more on wide tape, fewer on narrow. */
export function maxRows(s: LabelSettings): number {
  const room = Math.floor((tapeDots(s) - 228 - (s.foot.trim() ? 100 : 20)) / 58) + 1;
  return Math.max(1, Math.min(s.tape >= 62 ? 5 : 4, room));
}

const stamp = (q: Qso) => qf(q, "QSO_DATE") + qf(q, "TIME_ON");
export const newestFirst = (qsos: Qso[]) => [...qsos].sort((a, b) => stamp(b).localeCompare(stamp(a)));

/** One label per station (and own callsign), up to `maxRows` QSOs each, newest first; stations in the order picked. */
export function groupLabels(qsos: Qso[], s: LabelSettings): Qso[][] {
  const groups = new Map<string, Qso[]>();
  for (const q of qsos) {
    const key = `${qf(q, "CALL")}|${qf(q, "STATION_CALLSIGN")}`;
    groups.set(key, [...(groups.get(key) ?? []), q]);
  }
  const n = maxRows(s);
  const out: Qso[][] = [];
  for (const g of groups.values()) {
    const sorted = newestFirst(g);
    for (let i = 0; i < sorted.length; i += n) out.push(sorted.slice(i, i + n));
  }
  return out;
}

/** The label as it reads on the card (landscape), black on white. */
export function drawLabel(qsos: Qso[], s: LabelSettings): HTMLCanvasElement {
  const w = lengthDots(s);
  const h = tapeDots(s);
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const g = canvas.getContext("2d")!;
  g.fillStyle = "#fff";
  g.fillRect(0, 0, w, h);
  g.fillStyle = "#000";
  g.textBaseline = "alphabetic";
  const font = (px: number, bold = false) => `${bold ? "bold " : ""}${px}px ${MONO}`;
  const right = w - PAD_X;
  const usable = right - PAD_X;

  const rows = newestFirst(qsos).slice(0, maxRows(s));
  g.textAlign = "left";
  g.font = font(28);
  g.fillText("CONFIRMING QSO WITH", PAD_X, 92);
  g.textAlign = "right";
  g.font = font(80, true);
  g.fillText(qf(rows[0], "CALL"), right, 92);

  g.textAlign = "left";
  const xs: number[] = [];
  let x = PAD_X;
  for (const [, share] of COLUMNS) {
    xs.push(x);
    x += usable * share;
  }
  g.font = font(28, true);
  COLUMNS.forEach(([title], i) => g.fillText(title, xs[i], 160));
  g.fillRect(PAD_X, 172, usable, 3);

  g.font = font(40);
  rows.forEach((q, r) => {
    const cells = [cardDate(qf(q, "QSO_DATE")), cardTime(qf(q, "TIME_ON")), cardFreq(q), cardMode(q), qf(q, "RST_SENT")];
    cells.forEach((c, i) => g.fillText(c, xs[i], 228 + r * 58));
  });

  const foot = s.foot.trim();
  if (foot) {
    g.fillRect(PAD_X, h - 88, usable, 3);
    let px = 30;
    g.font = font(px);
    while (px > 18 && g.measureText(foot).width > usable) g.font = font(--px);
    g.fillText(foot, PAD_X, h - 42);
  }
  return canvas;
}

/** The label turned to run along the tape, packed 1 bit per dot (most significant first, 1 = black), row by row. */
function tapeBits(label: HTMLCanvasElement): Uint8Array {
  const w = label.width;
  const h = label.height;
  const turned = document.createElement("canvas");
  turned.width = h;
  turned.height = w;
  const g = turned.getContext("2d")!;
  g.fillStyle = "#fff";
  g.fillRect(0, 0, h, w);
  g.translate(0, w);
  g.rotate(-Math.PI / 2);
  g.drawImage(label, 0, 0);
  const px = g.getImageData(0, 0, h, w).data;
  const stride = Math.ceil(h / 8);
  const out = new Uint8Array(stride * w);
  for (let y = 0; y < w; y++) {
    for (let x = 0; x < h; x++) {
      const i = (y * h + x) * 4;
      if (px[i] * 0.3 + px[i + 1] * 0.59 + px[i + 2] * 0.11 < 128) out[y * stride + (x >> 3)] |= 0x80 >> (x & 7);
    }
  }
  return out;
}

/** Sends the labels to the printer; resolves with how many were printed. */
export async function printLabels(groups: Qso[][], s: LabelSettings): Promise<number> {
  if (!s.printer) throw new Error("Pick the label printer under Label printer first.");
  const pages = groups.map((g) => tapeBits(drawLabel(g, s)));
  const rows = lengthDots(s);
  const body = new Uint8Array(pages.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of pages) {
    body.set(p, at);
    at += p.length;
  }
  return api.labelPrint(s.printer, s.tape, tapeDots(s), rows, body);
}

function toBlob(c: HTMLCanvasElement): Promise<Blob> {
  return new Promise((res, rej) => c.toBlob((b) => (b ? res(b) : rej(new Error("Couldn't make the image."))), "image/png"));
}

/** Saves each label as a PNG, upright the way it sits on the card. Returns the file names. */
export async function saveLabelImages(groups: Qso[][], s: LabelSettings): Promise<string[]> {
  const seen = new Map<string, number>();
  const names: string[] = [];
  for (const g of groups) {
    const call = qf(g[0], "CALL").replace(/[^A-Za-z0-9]/g, "-");
    const n = (seen.get(call) ?? 0) + 1;
    seen.set(call, n);
    const name = `${call}${n > 1 ? `-${n}` : ""}.png`;
    download(name, await toBlob(drawLabel(g, s)));
    names.push(name);
    // Browsers drop downloads fired in the same tick.
    await new Promise((r) => setTimeout(r, 150));
  }
  return names;
}
