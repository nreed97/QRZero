import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { BANDS, modeLabel, slotGroup } from "../modes";
import { localGet, localSet } from "../prefs";
import type { Fields, LookupResult, Qso, SlotPick } from "../types";
import { adifDateTime, fmtDate, fmtTime } from "../util";
import type { EntryContext } from "./EntryPanel";
import { useColumnDrag } from "../columnDrag";
import { useDisplay } from "../display";
import "../worked.css";

export interface WorkedPaneProps {
  logId: number;
  call: string;                      // the call currently in the entry panel ("" when empty)
  lookup: LookupResult | null;       // has worked.call_count etc.
  entry: EntryContext;               // from ./EntryPanel: current band, mode, fields
  refreshKey: number;                // bumps when QSOs are logged or edited: reload
  selectedId?: number | null;        // QSO open in the editor, highlight it
  slot?: SlotPick | null;            // a DXCC slot picked in a slots grid: list every QSO in it instead
  onSlotClear?: () => void;          // back to the call's history
  onEdit: (q: Qso) => void;          // open the QSO in the editor
  onCopy: (fields: Fields) => void;  // copy NAME, QTH, GRIDSQUARE, STATE, CNTY etc. into the entry panel
  onShowInLog: (call: string) => void;
}

/** Station details worth carrying over from an earlier QSO. */
export const WORKED_COPY_FIELDS = ["NAME", "QTH", "GRIDSQUARE", "STATE", "CNTY", "QSL_VIA"] as const;

interface Toggles { band: boolean; mode: boolean; unconfirmed: boolean; full?: boolean }
const PREF_KEY = "qrzero.worked";
const NO_TOGGLES: Toggles = { band: false, mode: false, unconfirmed: false };
const LIMIT = 10_000;

type Qsl = "C" | "sent" | "";
const yes = (v: string | undefined) => v === "Y" || v === "V";
function qslState(f: Fields, rcvd: string, sent: string): Qsl {
  if (yes(f[rcvd])) return "C";
  if (f[sent] === "Y") return "sent";
  return "";
}
const lotw = (f: Fields) => qslState(f, "LOTW_QSL_RCVD", "LOTW_QSL_SENT");
const card = (f: Fields) => qslState(f, "QSL_RCVD", "QSL_SENT");
const eqsl = (f: Fields) => qslState(f, "EQSL_QSL_RCVD", "EQSL_QSL_SENT");
const confirmed = (f: Fields) => lotw(f) === "C" || card(f) === "C" || eqsl(f) === "C";

const bandOf = (f: Fields) => (f.BAND ?? "").toLowerCase();
const bandRank = (b: string) => {
  const i = BANDS.findIndex(([name]) => name === b);
  return i < 0 ? 999 : i;
};

function copyFields(f: Fields): Fields {
  const out: Fields = {};
  for (const k of WORKED_COPY_FIELDS) if (f[k]) out[k] = f[k];
  return out;
}

function QslCell({ v }: { v: Qsl }) {
  return <td className={v === "C" ? "wb-qsl wb-conf" : "wb-qsl"}>{v}</td>;
}

/** The list's columns in their usual order, with the width each takes once any column has been resized. */
const COLS = [
  { key: "call", label: "Call", w: 80 },
  { key: "date", label: "Date", w: 92 },
  { key: "utc", label: "UTC", w: 54 },
  { key: "band", label: "Band", w: 50 },
  { key: "mode", label: "Mode", w: 60 },
  { key: "sent", label: "Sent", w: 50 },
  { key: "rcvd", label: "Rcvd", w: 50 },
  { key: "lotw", label: "LoTW", w: 52, cls: "wb-qsl" },
  { key: "card", label: "Card", w: 52, cls: "wb-qsl" },
  { key: "eqsl", label: "eQSL", w: 52, cls: "wb-qsl" },
  { key: "name", label: "Name", w: 120 },
  { key: "comment", label: "Comment", w: 260, cls: "wb-wide" },
];
const COL_KEYS = COLS.map((c) => c.key);

interface Menu { x: number; y: number; q: Qso }

