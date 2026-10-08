import type { PopContext } from "../bus";
import type { PaneActions } from "./SharedPanes";

/** Watch list pane (placeholder until it is built). */
export default function WatchPane({ ctx, act }: { ctx: PopContext; act: PaneActions }) {
  void ctx;
  void act;
  return <p className="muted" style={{ padding: 16 }}>Watch list is coming soon.</p>;
}
