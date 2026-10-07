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

/** Small per-computer conveniences (last band, selected rig) in localStorage. */
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
}
