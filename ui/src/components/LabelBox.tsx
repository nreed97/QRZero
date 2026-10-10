import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { LABEL_DEFAULTS, TAPES, drawLabel, groupLabels, printLabels, saveLabelImages, type LabelSettings } from "../label";
import { usePref } from "../prefs";
import type { Qso } from "../types";

interface Props {
  /** The QSOs that go on the labels; grouped one label per station. */
  qsos: Qso[];
  /** Called after the printer took the labels, with the QSOs that were on them. */
  onPrinted?: (qsos: Qso[]) => void;
  /** Called with a line to show the user. */
  onMessage: (text: string, ok: boolean) => void;
  /** Keys that print / save while this is on screen are handled by the parent; this is just for the button hints. */
  hints?: boolean;
}

/** Label preview, Print and Save image buttons, and the printer settings behind them. */
export default function LabelBox({ qsos, onPrinted, onMessage, hints }: Props) {
  const [stored, save, loaded] = usePref<LabelSettings>("qsl_label", LABEL_DEFAULTS);
  const s = { ...LABEL_DEFAULTS, ...stored };
  const [printers, setPrinters] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const preview = useRef<HTMLCanvasElement>(null);
  const groups = useMemo(() => groupLabels(qsos, s), [qsos, s.tape, s.length, s.foot]);

  useEffect(() => {
    api.labelPrinters().then(setPrinters, () => {});
  }, []);

  // A QL printer is the likely pick the first time.
  useEffect(() => {
    if (!loaded || s.printer || !printers.length) return;
    const ql = printers.find((p) => /\bQL[- ]?\d/i.test(p));
    if (ql) save({ ...s, printer: ql });
  }, [loaded, printers]);

  useEffect(() => {
    const c = preview.current;
    if (!c) return;
    const g = c.getContext("2d")!;
    if (!groups.length) {
      g.clearRect(0, 0, c.width, c.height);
      return;
    }
    const label = drawLabel(groups[0], s);
    c.width = label.width;
    c.height = label.height;
    g.drawImage(label, 0, 0);
  }, [groups, s.tape, s.length, s.foot]);

  const change = (patch: Partial<LabelSettings>) => save({ ...s, ...patch });
  const n = groups.length;
  const plural = `${n} label${n === 1 ? "" : "s"}`;

  const print = async () => {
    if (!n || busy) return;
    setBusy(true);
    try {
      const done = await printLabels(groups, s);
      onMessage(`Printed ${done} label${done === 1 ? "" : "s"} on ${s.printer}.`, true);
      onPrinted?.(qsos);
    } catch (e) {
      onMessage((e as Error).message, false);
    } finally {
      setBusy(false);
    }
  };

  const saveImages = async () => {
    if (!n || busy) return;
    setBusy(true);
    try {
      const names = await saveLabelImages(groups, s);
      onMessage(`Saved ${names.join(", ")}.`, true);
    } catch (e) {
      onMessage((e as Error).message, false);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="label-box">
      <div className="label-preview">
        {n === 0 ? <p className="muted small">The label shows here.</p> : <canvas ref={preview} aria-label="Label preview" />}
      </div>
      <div className="row">
        <button className="primary" disabled={!n || busy} onClick={print} title={hints ? "F5" : undefined}>
          {busy ? "Working…" : `Print ${n ? plural : "label"}`}
        </button>
        <button disabled={!n || busy} onClick={saveImages} title={hints ? "F6" : undefined}>Save label image</button>
        {!s.printer && printers.length > 0 && <span className="muted small">Pick your printer under Label printer.</span>}
      </div>
      <details className="more">
        <summary>Label printer{s.printer ? `: ${s.printer}` : ""}</summary>
        <div className="row">
          <label className="f w-xl">
            <span>Printer</span>
            {printers.length ? (
              <select value={s.printer} onChange={(e) => change({ printer: e.target.value })}>
                <option value="">(none)</option>
                {printers.map((p) => <option key={p}>{p}</option>)}
                {s.printer && !printers.includes(s.printer) && <option>{s.printer}</option>}
              </select>
            ) : (
              <input value={s.printer} placeholder="Brother QL-700" onChange={(e) => change({ printer: e.target.value })} />
            )}
          </label>
          <label className="f w-l">
            <span>Tape</span>
            <select value={s.tape} onChange={(e) => change({ tape: Number(e.target.value) })}>
              {TAPES.map((t) => <option key={t.mm} value={t.mm}>{t.name}</option>)}
            </select>
          </label>
          <label className="f w-s">
            <span>Length (mm)</span>
            <input value={s.length} inputMode="decimal" onChange={(e) => change({ length: Math.min(300, Math.max(25, Number(e.target.value) || 101.6)) })} />
          </label>
          <label className="f w-xl">
            <span>Bottom line (optional)</span>
            <input value={s.foot} placeholder="RIG K3S · ANT hex beam · PWR 100W" onChange={(e) => change({ foot: e.target.value })} />
          </label>
        </div>
        <p className="small muted">
          Labels go straight to the printer, with no print dialog. The printer needs the Brother driver installed. {" "}
          {s.tape === 50 ? "DK-22223 at 101.6 mm is the fill-in block on the back of a card." : ""}
        </p>
      </details>
    </div>
  );
}
