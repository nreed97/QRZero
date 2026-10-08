import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { BANDS, modeLabel } from "../modes";
import { localGet, localSet } from "../prefs";
import type { Fields, LookupResult, Qso } from "../types";
import { adifDateTime, fmtDate, fmtTime } from "../util";
import type { EntryContext } from "./EntryPanel";
import { useDisplay } from "../display";
import "../worked.css";

export interface WorkedPaneProps {
  logId: number;
  call: string;                      // the call currently in the entry panel ("" when empty)
  lookup: LookupResult | null;       // has worked.call_count etc.
  entry: EntryContext;               // from ./EntryPanel: current band, mode, fields
  refreshKey: number;                // bumps when QSOs are logged or edited: reload
  selectedId?: number | null;        // QSO open in the editor, highlight it
  onEdit: (q: Qso) => void;          // open the QSO in the editor
  onCopy: (fields: Fields) => void;  // copy NAME, QTH, GRIDSQUARE, STATE, CNTY etc. into the entry panel
  onShowInLog: (call: string) => void;
}

/** Station details worth carrying over from an earlier QSO. */
export const WORKED_COPY_FIELDS = ["NAME", "QTH", "GRIDSQUARE", "STATE", "CNTY", "QSL_VIA"] as const;

interface Toggles { band: boolean; mode: boolean; unconfirmed: boolean }
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

interface Menu { x: number; y: number; q: Qso }

