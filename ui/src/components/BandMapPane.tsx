import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import type { PopContext } from "../bus";
import { onLive, useInstances, useRadios } from "../live";
import { BANDS, MODE_GROUP_NAME, bandForFreq, modeGroup } from "../modes";
import { localGet, localSet } from "../prefs";
import type { FtxDecode, Spot } from "../types";
import type { PaneActions } from "./SharedPanes";
import "../bandmap.css";
import ModeKey from "./ModeKey";

// A vertical band map: frequency runs down the left, spots and FTx decodes are
// stacked in columns beside it with a leader line to their frequency.

/** Zoom steps, kHz per pixel. */
const ZOOMS = [0.01, 0.025, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10];
/** Major and minor tick spacing in kHz; the first one at least 56 px apart wins. */
const TICKS: [number, number][] = [
  [0.5, 0.1], [1, 0.2], [2, 0.5], [5, 1], [10, 2], [25, 5], [50, 10], [100, 20], [250, 50], [500, 100], [1000, 200], [2500, 500], [5000, 1000],
];
const MAP_BANDS = BANDS.slice(0, 13).map(([b]) => b);

const STRIP_W = 6;
const SCALE_W = 70;
const GAP = 22;
const COL_W = 176;
const ROW_H = 17;
const MAX_SHIFT = ROW_H * 2;
const FTX_KEEP_S = 180;

type SegKind = "cw" | "data" | "phone" | "all" | "beacon";
type Plan = "us" | "r1" | "off";
/** [from kHz, to kHz, kind] */
type Seg = [number, number, SegKind];

// US: the FCC CW/data and phone/image sub-bands. IARU Region 1: the HF band plan, simplified.
const PLANS: Record<"us" | "r1", Record<string, Seg[]>> = {
  us: {
    "160m": [[1800, 2000, "all"]],
    "80m": [[3500, 3600, "data"], [3600, 4000, "phone"]],
    "40m": [[7000, 7125, "data"], [7125, 7300, "phone"]],
    "30m": [[10100, 10150, "data"]],
    "20m": [[14000, 14150, "data"], [14150, 14350, "phone"]],
    "17m": [[18068, 18110, "data"], [18110, 18168, "phone"]],
    "15m": [[21000, 21200, "data"], [21200, 21450, "phone"]],
    "12m": [[24890, 24930, "data"], [24930, 24990, "phone"]],
    "10m": [[28000, 28300, "data"], [28300, 29700, "phone"]],
    "6m": [[50000, 50100, "cw"], [50100, 54000, "all"]],
    "2m": [[144000, 144100, "cw"], [144100, 148000, "all"]],
  },
  r1: {
    "160m": [[1810, 1838, "cw"], [1838, 1843, "data"], [1843, 2000, "phone"]],
    "80m": [[3500, 3570, "cw"], [3570, 3600, "data"], [3600, 3800, "phone"]],
    "40m": [[7000, 7040, "cw"], [7040, 7060, "data"], [7060, 7200, "phone"]],
    "30m": [[10100, 10130, "cw"], [10130, 10150, "data"]],
    "20m": [[14000, 14070, "cw"], [14070, 14099, "data"], [14099, 14101, "beacon"], [14101, 14112, "data"], [14112, 14350, "phone"]],
    "17m": [[18068, 18095, "cw"], [18095, 18109, "data"], [18109, 18111, "beacon"], [18111, 18120, "data"], [18120, 18168, "phone"]],
    "15m": [[21000, 21070, "cw"], [21070, 21149, "data"], [21149, 21151, "beacon"], [21151, 21450, "phone"]],
    "12m": [[24890, 24915, "cw"], [24915, 24929, "data"], [24929, 24931, "beacon"], [24931, 24940, "data"], [24940, 24990, "phone"]],
    "10m": [[28000, 28070, "cw"], [28070, 28190, "data"], [28190, 28225, "beacon"], [28225, 29700, "phone"]],
    "6m": [[50000, 50100, "cw"], [50100, 52000, "all"]],
    "2m": [[144000, 144110, "cw"], [144110, 146000, "all"]],
  },
};
const SEG_NAME: Record<SegKind, string> = { cw: "CW only", data: "CW and data", phone: "Phone (and all modes)", all: "All modes", beacon: "Beacons" };

