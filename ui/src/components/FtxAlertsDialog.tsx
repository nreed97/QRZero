import type { CSSProperties } from "react";
import { ALERTS, CONTINENTS, DEFAULT_ALERTS, type AlertKind, type FtxAlertConfig } from "../ftxAlerts";
import Modal from "./Modal";

/** Which FTx decodes stand out and in what colour, and which are hidden. Changes apply at once. */
export default function FtxAlertsDialog({ cfg, onChange, onClose }: { cfg: FtxAlertConfig; onChange: (c: FtxAlertConfig) => void; onClose: () => void }) {
  const set = (c: Partial<FtxAlertConfig>) => onChange({ ...cfg, ...c });
  const setAlert = (k: AlertKind, a: Partial<FtxAlertConfig["alerts"][AlertKind]>) =>
    set({ alerts: { ...cfg.alerts, [k]: { ...cfg.alerts[k], ...a } } });
  const toggleCont = (c: string) =>
    set({ continents: cfg.continents.includes(c) ? cfg.continents.filter((x) => x !== c) : [...cfg.continents, c] });

  return (
    <Modal title="FTx alerts and filters" onClose={onClose}>
      <div className="ftx-alerts">
        <h3>Alerts</h3>
        <p className="small muted">
          A station gets the colour of the first ticked alert that fits it, top to bottom. In call boxes the short tag is shown too.
          Calling CQ is shown as a line under the box.
        </p>
        <table className="list">
          <tbody>
            {ALERTS.map((a) => (
              <tr key={a.kind}>
                <td>
                  <label className="check" title={a.help}>
                    <input type="checkbox" checked={cfg.alerts[a.kind].on} onChange={(e) => setAlert(a.kind, { on: e.target.checked })} /> {a.name}
                  </label>
                </td>
                <td className="small muted">{a.help}</td>
                <td>
                  <span
                    className={`ftx-box sample ${a.kind === "cq" ? "cq" : "alerted"}`}
                    style={{ [a.kind === "cq" ? "--cq" : "--alert"]: cfg.alerts[a.kind].color } as CSSProperties}
                  >
                    <span className="call">{a.kind === "cq" ? "CQ" : a.tag}</span>
                  </span>
                </td>
                <td>
                  <input type="color" value={cfg.alerts[a.kind].color} onChange={(e) => setAlert(a.kind, { color: e.target.value })} aria-label={`${a.name} colour`} />
                </td>
              </tr>
            ))}
          </tbody>
        </table>

        <h3>Filters</h3>
        <p className="small muted">These apply to both views. Stations calling you are always shown.</p>
        <div className="form-grid">
          <label className="check">
            <input type="checkbox" checked={cfg.hideWorked} onChange={(e) => set({ hideWorked: e.target.checked })} /> Hide stations already worked on this band
          </label>
          <label>
            Weakest signal to show, dB{" "}
            <input
              type="number"
              className="narrow"
              value={cfg.minSnr ?? ""}
              placeholder="any"
              min={-30}
              max={30}
              onChange={(e) => set({ minSnr: e.target.value === "" ? null : Number(e.target.value) })}
            />
          </label>
          <fieldset>
            <legend>Continents to show (none ticked shows all)</legend>
            {CONTINENTS.map((c) => (
              <label key={c} className="check">
                <input type="checkbox" checked={cfg.continents.includes(c)} onChange={() => toggleCont(c)} /> {c}
              </label>
            ))}
          </fieldset>
          <label>
            Calls to ignore{" "}
            <input value={cfg.ignore} placeholder="e.g. K1ABC, W1AW, VE*" onChange={(e) => set({ ignore: e.target.value })} />
          </label>
        </div>
        <div className="row">
          <button onClick={() => onChange(DEFAULT_ALERTS)}>Reset</button>
          <span className="spacer" />
          <button className="primary" onClick={onClose}>Done</button>
        </div>
      </div>
    </Modal>
  );
}
