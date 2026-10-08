import { useEffect, useRef, useState } from "react";

/** Open state for a drop-down that closes on a click outside it or Escape. Put the ref on the element holding button and menu. */
export function usePopup<T extends HTMLElement>() {
  const [open, setOpen] = useState(false);
  const box = useRef<T>(null);
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", away);
    document.addEventListener("keydown", esc);
    return () => {
      document.removeEventListener("mousedown", away);
      document.removeEventListener("keydown", esc);
    };
  }, [open]);
  return { open, setOpen, box };
}
