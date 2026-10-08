import type { PopContext } from "../bus";
import type { PaneActions } from "./SharedPanes";

/** Propagation pane (placeholder until it is built). */
export default function PropagationPane({ ctx, act }: { ctx: PopContext; act: PaneActions }) {
  void ctx;
  void act;
  return <p className="muted" style={{ padding: 16 }}>Propagation is coming soon.</p>;
}
