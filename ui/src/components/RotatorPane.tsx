import type { PopContext } from "../bus";
import type { PaneActions } from "./SharedPanes";

/** Rotator pane (placeholder until it is built). */
export default function RotatorPane({ ctx, act }: { ctx: PopContext; act: PaneActions }) {
  void ctx;
  void act;
  return <p className="muted" style={{ padding: 16 }}>Rotator is coming soon.</p>;
}
