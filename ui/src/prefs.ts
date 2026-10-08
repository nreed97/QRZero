import { useCallback, useEffect, useState } from "react";
import { api } from "./api";

/**
 * A preference stored in the log database (so it survives reinstalls).
 * Returns [value, save, loaded].
 */
export function usePref<T>(key: string, fallback: T): [T, (v: T) => void, boolean] {
  const [value, setValue] = useState<T>(fallback);
  const [loaded, setLoaded] = useState(false);
  useEffect(() => {
    let live = true;
    api
      .getPref<T>(key)
      .then((v) => live && v !== null && setValue(v))
      .catch(() => {})
      .finally(() => live && setLoaded(true));
    return () => {
      live = false;
    };
  }, [key]);
  const save = useCallback(
    (v: T) => {
      setValue(v);
      api.setPref(key, v).catch(() => {});
    },
    [key],
  );
  return [value, save, loaded];
}

/**
 * Window and pane settings (layouts, columns, filters, last band...). They live in localStorage
 * for quick reads, and are copied to the database too: the desktop app's address changes every
 * start, and with it the browser storage, so the database copy is what survives.
 */
export function localGet<T>(key: string, fallback: T): T {
  try {
    const v = localStorage.getItem(key);
    return v ? { ...fallback, ...JSON.parse(v) } : fallback;
  } catch {
    return fallback;
  }
}

export function localSet(key: string, v: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(v));
  } catch { /* storage unavailable */ }
  saveSoon();
}

const STORE = "ui.local";
const PREFIX = "qrzero.";
let timer: ReturnType<typeof setTimeout> | null = null;

/** Every window setting in this browser's storage. Windows of one app share it, so this is the latest of all of them. */
function allLocal(): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  try {
    for (let i = 0; i < localStorage.length; i++) {
      const k = localStorage.key(i);
      if (!k?.startsWith(PREFIX)) continue;
      try {
        out[k] = JSON.parse(localStorage.getItem(k) ?? "null");
      } catch { /* not ours */ }
    }
  } catch { /* storage unavailable */ }
  return out;
}

function saveSoon() {
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => {
    timer = null;
    api.setPref(STORE, allLocal()).catch(() => {});
  }, 400);
}

/**
 * Before the first render: fills this browser's storage from the database copy. Anything already
 * in this browser's storage is at least as new (it is written first), so it wins and is saved up.
 */
export async function loadLocalPrefs(): Promise<void> {
  let saved: Record<string, unknown> | null = null;
  try {
    saved = await Promise.race([
      api.getPref<Record<string, unknown>>(STORE),
      new Promise<null>((ok) => setTimeout(() => ok(null), 3000)),
    ]);
  } catch {
    return;
  }
  const local = allLocal();
  try {
    for (const [k, v] of Object.entries(saved ?? {})) if (!(k in local)) localStorage.setItem(k, JSON.stringify(v));
  } catch {
    return;
  }
  const differs = Object.entries(local).some(([k, v]) => !saved || JSON.stringify(saved[k]) !== JSON.stringify(v));
  if (differs) saveSoon();
}

// Don't lose a change made just before the window closes.
window.addEventListener("pagehide", () => {
  if (!timer) return;
  clearTimeout(timer);
  timer = null;
  api.setPref(STORE, allLocal(), true).catch(() => {});
});
