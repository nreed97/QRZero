import { useState, type FormEvent } from "react";
import { api } from "../api";
import { qsoStage } from "../ftxStage";
import type { FtxInstance } from "../types";

const SPECIAL = ["", "NA VHF contest", "EU VHF contest", "Field Day", "RTTY Roundup", "WW Digi contest", "Fox", "Hound"];
const MODES = ["FT8", "FT4"];

/**
 * One line per WSJT-X / JTDX: what it is doing and where it is in the QSO, with the controls
 * its UDP interface allows. Everything here is a request; WSJT-X only acts on it with
 * "Accept UDP requests" ticked.
 */
export default function FtxControls({ instances, mycall, label, tagFor, describe, onMsg }: {
  instances: FtxInstance[];
  mycall: string;
  label: (i: FtxInstance) => string;
  tagFor: (i: FtxInstance) => string;
  describe: (i: FtxInstance) => string;
  onMsg: (text: string) => void;
}) {
  if (instances.length === 0) return null;
  return (
    <div className="ftx-strip" aria-label="WSJT-X control">
      {instances.map((i) => <InstanceLine key={i.id} i={i} mycall={mycall} label={label(i)} tag={tagFor(i)} title={describe(i)} onMsg={onMsg} />)}
    </div>
  );
}

function InstanceLine({ i, mycall, label, tag, title, onMsg }: {
  i: FtxInstance;
  mycall: string;
  label: string;
  tag: string;
  title: string;
  onMsg: (text: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const [dx, setDx] = useState("");
  const [dxGrid, setDxGrid] = useState("");
  const [rx, setRx] = useState("");
  const [config, setConfig] = useState("");
  const call = (i.de_call || mycall).toUpperCase();
  const stage = qsoStage({ tx_message: i.tx_message, dx_call: i.dx_call, mycall: call, tx_enabled: i.tx_enabled, transmitting: i.transmitting });
  const name = label;

  const run = async (what: string, f: () => Promise<unknown>) => {
    try {
      await f();
      onMsg(`${what} (${name}).`);
    } catch (e) {
      onMsg(`${name}: ${(e as Error).message}`);
    }
  };
  const halt = (autoOnly: boolean) => run(autoOnly ? "Asked to stop after this transmission" : "Asked to halt TX", () => api.ftxHalt(i.id, autoOnly));
  const cq = () => {
    if (!call) return onMsg("Set your callsign first.");
    const msg = ["CQ", call, i.de_grid.slice(0, 4)].filter(Boolean).join(" ");
    return run(`Asked to send ${msg}`, () => api.ftxFreeText(i.id, msg, true));
  };
  const sendText = (send: boolean) => (e?: FormEvent) => {
    e?.preventDefault();
    const t = text.trim();
    if (!t) return;
    void run(send ? `Asked to send ${t.toUpperCase()}` : `Set Tx5 to ${t.toUpperCase()}`, () => api.ftxFreeText(i.id, t, send));
  };
  const setDxCall = (e: FormEvent) => {
    e.preventDefault();
    if (!dx.trim()) return;
    void run(`Set DX call ${dx.trim().toUpperCase()}`, () => api.ftxConfigure(i.id, { dx_call: dx, dx_grid: dxGrid || undefined, generate_messages: true }));
  };
  const setRxDf = (e: FormEvent) => {
    e.preventDefault();
    const hz = Number(rx);
    if (!rx.trim() || !Number.isInteger(hz) || hz < 0 || hz > 5000) return onMsg("Rx offset is 0 to 5000 Hz.");
    void run(`Set Rx offset to ${hz} Hz`, () => api.ftxConfigure(i.id, { rx_df: hz }));
  };
  const switchConfig = (e: FormEvent) => {
    e.preventDefault();
    if (!config.trim()) return;
    void run(`Asked to switch to configuration ${config.trim()}`, () => api.ftxSwitchConfiguration(i.id, config));
  };

  const state = i.transmitting ? "TX" : i.tx_enabled ? "Tx on" : "Tx off";
  const modes = MODES.includes(i.mode) || !i.mode ? MODES : [i.mode, ...MODES];
  return (
    <div className={`ftx-inst src-${i.color_index % 8} ${i.transmitting ? "on-air" : ""}`} role="group" aria-label={`Control ${name}`}>
      <div className="ftx-inst-line">
        <span className="src-tag mono" title={title}>{tag}</span>
        <strong className="who" title={title}>{name}</strong>
        <span className="mono">{i.band ?? ""} {i.mode} {(i.dial_freq / 1e6).toFixed(3)}</span>
        <span className={`state ${i.transmitting ? "tx" : i.tx_enabled ? "armed" : ""}`} title={i.transmitting ? "On the air" : i.tx_enabled ? "Enable Tx is ticked" : "Enable Tx is off"} data-testid="ftx-state">{state}</span>
        {i.decoding && <span className="muted">Decoding</span>}
        <span className="mono txmsg" title="The message being sent, or to be sent next" data-testid="ftx-txmsg">Tx: {i.tx_message || "-"}</span>
        <span className={`stage stage-${stage.kind}`} title="Where the QSO is, from the message being sent" data-testid="ftx-stage">{stage.text}</span>
        {(i.dx_call || i.dx_grid) && <span className="mono" title="DX call and grid">DX {i.dx_call} {i.dx_grid}</span>}
        {i.report && <span className="mono" title="Report to send">Rpt {i.report}</span>}
        <span className="mono" title="Rx and Tx audio offsets, Hz">Rx {i.rx_df} Tx {i.tx_df}</span>
        {i.tr_period > 0 && <span className="mono" title="T/R period">{i.mode === "FT4" ? "7.5" : i.tr_period} s</span>}
        {i.special_op_mode > 0 && <span title="Special operating activity">{SPECIAL[i.special_op_mode] ?? `Special ${i.special_op_mode}`}</span>}
        {i.tx_watchdog && <span className="warn" title="WSJT-X's Tx watchdog stopped transmitting. Tick Enable Tx in WSJT-X, or answer a station, to carry on.">Tx watchdog</span>}
        <span className="spacer" />
        <button className="halt" onClick={() => void halt(false)} title="Stop transmitting now (Halt Tx)">Halt TX</button>
        <button onClick={() => void halt(true)} title="Untick Enable Tx so this transmission finishes and no more follow">Stop after this</button>
        <button onClick={() => void cq()} title={`Send "CQ ${call || "your call"} ${i.de_grid.slice(0, 4)}" as free text. WSJT-X transmits it while Enable Tx is on.`}>Call CQ</button>
        <button onClick={() => setOpen(!open)} aria-expanded={open} title="Free text, DX call, Rx offset, mode and more">{open ? "Fewer controls" : "More controls"}</button>
      </div>
      {open && (
        <div className="ftx-inst-ctl">
          <form onSubmit={sendText(true)}>
            <label>Message <input className="mono" value={text} onChange={(e) => setText(e.target.value)} maxLength={37} placeholder="free text" aria-label="Free text message" title="Free text is up to 13 characters; a standard message such as K1ABC N0CALL RR73 can be longer" /></label>
            <button type="submit" title="Put it in Tx5 and send it next, while Enable Tx is on">Send</button>
            <button type="button" onClick={() => sendText(false)()} title="Put it in Tx5 without sending it">Set Tx5</button>
          </form>
          <form onSubmit={setDxCall}>
            <label>DX call <input className="mono narrow" value={dx} onChange={(e) => setDx(e.target.value)} aria-label="DX call" /></label>
            <label>Grid <input className="mono narrow" value={dxGrid} onChange={(e) => setDxGrid(e.target.value)} aria-label="DX grid" /></label>
            <button type="submit" title="Fill in WSJT-X's DX call (and grid) and generate the standard messages. WSJT-X can't be asked to clear the DX call.">Set DX</button>
          </form>
          <form onSubmit={setRxDf}>
            <label>Rx offset <input className="mono narrow" value={rx} onChange={(e) => setRx(e.target.value)} placeholder={String(i.rx_df)} inputMode="numeric" aria-label="Rx offset" /></label>
            <button type="submit" title="Move WSJT-X's Rx frequency. The Tx offset can't be changed over UDP: set it in WSJT-X (Shift+click the waterfall, or Rx to Tx).">Set Rx</button>
          </form>
          <label>
            Mode{" "}
            <select value={i.mode} onChange={(e) => void run(`Asked to switch to ${e.target.value}`, () => api.ftxConfigure(i.id, { mode: e.target.value }))} aria-label="Mode">
              {modes.map((m) => <option key={m}>{m}</option>)}
            </select>
          </label>
          <form onSubmit={switchConfig}>
            <label>Configuration <input className="narrow" value={config} onChange={(e) => setConfig(e.target.value)} placeholder={i.configuration_name || "name"} aria-label="Configuration" /></label>
            <button type="submit" title="Switch to another of WSJT-X's configurations, by its exact name (File, Settings, Configurations). JTDX ignores this.">Switch</button>
          </form>
          <button onClick={() => void run("Asked to send its decodes again", () => api.ftxReplay(i.id))} title="WSJT-X sends everything in its Band Activity window again; decodes already listed aren't repeated">Replay</button>
          <button onClick={() => void run("Cleared its windows", () => api.ftxClear(i.id, 2))} title="Clear WSJT-X's Band Activity and Rx Frequency windows, and this program's decodes here">Clear windows</button>
        </div>
      )}
    </div>
  );
}