interface Prefs { band: string; zoom: Record<string, number>; plan: Plan; needed: boolean; follow: boolean }
const PREFS_KEY = "qrzero.bandmap";
const defaultZoom = (band: string) => (["6m", "4m", "2m"].includes(band) ? 1 : 0.2);

interface StampedDecode { d: FtxDecode; at: number }

/** Seconds since the epoch for a decode's HHMMSS, assuming the last 24 hours. */
function decodeTime(hhmmss: string, now: number): number {
  const d = new Date(now * 1000);
  const t = Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate(), +hhmmss.slice(0, 2), +hhmmss.slice(2, 4), +hhmmss.slice(4, 6)) / 1000;
  return t > now + 60 ? t - 86400 : t;
}

function ageText(s: number): string {
  if (s < 60) return "now";
  if (s < 3600) return `${Math.floor(s / 60)}m`;
  return `${Math.floor(s / 3600)}h`;
}

const fmtKhz = (khz: number, step: number) => (step < 1 ? khz.toFixed(1) : khz.toFixed(0));
const isNeeded = (n: Spot["needed"]) => !!n && (n.new_dxcc || n.new_band || n.new_mode || !!n.new_grid);

function needText(n: Spot["needed"]): string {
  if (!n) return "";
  const out: string[] = [];
  if (n.new_dxcc) out.push("DXCC");
  if (n.new_band) out.push("band");
  if (n.new_mode) out.push("mode");
  if (n.new_grid) out.push("grid");
  return out.join(" ");
}

/** One thing to place beside the scale. */
interface Item {
  key: string;
  khz: number;
  h: number;
  node: (style: React.CSSProperties) => JSX.Element;
}

