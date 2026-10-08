// Settings, Backups: back up the log database, keep daily copies, restore.
import { useEffect, useState } from "react";
import { backups, type BackupKind, type BackupOverview } from "../backups";
import { download } from "../util";
import "../backups.css";

const KIND_NAMES: Record<BackupKind, string> = { auto: "Automatic", manual: "Manual", before_restore: "Before restore" };

const pad = (n: number) => String(n).padStart(2, "0");
function localTime(ts: number): string {
  const d = new Date(ts * 1000);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

function size(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export default function BackupsTab() {
  const [o, setO] = useState<BackupOverview | null>(null);
  const [keep, setKeep] = useState("");
  const [busy, setBusy] = useState("");
  const [msg, setMsg] = useState<{ text: string; ok: boolean } | null>(null);

  const load = async () => {
    const r = await backups.overview();
    setO(r);
    setKeep(String(r.settings.keep));
  };
  useEffect(() => {
    load().catch((e) => setMsg({ text: e.message, ok: false }));
  }, []);
  if (!o) return msg ? <p className="err">{msg.text}</p> : <p className="muted">Loading…</p>;

  const run = async (what: string, fn: () => Promise<string | void>) => {
    setBusy(what);
    setMsg(null);
    try {
      const text = await fn();
      await load();
      if (text) setMsg({ text, ok: true });
    } catch (e) {
      setMsg({ text: (e as Error).message, ok: false });
    } finally {
      setBusy("");
    }
  };
  const saveSettings = (auto: boolean) =>
    run("settings", async () => {
      const k = Math.min(999, Math.max(1, Math.round(Number(keep)) || o.settings.keep));
      setO(await backups.saveSettings({ auto, keep: k }));
      return "Saved.";
    });


  return (
    <div className="backups-tab">
      <p className="muted">
        A backup is a copy of the whole log file: every log, QSO, location, piece of equipment and setting. Passwords
        aren't in it (they stay in Windows Credential Manager). Copy the backups folder to another drive or a USB stick
        now and then, so a dead disk doesn't take your log with it.
      </p>

      {o.last_restore && <p className={o.last_restore.ok ? "ok" : "err"}>{o.last_restore.message}</p>}

      {o.pending && (
        <fieldset className="restore-pending">
          <legend>Restore waiting</legend>
          <p><b>Restart QRZero to finish restoring.</b></p>
          <p>
            When QRZero next starts it replaces the log with <b>{o.pending.source}</b> ({o.pending.summary.logs}{" "}
            {o.pending.summary.logs === 1 ? "log" : "logs"}, {o.pending.summary.qsos.toLocaleString()} QSOs). The log as it
            is now is saved first, as a "before restore" backup. QSOs you log before restarting won't be in the restored log.
          </p>
          <button disabled={!!busy} onClick={() => run("cancel", async () => { await backups.cancelRestore(); return "Restore cancelled."; })}>Cancel restore</button>
        </fieldset>
      )}

      <fieldset>
        <legend>Automatic backups</legend>
        <label className="check">
          <input type="checkbox" checked={o.settings.auto} disabled={!!busy} onChange={(e) => saveSettings(e.target.checked)} /> Back up the log when QRZero starts (once a day)
        </label>
        <div className="check">
          <label htmlFor="backup-keep">Keep the newest</label>
          <input id="backup-keep" className="keep" value={keep} inputMode="numeric" onChange={(e) => setKeep(e.target.value.replace(/\D/g, ""))} />
          <label htmlFor="backup-keep">automatic backups</label>
          <button className="tiny" disabled={!!busy || keep === String(o.settings.keep)} onClick={() => saveSettings(o.settings.auto)}>Save</button>
        </div>
        <p className="small muted">Older automatic backups are deleted. Manual and "before restore" backups are kept until you delete them.</p>
      </fieldset>

      <div className="row">
        <button disabled={!!busy} onClick={() => run("create", async () => `Saved ${(await backups.create()).name}.`)}>
          {busy === "create" ? "Backing up…" : "Back up now"}
        </button>
        <label className={`button-like${busy ? " disabled" : ""}`}>
          Restore from a file…
          <input
            type="file"
            accept=".db,.sqlite,application/vnd.sqlite3,application/x-sqlite3"
            hidden
            disabled={!!busy}
            data-testid="restore-file"
            onChange={(e) => {
              const f = e.target.files?.[0];
              e.target.value = "";
              if (f) run("upload", async () => { await backups.restoreUpload(f); });
            }}
          />
        </label>
        {busy === "upload" && <span className="muted small">Checking the file…</span>}
      </div>

      {msg && <p className={msg.ok ? "ok" : "err"}>{msg.text}</p>}

      {o.backups.length === 0 ? (
        <p className="muted">No backups yet.</p>
      ) : (
        <table className="list backups">
          <thead>
            <tr><th>Made (local time)</th><th className="num">Size</th><th>Type</th><th>File</th><th /></tr>
          </thead>
          <tbody>
            {o.backups.map((b) => (
              <tr key={b.name}>
                <td className="nowrap">{localTime(b.created)}</td>
                <td className="num nowrap">{size(b.size)}</td>
                <td className="nowrap">{KIND_NAMES[b.kind]}</td>
                <td className="file">{b.name}</td>
                <td className="nowrap">
                  <button className="tiny" disabled={!!busy} onClick={() => run("download", async () => download(b.name, await backups.download(b.name)))}>Download</button>
                  <button
                    className="tiny"
                    disabled={!!busy}
                    onClick={() => {
                      if (confirm(`Restore the log from ${b.name}?\n\nThis replaces the whole log (all logs and QSOs) when QRZero next starts. The log as it is now is saved as a backup first.`))
                        run("restore", async () => { await backups.restore(b.name); });
                    }}
                  >
                    Restore
                  </button>
                  <button
                    className="tiny danger"
                    disabled={!!busy}
                    onClick={() => { if (confirm(`Delete the backup ${b.name}?`)) run("delete", () => backups.remove(b.name).then(() => undefined)); }}
                  >
                    Delete
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <div className="f folder">
        <span>Backups folder</span>
        <span className="folder-row">
          <input readOnly value={o.folder} onFocus={(e) => e.target.select()} aria-label="Backups folder" />
          <button
            onClick={() => navigator.clipboard.writeText(o.folder).then(() => setMsg({ text: "Folder path copied.", ok: true }), () => setMsg({ text: "Couldn't copy; select the path and press Ctrl+C.", ok: false }))}
          >
            Copy
          </button>
        </span>
      </div>
    </div>
  );
}