export default function WorkedPane({ logId, call, lookup, entry, refreshKey, selectedId, slot, onSlotClear, onEdit, onCopy, onShowInLog }: WorkedPaneProps) {
  useDisplay();
  const entryCall = call.trim().toUpperCase();
  // What is listed: the call's history, or every QSO in the picked slot.
  const target = slot ? `${slot.dxcc} ${slot.band} ${slot.group}` : entryCall;
  const [rows, setRows] = useState<Qso[] | null>(null);
  const [loadedFor, setLoadedFor] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const [toggles, setToggles] = useState<Toggles>(() => localGet(PREF_KEY, NO_TOGGLES));
  const [sel, setSel] = useState<number | null>(selectedId ?? null);
  const [menu, setMenu] = useState<Menu | null>(null);
  const bodyRef = useRef<HTMLDivElement>(null);
  const head = useRef<HTMLTableRowElement>(null);
  const colDrag = useColumnDrag("qrzero.workedCols", COL_KEYS, head);
  const [picking, setPicking] = useState<{ x: number; y: number } | null>(null);

  // Load the whole history for the call; wait for typing to settle and drop stale answers.
  useEffect(() => {
    if (!target) {
      setRows(null);
      setLoadedFor("");
      setError(null);
      return;
    }
    let live = true;
    const timer = setTimeout(() => {
      api
        .search(logId, slot ? { dxcc: slot.dxcc, bands: [slot.band] } : { exact_call: target }, 0, LIMIT, "newest")
        .then((r) => {
          if (!live) return;
          setRows(slot ? r.rows.filter((q) => slotGroup(modeLabel(q.fields)) === slot.group) : r.rows);
          setLoadedFor(target);
          setError(null);
        })
        .catch((e) => live && setError(String(e.message ?? e)));
    }, slot ? 0 : 200);
    return () => {
      live = false;
      clearTimeout(timer);
    };
    // The slot is in `target`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [logId, target, refreshKey, reload]);

  useEffect(() => setSel(selectedId ?? null), [selectedId]);
  useEffect(() => setMenu(null), [target, logId]);

  const setToggle = (k: keyof Toggles) => {
    const next = { ...toggles, [k]: !toggles[k] };
    setToggles(next);
    localSet(PREF_KEY, next);
  };

  // While the full list loads, the lookup's recent QSOs (same call) stand in.
  const all = useMemo<Qso[]>(() => {
    if (loadedFor === target && rows) return rows;
    if (slot) return [];
    const recent = lookup?.worked.recent ?? [];
    return recent.length && recent[0].fields.CALL?.toUpperCase() === target ? recent : [];
  }, [rows, loadedFor, target, lookup, slot]);
  const loading = !!target && loadedFor !== target && !error;

  const band = entry.band.toLowerCase();
  const mode = entry.mode.toUpperCase();
  const isSlot = (f: Fields) => !slot && !!band && !!mode && bandOf(f) === band && modeLabel(f).toUpperCase() === mode;

  const shown = useMemo(
    () =>
      all.filter(
        (q) =>
          (!!slot || !toggles.band || !band || bandOf(q.fields) === band) &&
          (!!slot || !toggles.mode || !mode || modeLabel(q.fields).toUpperCase() === mode) &&
          (!toggles.unconfirmed || !confirmed(q.fields)),
      ),
    [all, toggles, band, mode, slot],
  );

  const summary = useMemo(() => {
    const bands = [...new Set(all.map((q) => bandOf(q.fields)).filter(Boolean))].sort((a, b) => bandRank(a) - bandRank(b));
    // Sideband is one mode here: LSB and USB are listed as SSB.
    const modes = [...new Set(all.map((q) => (["LSB", "USB"].includes(modeLabel(q.fields).toUpperCase()) ? "SSB" : modeLabel(q.fields))).filter(Boolean))].sort();
    const conf = [...new Set(all.filter((q) => confirmed(q.fields)).map((q) => bandOf(q.fields)).filter(Boolean))].sort((a, b) => bandRank(a) - bandRank(b));
    const first = all.length ? fmtDate(all[all.length - 1].fields) : "";
    const callOf = (q: Qso) => (q.fields.CALL ?? "").toUpperCase();
    const calls = new Set(all.map(callOf)).size;
    const confCalls = new Set(all.filter((q) => confirmed(q.fields)).map(callOf)).size;
    return { bands, modes, conf, first, calls, confCalls };
  }, [all]);

  // Keep the selected row in view when moving with the keyboard.
  useEffect(() => {
    if (sel === null) return;
    bodyRef.current?.querySelector<HTMLElement>(`tr[data-id="${sel}"]`)?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  // The column list closes on Esc or a click elsewhere.
  useEffect(() => {
    if (!picking) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setPicking(null);
    const onDown = (e: MouseEvent) => {
      if (!(e.target as Element).closest?.(".wb-colwrap")) setPicking(null);
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onDown);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onDown);
    };
  }, [picking]);

  // The context menu closes on Esc, a click elsewhere, scrolling or resizing.
  useEffect(() => {
    if (!menu) return;
    const close = () => setMenu(null);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    const onDown = (e: MouseEvent) => {
      if (!(e.target as Element).closest?.(".wb-menu")) close();
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onDown);
    window.addEventListener("resize", close);
    window.addEventListener("blur", close);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onDown);
      window.removeEventListener("resize", close);
      window.removeEventListener("blur", close);
    };
  }, [menu]);

  const mark = useCallback(
    (q: Qso, fields: Fields) => {
      setMenu(null);
      api.markQsos([q.id], fields).then(() => setReload((n) => n + 1)).catch((e) => setError(String(e.message ?? e)));
    },
    [],
  );

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (!shown.length) return;
    const i = shown.findIndex((q) => q.id === sel);
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const next = i < 0 ? 0 : Math.max(0, Math.min(shown.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)));
      setSel(shown[next].id);
    } else if (e.key === "Enter" && i >= 0) {
      e.preventDefault();
      onEdit(shown[i]);
    } else if (e.key === "ContextMenu" && i >= 0) {
      e.preventDefault();
      const r = bodyRef.current?.querySelector(`tr[data-id="${sel}"]`)?.getBoundingClientRect();
      if (r) setMenu({ x: r.left + 40, y: r.bottom, q: shown[i] });
    }
  };

  // The columns on show: in the saved order, minus the hidden ones (and Call, except in a slot's list).
  const cols = colDrag.order
    .filter((k) => !colDrag.hidden.includes(k) && (slot || k !== "call"))
    .map((k) => COLS.find((c) => c.key === k)!);
  const sized = Object.keys(colDrag.widths).length > 0;
  const tableWidth = 56 + cols.reduce((n, c) => n + (colDrag.widths[c.key] ?? c.w), 0);
  const cell = (key: string, q: Qso) => {
    const f = q.fields;
    switch (key) {
      case "call": return <td key={key} className={f.CALL?.toUpperCase() === entryCall ? "wb-call wb-this" : "wb-call"}>{f.CALL}</td>;
      case "date": return <td key={key}>{fmtDate(f)}</td>;
      case "utc": return <td key={key}>{fmtTime(f)}</td>;
      case "band": return <td key={key}>{f.BAND}</td>;
      case "mode": return <td key={key}>{modeLabel(f)}</td>;
      case "sent": return <td key={key}>{f.RST_SENT}</td>;
      case "rcvd": return <td key={key}>{f.RST_RCVD}</td>;
      case "lotw": return <QslCell key={key} v={lotw(f)} />;
      case "card": return <QslCell key={key} v={card(f)} />;
      case "eqsl": return <QslCell key={key} v={eqsl(f)} />;
      case "name": return <td key={key} className="wb-text" title={f.NAME}>{f.NAME}</td>;
      default:
        return (
          <td key={key} className="wb-text wb-wide" title={f.COMMENT}>
            {!slot && f.CALL && f.CALL.toUpperCase() !== target && <span className="wb-as">as {f.CALL}</span>}
            {f.COMMENT}
          </td>
        );
    }
  };

  const count = loadedFor === target ? ` (${all.length})` : "";
  const title = slot ? `Worked before: ${slot.name} ${slot.band} ${slot.group}${count}` : target ? `Worked before: ${target}${count}` : "Worked before";
  const filtered = shown.length !== all.length;

  // In a slot the rows are other stations: the menu acts on the row's call.
  const menuCall = menu ? (slot ? (menu.q.fields.CALL ?? "").toUpperCase() : target) : "";

  let body: React.ReactNode;
  if (!target) body = <div className="wb-empty muted">Type a call to see your history with it.</div>;
  else if (error && !all.length) body = <div className="wb-empty err">Could not load the history: {error}</div>;
  else if (!all.length) body = <div className="wb-empty muted">{loading ? "Loading..." : slot ? `No QSOs with ${slot.name} on ${slot.band} ${slot.group}.` : `No previous QSOs with ${target}.`}</div>;
  else
    body = (
      <>
        <div className="wb-scroll" ref={bodyRef} tabIndex={0} onKeyDown={onKeyDown} aria-label={slot ? `QSOs in ${slot.name} ${slot.band} ${slot.group}` : `QSOs with ${target}`}>
          <table className="wb-table" style={sized ? { tableLayout: "fixed", width: tableWidth, minWidth: "100%" } : undefined}>
            <thead>
              <tr ref={head}>
                {cols.map((c) => {
                  const m = colDrag.moving;
                  const at = colDrag.order.indexOf(c.key);
                  const last = c.key === cols[cols.length - 1].key;
                  const cls = ["col-head", c.cls, m?.key === c.key && "moving", m && m.to === at && m.key !== c.key && "drop-before", m && m.to === colDrag.order.length && last && "drop-after"].filter(Boolean).join(" ");
                  return (
                    <th key={c.key} data-key={c.key} className={cls} style={sized ? { width: colDrag.widths[c.key] ?? c.w } : undefined}
                      title="Drag to move this column, drag its right edge to resize" onPointerDown={(e) => colDrag.move(e, c.key)}>
                      {c.label}
                      <span className="col-resize" onPointerDown={(e) => colDrag.resize(e, c.key)} onDoubleClick={() => colDrag.setWidth(c.key, 0)} title="Drag to resize, double-click to reset" />
                    </th>
                  );
                })}
                <th className="wb-act" aria-label="Actions" style={sized ? { width: 56 } : undefined} />
              </tr>
            </thead>
            <tbody>
              {shown.map((q) => {
                const f = q.fields;
                const cls = [q.id === sel && "sel", q.id === selectedId && "editing", isSlot(f) && "slot"].filter(Boolean).join(" ");
                return (
                  <tr
                    key={q.id}
                    data-id={q.id}
                    className={cls}
                    aria-selected={q.id === sel}
                    title={isSlot(f) ? `Same band and mode as the entry (${entry.band} ${entry.mode})` : undefined}
                    onClick={() => setSel(q.id)}
                    onDoubleClick={() => onEdit(q)}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      setSel(q.id);
                      setMenu({ x: e.clientX, y: e.clientY, q });
                    }}
                  >
                    {cols.map((c) => cell(c.key, q))}
                    <td className="wb-act">
                      <span className="wb-btns">
                        <button type="button" className="tiny" onClick={(e) => { e.stopPropagation(); onEdit(q); }}>Edit</button>
                        {!slot && <button type="button" className="tiny" title="Copy name, QTH, grid, state, county and QSL via into the entry" onClick={(e) => { e.stopPropagation(); onCopy(copyFields(f)); }}>Copy</button>}
                      </span>
                    </td>
                  </tr>
                );
              })}
              {!shown.length && (
                <tr className="wb-none">
                  <td colSpan={cols.length + 1} className="muted">No QSOs match the filters.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        {slot ? (
          <div className="wb-summary">
            <span><b>Calls</b> {summary.calls}</span>
            <span><b>Confirmed</b> {summary.confCalls ? <span className="wb-conf">{summary.confCalls} {summary.confCalls === 1 ? "call" : "calls"}</span> : "none"}</span>
            <span><b>First QSO</b> {summary.first || "unknown"}</span>
            {error && <span className="err">{error}</span>}
          </div>
        ) : toggles.full && (
          <div className="wb-summary">
            <span><b>Bands</b> {summary.bands.join(" ") || "none"}</span>
            <span><b>Modes</b> {summary.modes.join(" ") || "none"}</span>
            <span><b>First QSO</b> {summary.first || "unknown"}</span>
            <span><b>Confirmed</b> {summary.conf.length ? <span className="wb-conf">{summary.conf.join(" ")}</span> : "none"}</span>
            {error && <span className="err">{error}</span>}
          </div>
        )}
      </>
    );

  return (
    <section className="panel worked-pane">
      <div className="panel-title">
        <span className="wb-title">{title}</span>
        <span className="wb-colwrap">
          <button type="button" className={`tiny${picking ? " on" : ""}`} aria-pressed={!!picking} title="Show or hide columns"
            onClick={(e) => { const r = e.currentTarget.getBoundingClientRect(); setPicking(picking ? null : { x: r.left, y: r.bottom }); }}>Columns</button>
          {picking && (
            <div className="wb-colmenu" role="group" aria-label="Show columns" style={{ left: Math.min(picking.x, window.innerWidth - 330), top: picking.y }}>
              {COLS.filter((c) => slot || c.key !== "call").map((c) => (
                <label key={c.key}><input type="checkbox" checked={!colDrag.hidden.includes(c.key)} onChange={() => colDrag.toggleHidden(c.key)} /> {c.label}</label>
              ))}
              <button type="button" className="tiny" disabled={!colDrag.changed} onClick={colDrag.reset} title="Put the columns back in their usual order, widths and visibility">Reset columns</button>
            </div>
          )}
        </span>
        {!slot && <button type="button" className={`tiny${toggles.full ? " on" : ""}`} aria-pressed={!!toggles.full}
          title="Full view adds a summary under the list: bands, modes, first QSO and what is confirmed" onClick={() => setToggle("full")}>Full</button>}
        {slot && <span className="wb-slot" title="Every QSO in this DXCC slot, with any station">{slot.name} {slot.band} {slot.group}{loadedFor === target ? `, ${all.length} QSOs` : ""}</span>}
        {filtered && <span className="small muted">showing {shown.length}</span>}
        <span className="spacer" />
        <span className="wb-toggles" role="group" aria-label="Filter">
          {slot && (
            <button type="button" className="tiny" title="Go back to the history with the call in the entry" onClick={onSlotClear}>
              {entryCall ? `Back to ${entryCall}` : "Back"}
            </button>
          )}
          {!slot && <button type="button" className={`tiny${toggles.band ? " on" : ""}`} aria-pressed={toggles.band} disabled={!entry.band}
            title={entry.band ? `Only QSOs on ${entry.band}` : "No band in the entry"} onClick={() => setToggle("band")}>This band</button>}
          {!slot && <button type="button" className={`tiny${toggles.mode ? " on" : ""}`} aria-pressed={toggles.mode} disabled={!entry.mode}
            title={entry.mode ? `Only QSOs in ${entry.mode}` : "No mode in the entry"} onClick={() => setToggle("mode")}>This mode</button>}
          <button type="button" className={`tiny${toggles.unconfirmed ? " on" : ""}`} aria-pressed={toggles.unconfirmed}
            title="Only QSOs not confirmed by LoTW, card or eQSL" onClick={() => setToggle("unconfirmed")}>Unconfirmed</button>
        </span>
      </div>
      <div className="wb-body">{body}</div>
      {menu && (
        <div
          className="wb-menu"
          role="menu"
          style={{ left: Math.min(menu.x, window.innerWidth - 240), top: Math.min(menu.y, window.innerHeight - 150) }}
          onContextMenu={(e) => e.preventDefault()}
        >
          <button type="button" role="menuitem" autoFocus onClick={() => { setMenu(null); onEdit(menu.q); }}>Edit QSO</button>
          {!slot && <button type="button" role="menuitem" onClick={() => { setMenu(null); onCopy(copyFields(menu.q.fields)); }}>Copy name and QTH to entry</button>}
          <button type="button" role="menuitem" onClick={() => { setMenu(null); onShowInLog(menuCall); }}>Show all with {menuCall} in log</button>
          <hr />
          <button type="button" role="menuitem" disabled={yes(menu.q.fields.QSL_RCVD)}
            onClick={() => mark(menu.q, { QSL_RCVD: "Y", QSLRDATE: adifDateTime(new Date()).date })}>Mark card received</button>
          <button type="button" role="menuitem" disabled={menu.q.fields.QSL_SENT === "Y" || menu.q.fields.QSL_SENT === "Q"}
            onClick={() => mark(menu.q, { QSL_SENT: "Q" })}>Queue a card</button>
        </div>
      )}
    </section>
  );
}
