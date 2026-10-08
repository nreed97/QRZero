import type { PopContext } from "../bus";
import type { PaneActions } from "./SharedPanes";

/** Notes pane (placeholder until it is built). */
export default function NotesPane({ ctx, act }: { ctx: PopContext; act: PaneActions }) {
  void ctx;
  void act;
  return <p className="muted" style={{ padding: 16 }}>Notes is coming soon.</p>;
}
