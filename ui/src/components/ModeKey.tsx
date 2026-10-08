import type { CSSProperties } from "react";

/** The colour key for spots: CW, digital and phone. */
export default function ModeKey() {
  return (
    <span className="mode-key" title="Spot colours by mode">
      <span style={{ "--k": "var(--mode-cw)" } as CSSProperties}>CW</span>
      <span style={{ "--k": "var(--mode-digital)" } as CSSProperties}>Digital</span>
      <span style={{ "--k": "var(--mode-phone)" } as CSSProperties}>Phone</span>
    </span>
  );
}