export default function WorkedPane({ logId, call, lookup, entry, refreshKey, selectedId, onEdit, onCopy, onShowInLog }: WorkedPaneProps) {
  useDisplay();
  const target = call.trim().toUpperCase();
  const [rows, setRows] = useState<Qso[] | null>(null);
  const [loadedFor, setLoadedFor] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [reload, setReload] = useState(0);
  const [toggles, setToggles] = useState<Toggles>(() => localGet(PREF_KEY, NO_TOGGLES));
  const [sel, setSel] = useState<number | null>(selectedId ?? null);
  const [menu, setMenu] = useState<Menu | null>(null);
  const bodyRef = useRef<HTMLDivElement>(null);

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
        .search(logId, { exact_call: target }, 0, LIMIT, "newest")
        .then((r) => {
          if (!live) return;
          setRows(r.rows);
          setLoadedFor(target);
          setError(null);
        })
        .catch((e) => live && setError(String(e.message ?? e)));
    }, 200);
    return () => {
      live = false;
      clearTimeout(timer);
    };
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
    const recent = lookup?.worked.recent ?? [];
    return recent.length && recent[0].fields.CALL?.toUpperCase() === target ? recent : [];
  }, [rows, loadedFor, target, lookup]);
  const loading = !!target && loadedFor !== target && !error;

  const band = entry.band.toLowerCase();
  const mode = entry.mode.toUpperCase();
  const isSlot = (f: Fields) => !!band && !!mode && bandOf(f) === band && modeLabel(f).toUpperCase() === mode;

  const shown = useMemo(
    () =>
      all.filter(
        (q) =>
          (!toggles.band || !band || bandOf(q.fields) === band) &&
          (!toggles.mode || !mode || modeLabel(q.fields).toUpperCase() === mode) &&
          (!toggles.unconfirmed || !confirmed(q.fields)),
      ),
    [all, toggles, band, mode],
  );

  const summary = useMemo(() => {
    const bands = [...new Set(all.map((q) => bandOf(q.fields)).filter(Boolean))].sort((a, b) => bandRank(a) - bandRank(b));
    const modes = [...new Set(all.map((q) => modeLabel(q.fields)).filter(Boolean))].sort();
    const slots = new Set<string>();
    for (const q of all) if (confirmed(q.fields) && bandOf(q.fields)) slots.add(`${bandOf(q.fields)} ${modeLabel(q.fields)}`.trim());
    const conf = [...slots].sort((a, b) => bandRank(a.split(" ")[0]) - bandRank(b.split(" ")[0]) || a.localeCompare(b));
    const first = all.length ? fmtDate(all[all.length - 1].fields) : "";
    return { bands, modes, conf, first };
  }, [all]);

  // Keep the selected row in view when moving with the keyboard.
  useEffect(() => {
    if (sel === null) return;
    bodyRef.current?.querySelector<HTMLElement>(`tr[data-id="${sel}"]`)?.scrollIntoView({ block: "nearest" });
  }, [sel]);

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

  const title = target ? `Worked before: ${target}${loadedFor === target ? ` (${all.length})` : ""}` : "Worked before";
  const filtered = shown.length !== all.length;

  let body: React.ReactNode;
  if (!target) body = <div className="wb-empty muted">Type a call to see your history with it.</div>;
  else if (error && !all.length) body = <div className="wb-empty err">Could not load the history: {error}</div>;
  else if (!all.length) body = <div className="wb-empty muted">{loading ? "Loading..." : `No previous QSOs with ${target}.`}</div>;
  else
    body = (
      <>
        <div className="wb-scroll" ref={bodyRef} tabIndex={0} onKeyDown={onKeyDown} aria-label={`QSOs with ${target}`}>
          <table className="wb-table">
            <thead>
              <tr>
                <th>Date</th>
                <th>UTC</th>
                <th>Band</th>
                <th>Mode</th>
                <th>Sent</th>
                <th>Rcvd</th>
                <th className="wb-qsl">LoTW</th>
                <th className="wb-qsl">Card</th>
                <th className="wb-qsl">eQSL</th>
                <th>Name</th>
                <th className="wb-wide">Comment</th>
                <th className="wb-act" aria-label="Actions" />
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
                    <td>{fmtDate(f)}</td>
                    <td>{fmtTime(f)}</td>
                    <td>{f.BAND}</td>
                    <td>{modeLabel(f)}</td>
                    <td>{f.RST_SENT}</td>
                    <td>{f.RST_RCVD}</td>
                    <QslCell v={lotw(f)} />
                    <QslCell v={card(f)} />
                    <QslCell v={eqsl(f)} />
                    <td className="wb-text" title={f.NAME}>{f.NAME}</td>
                    <td className="wb-text wb-wide" title={f.COMMENT}>
                      {f.CALL && f.CALL.toUpperCase() !== target && <span className="wb-as">as {f.CALL}</span>}
                      {f.COMMENT}
                    </td>
                    <td className="wb-act">
                      <span className="wb-btns">
                        <button type="button" className="tiny" onClick={(e) => { e.stopPropagation(); onEdit(q); }}>Edit</button>
                        <button type="button" className="tiny" title="Copy name, QTH, grid, state, county and QSL via into the entry" onClick={(e) => { e.stopPropagation(); onCopy(copyFields(f)); }}>Copy</button>
                      </span>
                    </td>
                  </tr>
                );
              })}
              {!shown.length && (
                <tr className="wb-none">
                  <td colSpan={12} className="muted">No QSOs match the filters.</td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
        <div className="wb-summary">
          <span><b>Bands</b> {summary.bands.join(" ") || "none"}</span>
          <span><b>Modes</b> {summary.modes.join(" ") || "none"}</span>
          <span><b>First QSO</b> {summary.first || "unknown"}</span>
          <span><b>Confirmed</b> {summary.conf.length ? <span className="wb-conf">{summary.conf.join(", ")}</span> : "none"}</span>
          {error && <span className="err">{error}</span>}
        </div>
      </>
    );

  return (
    <section className="panel worked-pane">
      <div className="panel-title">
        <span className="wb-title">{title}</span>
        {filtered && <span className="small muted">showing {shown.length}</span>}
        <span className="spacer" />
        <span className="wb-toggles" role="group" aria-label="Filter">
          <button type="button" className={`tiny${toggles.band ? " on" : ""}`} aria-pressed={toggles.band} disabled={!entry.band}
            title={entry.band ? `Only QSOs on ${entry.band}` : "No band in the entry"} onClick={() => setToggle("band")}>This band</button>
          <button type="button" className={`tiny${toggles.mode ? " on" : ""}`} aria-pressed={toggles.mode} disabled={!entry.mode}
            title={entry.mode ? `Only QSOs in ${entry.mode}` : "No mode in the entry"} onClick={() => setToggle("mode")}>This mode</button>
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
          <button type="button" role="menuitem" onClick={() => { setMenu(null); onCopy(copyFields(menu.q.fields)); }}>Copy name and QTH to entry</button>
          <button type="button" role="menuitem" onClick={() => { setMenu(null); onShowInLog(target); }}>Show all with {target} in log</button>
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
