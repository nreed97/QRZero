import type { PopContext } from "../bus";
import type { PaneActions } from "./SharedPanes";

/** Band map pane (placeholder until it is built). */
export default function BandMapPane({ ctx, act }: { ctx: PopContext; act: PaneActions }) {
  void ctx;
  void act;
  return <p className="muted" style={{ padding: 16 }}>Band map is coming soon.</p>;
}
