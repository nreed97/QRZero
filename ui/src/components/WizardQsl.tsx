import { useEffect, useState } from "react";
import { api } from "../api";
import type { QslConfig, QslOverview } from "../types";

interface Props {
  call: string;
  locationId: number | null;
  locationName: string;
  /** Set by this step; the wizard calls it when moving on. */
  saveRef: { current: (() => Promise<void>) | null };
}

/** Compact QSL service setup for the wizard. The QSL window has every option. */
export default function WizardQsl({ call, locationId, locationName, saveRef }: Props) {
  const [o, setO] = useState<QslOverview | null>(null);
  const [cfg, setCfg] = useState<QslConfig | null>(null);
  const [err, setErr] = useState("");
  const [use, setUse] = useState({ clublog: false, qrz: false, lotw: false, eqsl: false });
  const [clPass, setClPass] = useState("");
  const [qrzKey, setQrzKey] = useState("");
  const [lotwPass, setLotwPass] = useState("");
  const [eqslPass, setEqslPass] = useState("");

  useEffect(() => {
    api.qsl().then((r) => {
      setO(r);
      setCfg(r.config);
      setUse({
        clublog: r.config.clublog_enabled,
        qrz: r.config.qrz_enabled,
        lotw: r.config.lotw.length > 0 || !!r.config.lotw_username,
        eqsl: r.config.eqsl_enabled,
      });
    }, (e) => setErr((e as Error).message));
  }, []);

  const set = (patch: Partial<QslConfig>) => cfg && setCfg({ ...cfg, ...patch });
  const flip = (k: keyof typeof use, on: boolean) => setUse({ ...use, [k]: on });

  saveRef.current = async () => {
    if (!cfg || !o) return;
    const touched = Object.values(use).some(Boolean) || o.config.clublog_enabled || o.config.qrz_enabled || o.config.eqsl_enabled;
    if (!touched) return;
    const withCall = (list: string[]) => (list.includes(call) ? list : [...list, call]);
    const next: QslConfig = {
      ...cfg,
      clublog_enabled: use.clublog,
      clublog_live: use.clublog,
      clublog_calls: use.clublog && call ? withCall(cfg.clublog_calls) : cfg.clublog_calls,
      qrz_enabled: use.qrz,
      qrz_live: use.qrz,
      qrz_calls: use.qrz && call ? withCall(cfg.qrz_calls) : cfg.qrz_calls,
      eqsl_enabled: use.eqsl,
      eqsl_live: use.eqsl,
      eqsl_calls: use.eqsl && call ? withCall(cfg.eqsl_calls) : cfg.eqsl_calls,
    };
    await api.saveQsl(next, {
      qrz_keys: use.qrz && qrzKey.trim() && call ? { [call]: qrzKey.trim() } : {},
      ...(use.clublog && clPass ? { clublog_password: clPass } : {}),
      ...(use.lotw && lotwPass ? { lotw_password: lotwPass } : {}),
      ...(use.eqsl && eqslPass ? { eqsl_password: eqslPass } : {}),
    });
  };

  if (err) return <p className="err">{err}</p>;
  if (!o || !cfg) return <p className="muted">Loading…</p>;

  const mine = cfg.lotw.find((m) => m.callsign === call && m.location_id === locationId);
  const setMapping = (name: string) => {
    if (!locationId) return;
    const rest = cfg.lotw.filter((m) => !(m.callsign === call && m.location_id === locationId));
    set({ lotw: name ? [...rest, { callsign: call, location_id: locationId, station_location: name }] : rest });
  };

  return (
    <>
      <h3>Confirmations</h3>
      <p>Tick the services you use and QRZero sends new QSOs to them a couple of minutes after you log. Everything is optional; the QSL window in the ☰ menu has the full options, including paper cards.</p>

      <fieldset>
        <legend><label className="check"><input type="checkbox" checked={use.clublog} onChange={(e) => flip("clublog", e.target.checked)} /> Club Log</label></legend>
        {use.clublog && (
          <div className="row">
            <label className="f w-l"><span>Email</span><input value={cfg.clublog_email} onChange={(e) => set({ clublog_email: e.target.value })} /></label>
            <label className="f w-m"><span>Password</span><input type="password" value={clPass} placeholder={o.secrets.clublog_password ? "saved" : ""} onChange={(e) => setClPass(e.target.value)} /></label>
          </div>
        )}
      </fieldset>

      <fieldset>
        <legend><label className="check"><input type="checkbox" checked={use.qrz} onChange={(e) => flip("qrz", e.target.checked)} /> QRZ Logbook</label></legend>
        {use.qrz && (
          <>
            <div className="row">
              <label className="f w-xl"><span>API key for {call || "your callsign"} (QRZ Logbook, Settings, API)</span>
                <input type="password" value={qrzKey} placeholder={o.secrets.qrz_calls.includes(call) ? "API key saved" : ""} onChange={(e) => setQrzKey(e.target.value)} /></label>
            </div>
          </>
        )}
      </fieldset>

      <fieldset>
        <legend><label className="check"><input type="checkbox" checked={use.lotw} onChange={(e) => flip("lotw", e.target.checked)} /> LoTW</label></legend>
        {use.lotw && (
          <>
            <p className="small muted">Uploads are signed by TQSL and sent when you press the button in the QSL window; LoTW is never live. Enter your LoTW website login (not your TQSL password) to download confirmations. Pick the TQSL station location for {locationName}. {o.tqsl.found ? "" : "TQSL wasn't found; you can set its path in the QSL window."}</p>
            <div className="row">
              <label className="f w-l"><span>TQSL station location</span>
                {o.tqsl.locations.length ? (
                  <select value={mine?.station_location ?? ""} onChange={(e) => setMapping(e.target.value)}>
                    <option value="">(none)</option>
                    {o.tqsl.locations.map((n) => <option key={n}>{n}</option>)}
                  </select>
                ) : (
                  <input value={mine?.station_location ?? ""} placeholder="name in TQSL" onChange={(e) => setMapping(e.target.value)} />
                )}
              </label>
            </div>
            <div className="row">
              <label className="f w-m"><span>LoTW username</span><input value={cfg.lotw_username} onChange={(e) => set({ lotw_username: e.target.value })} /></label>
              <label className="f w-m"><span>Password</span><input type="password" value={lotwPass} placeholder={o.secrets.lotw_password ? "saved" : ""} onChange={(e) => setLotwPass(e.target.value)} /></label>
            </div>
            <label className="check"><input type="checkbox" checked={cfg.lotw_download_enabled} onChange={(e) => set({ lotw_download_enabled: e.target.checked })} /> Download LoTW confirmations automatically</label>
          </>
        )}
      </fieldset>

      <fieldset>
        <legend><label className="check"><input type="checkbox" checked={use.eqsl} onChange={(e) => flip("eqsl", e.target.checked)} /> eQSL</label></legend>
        {use.eqsl && (
          <div className="row">
            <label className="f w-m"><span>Username</span><input value={cfg.eqsl_username} onChange={(e) => set({ eqsl_username: e.target.value.toUpperCase() })} /></label>
            <label className="f w-m"><span>Password</span><input type="password" value={eqslPass} placeholder={o.secrets.eqsl_password ? "saved" : ""} onChange={(e) => setEqslPass(e.target.value)} /></label>
          </div>
        )}
      </fieldset>
    </>
  );
}