export default function BandMapPane({ ctx, act }: { ctx: PopContext; act: PaneActions }) {
  const radios = useRadios();
  const instances = useInstances();
  const [prefs, setPrefs] = useState<Prefs>(() => localGet<Prefs>(PREFS_KEY, { band: "20m", zoom: {}, plan: "us", needed: false, follow: true }));
  const [spots, setSpots] = useState<Spot[]>([]);
  const [decodes, setDecodes] = useState<StampedDecode[]>([]);
  const [now, setNow] = useState(Date.now() / 1000);
  const [size, setSize] = useState({ w: 600, h: 400 });
  const [manualCentre, setManualCentre] = useState<number | null>(null);
  const mapRef = useRef<HTMLDivElement>(null);

  const save = (p: Partial<Prefs>) => {
    setPrefs((old) => {
      const next = { ...old, ...p };
      localSet(PREFS_KEY, next);
      return next;
    });
  };

  // Spots and decodes: a snapshot, then live events.
  useEffect(() => {
    api.cluster().then((c) => setSpots(c.spots.slice().reverse())).catch(() => {});
    api
      .ftx()
      .then((r) => {
        const t = Date.now() / 1000;
        setDecodes(r.decodes.slice(-400).map((d) => ({ d, at: decodeTime(d.time, t) })).filter((x) => t - x.at < FTX_KEEP_S));
      })
      .catch(() => {});
    const tick = setInterval(() => setNow(Date.now() / 1000), 10000);
    const off = onLive((e) => {
      if (e.type === "spot") setSpots((list) => [e.spot, ...list.filter((s) => s.call !== e.spot.call)].slice(0, 500));
      if (e.type === "decode") {
        const t = Date.now() / 1000;
        setDecodes((list) => [{ d: e.decode, at: t }, ...list.filter((x) => t - x.at < FTX_KEEP_S)].slice(0, 400));
      }
      if (e.type === "ftx_clear") setDecodes((list) => list.filter((x) => x.d.instance !== e.instance));
    });
    return () => {
      clearInterval(tick);
      off();
    };
  }, []);

  useLayoutEffect(() => {
    const el = mapRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setSize({ w: el.clientWidth, h: el.clientHeight }));
    ro.observe(el);
    setSize({ w: el.clientWidth, h: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  // Which radio, band and centre.
  const radio = radios.find((r) => r.key === ctx.radioKey && r.connected && r.freq_hz > 0) ?? null;
  const radioKhz = radio ? radio.freq_hz / 1000 : null;
  const radioBand = radio ? bandForFreq(radio.freq_hz / 1e6) ?? null : null;
  const following = !!radioBand && prefs.follow;
  const band = following ? radioBand! : MAP_BANDS.includes(prefs.band) ? prefs.band : "20m";
  const [, loMhz, hiMhz] = BANDS.find(([b]) => b === band)!;
  const lo = loMhz * 1000, hi = hiMhz * 1000;
  const zoom = prefs.zoom[band] ?? defaultZoom(band);
  const span = size.h * zoom;
  const clampCentre = (c: number) => Math.min(hi + span / 4, Math.max(lo - span / 4, c));
  const centre = following ? radioKhz! : clampCentre(manualCentre ?? lo - 12 * zoom + span / 2);
  const top = centre - span / 2;
  const yOf = (khz: number) => (khz - top) / zoom;

  // Reset the manual position when the band changes.
  useEffect(() => setManualCentre(null), [band]);

  const setZoom = (dir: -1 | 1) => {
    const i = ZOOMS.findIndex((z) => z >= zoom - 1e-9);
    const next = ZOOMS[Math.min(ZOOMS.length - 1, Math.max(0, (i < 0 ? ZOOMS.length - 1 : i) + dir))];
    save({ zoom: { ...prefs.zoom, [band]: next } });
  };
  const scrollBy = (khz: number) => {
    if (following) save({ follow: false, band });
    setManualCentre(clampCentre(centre + khz));
  };

  // Mouse wheel: scroll; with Ctrl, zoom. Needs a non-passive listener to stop the browser zooming.
  const wheel = useRef({ setZoom, scrollBy, zoom });
  wheel.current = { setZoom, scrollBy, zoom };
  useEffect(() => {
    const el = mapRef.current;
    if (!el) return;
    const h = (e: WheelEvent) => {
      e.preventDefault();
      if (e.ctrlKey) wheel.current.setZoom(e.deltaY > 0 ? 1 : -1);
      else wheel.current.scrollBy(Math.sign(e.deltaY) * 40 * wheel.current.zoom);
    };
    el.addEventListener("wheel", h, { passive: false });
    return () => el.removeEventListener("wheel", h);
  }, []);

  // Age limit: the cluster pane's setting (0 = any age, which fades over an hour).
  const maxAgeMin = localGet("qrzero.cluster", { maxAge: 30 }).maxAge;
  const fadeOver = (maxAgeMin || 60) * 60;

  const bandSpots = useMemo(
    () =>
      spots.filter((s) => {
        const khz = s.freq_hz / 1000;
        const age = now - s.received;
        return (s.band === band || (khz >= lo && khz <= hi)) && (!maxAgeMin || age < maxAgeMin * 60) && (!prefs.needed || isNeeded(s.needed));
      }),
    [spots, band, lo, hi, now, maxAgeMin, prefs.needed],
  );

  // FTx: one compact group per WSJT-X/JTDX instance on this band, at its dial frequency.
  const groups = useMemo(() => {
    return instances
      .filter((i) => i.dial_freq / 1000 >= lo && i.dial_freq / 1000 <= hi)
      .map((inst) => {
        const latest = new Map<string, StampedDecode>();
        for (const x of decodes) {
          if (x.d.instance !== inst.id || !x.d.call || now - x.at > FTX_KEEP_S) continue;
          if (prefs.needed && !isNeeded(x.d.needed)) continue;
          if (!latest.has(x.d.call)) latest.set(x.d.call, x);
        }
        const list = [...latest.values()].sort(
          (a, b) => Number(isNeeded(b.d.needed)) - Number(isNeeded(a.d.needed)) || Number(b.d.cq) - Number(a.d.cq) || b.d.snr - a.d.snr,
        );
        return { inst, list };
      });
  }, [instances, decodes, lo, hi, now, prefs.needed]);

  const ncols = Math.max(1, Math.floor((size.w - STRIP_W - SCALE_W - GAP) / COL_W));
  const ftxShow = ncols > 1 ? 16 : 10;

  // Everything to place, by frequency.
  const items: Item[] = [];
  for (const s of bandSpots) {
    const age = now - s.received;
    const n = s.needed;
    const worked = !!n && !n.new_call_band;
    const need = needText(n);
    const group = modeGroup(s.mode);
    items.push({
      key: `s${s.seq}`,
      khz: s.freq_hz / 1000,
      h: ROW_H,
      node: (style) => (
        <div
          key={`s${s.seq}`}
          className={`bm-label ${group ? `mode-${group}` : ""} ${worked ? "worked" : ""} ${s.watched ? "watched" : ""}`}
          style={{ ...style, opacity: Math.max(0.4, 1 - (age / fadeOver) * 0.6) }}
          onClick={() => act.onPick({ call: s.call, grid: null, band: s.band, mode: s.mode, freq_hz: s.freq_hz, tx_freq_hz: s.tx_freq_hz ?? undefined })}
          onDoubleClick={() => act.onPick({ call: s.call, grid: null, band: s.band, mode: s.mode, freq_hz: s.freq_hz, tx_freq_hz: s.tx_freq_hz ?? undefined, qsy: true })}
          title={`${s.call}  ${(s.freq_hz / 1000).toFixed(1)} kHz  ${s.mode}${group ? ` (${MODE_GROUP_NAME[group]})` : ""}\n${s.entity?.name ?? ""}\nSpotted by ${s.spotter} at ${s.time.slice(0, 2)}:${s.time.slice(2, 4)}${s.comment ? `\n${s.comment}` : ""}\nClick to fill in; double-click to fill in and tune`}
        >
          <span className="call">{s.call}</span>
          <span className="age">{ageText(age)}</span>
          {need && <span className="need">{need}</span>}
          {s.watched ? <span className="watch-mark">Watched</span> : null}
        </div>
      ),
    });
  }
  for (const { inst, list } of groups) {
    const shown = list.slice(0, ftxShow);
    const rows = Math.max(1, Math.ceil(shown.length / 2));
    items.push({
      key: `f${inst.id}`,
      khz: inst.dial_freq / 1000,
      h: ROW_H + rows * 15 + (list.length > shown.length ? 15 : 0) + 4,
      node: (style) => (
        <div key={`f${inst.id}`} className="bm-ftx" style={style} title={`${inst.source}: ${inst.mode} decodes from the last ${FTX_KEEP_S / 60} minutes, dial ${(inst.dial_freq / 1e3).toFixed(1)} kHz`}>
          <div className="bm-ftx-head">
            <span>{inst.mode}</span>
            <span className="muted">{inst.source}</span>
            <span className="muted count">{list.length}</span>
          </div>
          <div className="bm-ftx-calls">
            {shown.length === 0 && <span className="muted">no decodes</span>}
            {shown.map(({ d }) => (
              <span
                key={d.call}
                className={`${isNeeded(d.needed) ? "needed" : ""} ${d.needed && !d.needed.new_call_band ? "worked" : ""} ${d.cq ? "cq" : ""}`}
                onClick={() => act.onPick({ call: d.call!, grid: d.grid, band: d.band, mode: d.mode, freq_hz: d.freq_hz })}
                onDoubleClick={() => act.onPick({ call: d.call!, grid: d.grid, band: d.band, mode: d.mode, freq_hz: d.freq_hz, qsy: true })}
                title={`${d.message}\n${d.snr} dB  ${d.df} Hz  ${d.entity?.name ?? ""}${needText(d.needed) ? `\nNew ${needText(d.needed)}` : ""}`}
              >
                {d.call}
              </span>
            ))}
          </div>
          {list.length > shown.length && <div className="muted more">and {list.length - shown.length} more</div>}
        </div>
      ),
    });
  }
  items.sort((a, b) => a.khz - b.khz);

  // Stack labels in columns: take the first column where the label lands within a couple
  // of rows of its frequency, else the column that pushes it least.
  const placed: { item: Item; col: number; y: number }[] = [];
  let above = 0, below = 0, firstAbove: number | null = null, firstBelow: number | null = null;
  const bottoms = new Array(ncols).fill(0);
  for (const item of items) {
    const fy = yOf(item.khz);
    if (fy < 0) { above++; firstAbove = item.khz; continue; }
    if (fy > size.h) { below++; if (firstBelow === null) firstBelow = item.khz; continue; }
    const want = Math.max(0, fy - ROW_H / 2);
    let col = bottoms.findIndex((b) => Math.max(want, b) - want <= MAX_SHIFT);
    if (col < 0) col = bottoms.indexOf(Math.min(...bottoms));
    const y = Math.max(want, bottoms[col]);
    bottoms[col] = y + item.h + 2;
    placed.push({ item, col, y });
  }

  // The scale.
  const [major, minor] = TICKS.find(([m]) => m / zoom >= 56) ?? TICKS[TICKS.length - 1];
  const ticks: { khz: number; major: boolean }[] = [];
  for (let k = Math.ceil(top / minor) * minor; k <= top + span; k += minor) {
    const r = Math.round(k / minor) * minor;
    ticks.push({ khz: r, major: Math.abs(r / major - Math.round(r / major)) < 1e-6 });
  }
  const segs = prefs.plan === "off" ? [] : PLANS[prefs.plan][band] ?? [];
  const labelX = STRIP_W + SCALE_W + GAP;
  const scaleRight = STRIP_W + SCALE_W;

  const status = radio
    ? radioBand
      ? following
        ? `${radio.name} ${(radio.freq_hz / 1000).toFixed(2)} kHz`
        : `${radio.name} on ${radioBand}`
      : `${radio.name} is outside the ham bands`
    : ctx.radioKey
      ? "Radio not connected"
      : "No radio selected";

  return (
    <div className="bandmap">
      <div className="grid-tools">
        <select
          value={band}
          onChange={(e) => {
            save({ band: e.target.value, follow: radioBand === e.target.value });
            setManualCentre(null);
          }}
          aria-label="Band"
        >
          {MAP_BANDS.map((b) => <option key={b}>{b}</option>)}
        </select>
        {radioBand && (
          <label className="check" title="Keep the map centred on the radio">
            <input type="checkbox" checked={following} onChange={(e) => save({ follow: e.target.checked })} /> Follow radio
          </label>
        )}
        <span className="muted small">{status}</span>
        <span className="spacer" />
        <ModeKey />
        <button className="bm-btn" onClick={() => scrollBy(-size.h * zoom * 0.5)} title="Scroll down the band (wheel)">Lower</button>
        <button className="bm-btn" onClick={() => scrollBy(size.h * zoom * 0.5)} title="Scroll up the band (wheel)">Higher</button>
        <button className="bm-btn" onClick={() => setZoom(-1)} title="Zoom in (Ctrl+wheel)">+</button>
        <button className="bm-btn" onClick={() => setZoom(1)} title="Zoom out (Ctrl+wheel)">-</button>
        <span className="muted small mono bm-zoom" title="Kilohertz per pixel">{zoom < 0.1 ? zoom * 1000 + " Hz" : zoom + " kHz"}/px</span>
        <label className="check"><input type="checkbox" checked={prefs.needed} onChange={(e) => save({ needed: e.target.checked })} /> Needed only</label>
        <select value={prefs.plan} onChange={(e) => save({ plan: e.target.value as Plan })} aria-label="Band plan" title="Band plan strip down the scale">
          <option value="us">US plan</option>
          <option value="r1">IARU R1 plan</option>
          <option value="off">No plan</option>
        </select>
      </div>
      <div className="bm-map" ref={mapRef}>
        <svg className="bm-svg" width={size.w} height={size.h}>
          {segs.map(([a, b, kind]) => {
            const y1 = Math.max(0, yOf(a)), y2 = Math.min(size.h, yOf(b));
            if (y2 <= y1) return null;
            return (
              <rect key={`${a}`} className={`bm-seg ${kind}`} x={0} y={y1} width={STRIP_W} height={y2 - y1}>
                <title>{`${a}-${b} kHz: ${SEG_NAME[kind]}`}</title>
              </rect>
            );
          })}
          {/* Outside the band */}
          {yOf(lo) > 0 && <rect className="bm-out" x={STRIP_W} y={0} width={SCALE_W} height={yOf(lo)} />}
          {yOf(hi) < size.h && <rect className="bm-out" x={STRIP_W} y={yOf(hi)} width={SCALE_W} height={size.h - yOf(hi)} />}
          <line className="bm-axis" x1={scaleRight} x2={scaleRight} y1={0} y2={size.h} />
          {ticks.map((t) => {
            const y = yOf(t.khz);
            return (
              <g key={t.khz}>
                <line className="bm-tick" x1={scaleRight - (t.major ? 9 : 4)} x2={scaleRight} y1={y} y2={y} />
                {t.major && (
                  <text className="bm-tick-label" x={scaleRight - 12} y={y + 4} textAnchor="end">
                    {fmtKhz(t.khz, major)}
                  </text>
                )}
              </g>
            );
          })}
          {/* FTx passbands: dial to dial + 3 kHz */}
          {groups.map(({ inst }) => {
            const y1 = yOf(inst.dial_freq / 1000), y2 = yOf(inst.dial_freq / 1000 + 3);
            return <rect key={inst.id} className="bm-pass" x={scaleRight - 3} y={y1} width={3} height={Math.max(2, y2 - y1)} />;
          })}
          {placed.map(({ item, col, y }) => {
            const fy = yOf(item.khz);
            const x2 = labelX + col * COL_W;
            const ly = Math.min(y + ROW_H / 2, y + item.h / 2);
            return (
              <polyline
                key={item.key}
                className="bm-leader"
                points={`${scaleRight},${fy} ${scaleRight + 8},${fy} ${x2 - 6},${ly} ${x2},${ly}`}
              />
            );
          })}
          {radioKhz !== null && yOf(radioKhz) >= 0 && yOf(radioKhz) <= size.h && (
            <g className="bm-rig">
              <line x1={STRIP_W} x2={size.w} y1={yOf(radioKhz)} y2={yOf(radioKhz)} />
              <polygon points={`${STRIP_W},${yOf(radioKhz) - 5} ${STRIP_W + 7},${yOf(radioKhz)} ${STRIP_W},${yOf(radioKhz) + 5}`} />
            </g>
          )}
        </svg>
        {placed.map(({ item, col, y }) => item.node({ left: labelX + col * COL_W, top: y, width: COL_W - 8, minHeight: item.h }))}
        {above > 0 && (
          <button className="bm-more top" onClick={() => firstAbove !== null && scrollBy(firstAbove - centre)}>
            {above} above
          </button>
        )}
        {below > 0 && (
          <button className="bm-more bottom" onClick={() => firstBelow !== null && scrollBy(firstBelow - centre)}>
            {below} below
          </button>
        )}
        {items.length === 0 && <div className="bm-empty muted small">No spots or decodes on {band}{maxAgeMin ? ` in the last ${maxAgeMin} minutes` : ""}.</div>}
      </div>
    </div>
  );
}
