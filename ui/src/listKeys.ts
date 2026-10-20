import { useEffect, useRef, useState } from "react";

/**
 * Arrow-key selection for a list of rows (spots, decodes). Up/Down/Home/End move, Enter does what a
 * click does and Ctrl+Enter what a double-click does. Spread `props` on the scrolling list element (give it
 * the class "kb-list") and mark each row with `rowProps(key)`.
 */
export function useListKeys<T>(items: T[], keyOf: (t: T) => string, act: (t: T, tune: boolean) => void) {
  const [sel, setSel] = useState<string | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const actRef = useRef(act);
  actRef.current = act;

  useEffect(() => {
    box.current?.querySelector("[data-kb-sel]")?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.target !== e.currentTarget || e.altKey || e.shiftKey || items.length === 0) return;
    const cur = sel === null ? -1 : items.findIndex((t) => keyOf(t) === sel);
    let next = -1;
    if (e.key === "ArrowDown") next = Math.min(items.length - 1, cur + 1);
    else if (e.key === "ArrowUp") next = Math.max(0, cur < 0 ? 0 : cur - 1);
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = items.length - 1;
    else if (e.key === "Enter" && cur >= 0) {
      e.preventDefault();
      actRef.current(items[cur], e.ctrlKey);
      return;
    } else return;
    e.preventDefault();
    setSel(keyOf(items[next]));
  };

  return {
    props: { ref: box, tabIndex: 0, onKeyDown },
    rowProps: (key: string) => (sel === key ? { "data-kb-sel": "" } : {}),
  };
}
