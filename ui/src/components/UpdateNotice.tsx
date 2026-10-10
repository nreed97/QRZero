import { useEffect, useState } from "react";
import { api } from "../api";
import { localGet, localSet } from "../prefs";
import type { UpdateInfo } from "../types";

/** Settings fires this after the check is turned on or off, so the notice follows at once. */
export const UPDATES_CHANGED = "qrzero-updates-changed";

const DISMISSED = "update_dismissed";

/**
 * A small link in the top bar when a newer release is out. The server checks GitHub once a day;
 * this only asks the server what it found, a little after start (when its first check is done) and then hourly.
 */
export default function UpdateNotice() {
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [dismissed, setDismissed] = useState(() => localGet<{ version: string }>(DISMISSED, { version: "" }).version);

  useEffect(() => {
    let live = true;
    const load = () => api.updates().then((i) => live && setInfo(i)).catch(() => {});
    const first = window.setTimeout(load, 45_000);
    const hourly = window.setInterval(load, 3_600_000);
    window.addEventListener(UPDATES_CHANGED, load);
    return () => {
      live = false;
      window.clearTimeout(first);
      window.clearInterval(hourly);
      window.removeEventListener(UPDATES_CHANGED, load);
    };
  }, []);

  const u = info?.update;
  if (!u || u.version === dismissed) return null;
  return (
    <span className="update-notice">
      <a href={u.url} target="_blank" rel="noopener noreferrer" title={`You have ${info?.current}. Open the download page in your web browser.`}>Version {u.version} is available</a>
      <button className="link-x" title="Hide this until the next release" aria-label="Hide the update notice" onClick={() => { localSet(DISMISSED, { version: u.version }); setDismissed(u.version); }}>×</button>
    </span>
  );
}
