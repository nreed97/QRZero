import { useState } from "react";
import { api } from "../api";
import { PRESETS, type EntryLayout } from "../fields";
import { gridToLatLon } from "../geo";
import type { Equipment, Fields, ImportReport, Location, StationCallsign } from "../types";
import Modal from "./Modal";

interface Props {
  logId: number;
  callsigns: StationCallsign[];
  locations: Location[];
  equipment: Equipment[];
  onChanged: () => Promise<void> | void;
  onLayout: (l: EntryLayout) => void;
  onClose: () => void;
}

const STEPS = ["Your callsign", "Callsign lookup", "Home location", "Equipment", "Entry fields", "Import a log", "Done"];

const LOCATION_FROM_LOOKUP: [string, string][] = [
  ["GRIDSQUARE", "MY_GRIDSQUARE"], ["QTH", "MY_CITY"], ["STATE", "MY_STATE"], ["CNTY", "MY_CNTY"],
  ["COUNTRY", "MY_COUNTRY"], ["DXCC", "MY_DXCC"], ["CQZ", "MY_CQ_ZONE"], ["ITUZ", "MY_ITU_ZONE"],
];

export default function SetupWizard({ logId, callsigns, locations, equipment, onChanged, onLayout, onClose }: Props) {
  const home = locations.find((l) => l.is_default) ?? locations[0];
  const [step, setStep] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const [call, setCall] = useState(callsigns.find((c) => c.is_default)?.callsign ?? "");
  const [previous, setPrevious] = useState(callsigns.filter((c) => !c.is_default).map((c) => c.callsign).join(", "));

  const [qrzOn, setQrzOn] = useState(false);
  const [qrzUser, setQrzUser] = useState("");
  const [qrzPass, setQrzPass] = useState("");
  const [qrzMsg, setQrzMsg] = useState("");

  const [locName, setLocName] = useState(home?.name ?? "Home");
  const [loc, setLoc] = useState<Fields>(home?.fields ?? {});
  const [locId, setLocId] = useState<number | null>(home?.id ?? null);

  const [gearKind, setGearKind] = useState("rig");
  const [gearName, setGearName] = useState("");
  const [gearPower, setGearPower] = useState("");

  const [preset, setPreset] = useState(PRESETS[0].id);
  const [file, setFile] = useState<File | null>(null);
  const [report, setReport] = useState<ImportReport | null>(null);

  const guard = async (fn: () => Promise<boolean | void>) => {
    setBusy(true);
    setError("");
    try {
      const advance = await fn();
      if (advance !== false) setStep((s) => Math.min(STEPS.length - 1, s + 1));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const next = () =>
    guard(async () => {
      switch (step) {
        case 0: {
          const calls = [call, ...previous.split(/[\s,;]+/)].map((c) => c.trim().toUpperCase()).filter(Boolean);
          if (!calls.length) throw new Error("Enter your callsign.");
          const have = new Set(callsigns.map((c) => c.callsign));
          for (const c of calls) if (!have.has(c)) await api.addCallsign(logId, c);
          const added = await api.callsigns(logId);
          const mine = added.find((c) => c.callsign === calls[0]);
          if (mine && !mine.is_default) await api.defaultCallsign(mine.id);
          await onChanged();
          return;
        }
        case 1: {
          if (!qrzOn) return;
          await api.saveSettings({ qrz_enabled: true, qrz_username: qrzUser, ...(qrzPass ? { qrz_password: qrzPass } : {}) });
          // Use your own QRZ record to fill in the home location.
          const r = await api.lookup(logId, call.trim().toUpperCase());
          if (r.error) {
            setQrzMsg(`QRZ said: ${r.error}. Check the details, or untick lookups to continue without them.`);
            return false;
          }
          if (r.station) {
            const filled = { ...loc };
            for (const [from, to] of LOCATION_FROM_LOOKUP) if (r.station[from] && !filled[to]) filled[to] = r.station[from];
            setLoc(filled);
          }
          return;
        }
        case 2: {
          if (!locName.trim()) throw new Error("Give the location a name.");
          if (loc.MY_GRIDSQUARE && !gridToLatLon(loc.MY_GRIDSQUARE)) throw new Error("That grid doesn't look right. It should be like FN31 or FN31pr.");
          if (locId === null) {
            const created = await api.createLocation(logId, locName, loc);
            setLocId(created.id);
            if (!created.is_default) await api.defaultLocation(created.id);
          } else {
            await api.updateLocation(locId, locName, loc);
          }
          await onChanged();
          return;
        }
        case 4: {
          const p = PRESETS.find((x) => x.id === preset);
          if (p) onLayout(p.layout);
          return;
        }
        case 5: {
          if (!file || report) return;
          const r = await api.importAdif(logId, file, {
            ...(locId ? { location_id: String(locId) } : {}),
            apply_location: "fill",
            skip_duplicates: "true",
            add_station_callsigns: "true",
          });
          setReport(r);
          await onChanged();
          return false;
        }
      }
    });

  const addGear = () =>
    guard(async () => {
      if (!locId || !gearName.trim()) return false;
      await api.createEquipment(locId, gearKind, gearName, gearPower ? { POWER_W: gearPower } : {});
      setGearName("");
      setGearPower("");
      await onChanged();
      return false;
    });

  const locField = (key: string, label: string, cls = "w-m") => (
    <label className={`f ${cls}`}>
      <span>{label}</span>
      <input value={loc[key] ?? ""} onChange={(e) => setLoc({ ...loc, [key]: e.target.value })} />
    </label>
  );

  const myGear = equipment.filter((e) => e.location_id === locId);

  return (
    <Modal title="QRZero setup" onClose={onClose} wide>
      <div className="wizard">
        <ol className="wizard-steps">
          {STEPS.map((s, i) => (
            <li key={s} className={i === step ? "current" : i < step ? "done" : ""}>{s}</li>
          ))}
        </ol>
        <div className="wizard-page">
          {step === 0 && (
            <>
              <h3>Your callsign</h3>
              <p>QRZero keeps a log per operator. Enter the callsign you operate under now. If you've held other calls, add them too so QSOs from an old log are labelled correctly.</p>
              <div className="row">
                <label className="f w-m"><span>Callsign</span><input value={call} autoFocus onChange={(e) => setCall(e.target.value.toUpperCase())} /></label>
                <label className="f w-xl"><span>Previous callsigns (optional, separated by commas)</span><input value={previous} onChange={(e) => setPrevious(e.target.value.toUpperCase())} /></label>
              </div>
            </>
          )}
          {step === 1 && (
            <>
              <h3>Callsign lookup</h3>
              <p>With a QRZ.com XML subscription, QRZero fills in name, QTH, grid, country and zones as you log. It also uses your own QRZ record to set up your home location in the next step.</p>
              <label className="check"><input type="checkbox" checked={qrzOn} onChange={(e) => setQrzOn(e.target.checked)} /> Look up callsigns on QRZ.com</label>
              {qrzOn && (
                <div className="row">
                  <label className="f w-m"><span>QRZ username</span><input value={qrzUser} onChange={(e) => setQrzUser(e.target.value)} /></label>
                  <label className="f w-m"><span>QRZ password</span><input type="password" value={qrzPass} onChange={(e) => setQrzPass(e.target.value)} /></label>
                </div>
              )}
              {qrzMsg && <p className="err">{qrzMsg}</p>}
              <p className="small muted">You can change this later in Settings, Callsign lookup. On Windows the password is kept in Windows Credential Manager.</p>
            </>
          )}
          {step === 2 && (
            <>
              <h3>Home location</h3>
              <p>Where you usually operate from. These details go into every QSO as your MY_ fields. Add portable or holiday locations later in Settings.</p>
              <div className="row">
                <label className="f w-m"><span>Name</span><input value={locName} onChange={(e) => setLocName(e.target.value)} /></label>
                {locField("MY_GRIDSQUARE", "Grid", "w-s")}
                {locField("MY_CITY", "City")}
                {locField("MY_STATE", "State", "w-s")}
                {locField("MY_CNTY", "County")}
              </div>
              <div className="row">
                {locField("MY_COUNTRY", "Country")}
                {locField("MY_DXCC", "DXCC", "w-s")}
                {locField("MY_CQ_ZONE", "CQ zone", "w-s")}
                {locField("MY_ITU_ZONE", "ITU zone", "w-s")}
              </div>
              <p className="small muted">The grid also places you on the map. Six characters (FN31pr) is more accurate than four.</p>
            </>
          )}
          {step === 3 && (
            <>
              <h3>Equipment</h3>
              <p>Add the radios, antennas and amplifiers at {locName}. You'll pick which ones you're using when you log, and they're saved with each QSO. This is optional, and Settings, Equipment has the full tree for every location.</p>
              <div className="row">
                <label className="f w-m">
                  <span>Type</span>
                  <select value={gearKind} onChange={(e) => setGearKind(e.target.value)}>
                    <option value="rig">Radio</option>
                    <option value="antenna">Antenna</option>
                    <option value="amplifier">Amplifier</option>
                    <option value="rotator">Rotator</option>
                  </select>
                </label>
                <label className="f w-l"><span>Name</span><input value={gearName} placeholder={gearKind === "antenna" ? "e.g. 80m dipole" : "e.g. IC-7610"} onChange={(e) => setGearName(e.target.value)} onKeyDown={(e) => e.key === "Enter" && addGear()} /></label>
                {(gearKind === "rig" || gearKind === "amplifier") && (
                  <label className="f w-s"><span>Power W</span><input value={gearPower} onChange={(e) => setGearPower(e.target.value)} /></label>
                )}
                <div className="actions"><button disabled={!gearName.trim() || busy} onClick={addGear}>Add</button></div>
              </div>
              {myGear.length > 0 && (
                <table className="list">
                  <tbody>
                    {myGear.map((e) => (
                      <tr key={e.id}><td>{{ rig: "Radio", antenna: "Antenna", amplifier: "Amplifier", rotator: "Rotator", other: "Other" }[e.kind]}</td><td><strong>{e.name}</strong></td><td className="muted">{e.fields.POWER_W ? `${e.fields.POWER_W} W` : ""}</td></tr>
                    ))}
                  </tbody>
                </table>
              )}
            </>
          )}
          {step === 4 && (
            <>
              <h3>Entry fields</h3>
              <p>Pick the layout closest to how you operate. You can add, remove and rearrange fields any time in Settings, Entry fields, including fields of your own.</p>
              {PRESETS.map((p) => (
                <label key={p.id} className="preset">
                  <input type="radio" name="preset" checked={preset === p.id} onChange={() => setPreset(p.id)} />
                  <span><strong>{p.name}</strong><br /><span className="muted">{p.description}</span></span>
                </label>
              ))}
            </>
          )}
          {step === 5 && (
            <>
              <h3>Import a log</h3>
              {report ? (
                <p><strong>{report.imported.toLocaleString()}</strong> QSOs imported{report.duplicates ? `, ${report.duplicates.toLocaleString()} duplicates skipped` : ""}{report.rejected ? `, ${report.rejected} records couldn't be read` : ""}.</p>
              ) : (
                <>
                  <p>Coming from Log4OM or another logger? Export your log there as ADIF (.adi) and choose the file here. QSOs are linked to {locName}, and duplicates are skipped. You can also do this later from Import.</p>
                  <input type="file" accept=".adi,.adif,.txt" onChange={(e) => setFile(e.target.files?.[0] ?? null)} />
                </>
              )}
            </>
          )}
          {step === 6 && (
            <>
              <h3>You're ready</h3>
              <ul>
                <li>Type a call, press <kbd>Space</kbd> or <kbd>Tab</kbd> to look it up, and <kbd>Enter</kbd> to log.</li>
                <li><kbd>Esc</kbd> clears the form. <kbd>F1</kbd> opens the user guide.</li>
                <li>Double-click a QSO in the log to edit it. Use Columns to choose what the log shows.</li>
                <li>Everything here can be changed in Settings, and this wizard can be run again from Settings, General.</li>
              </ul>
            </>
          )}
          {error && <p className="err">{error}</p>}
        </div>
      </div>
      <div className="buttons">
        <span className="muted">Step {step + 1} of {STEPS.length}</span>
        {step > 0 && step < STEPS.length - 1 && <button onClick={() => setStep(step - 1)} disabled={busy}>Back</button>}
        {(step === 1 || step === 3 || step === 5) && !report && <button onClick={() => setStep(step + 1)} disabled={busy}>Skip</button>}
        {step === 5 && file && !report && <button onClick={next} disabled={busy}>{busy ? "Importing…" : "Import"}</button>}
        {step < STEPS.length - 1 ? (
          !(step === 5 && file && !report) && (
            <button className="primary" onClick={step === 5 ? () => setStep(6) : next} disabled={busy || (step === 0 && !call.trim())}>Next</button>
          )
        ) : (
          <button className="primary" onClick={onClose}>Start logging</button>
        )}
      </div>
    </Modal>
  );
}
