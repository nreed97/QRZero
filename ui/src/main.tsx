import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Popout from "./components/Popout";
import { PANES, type PaneId } from "./workspace";
import "./styles.css";

// A pane popped out of the main window opens this page with ?popout=<pane>.
const popout = new URLSearchParams(window.location.search).get("popout") as PaneId | null;
const pane = PANES.find((p) => p.id === popout && p.popout)?.id;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>{pane ? <Popout pane={pane} /> : <App />}</React.StrictMode>,
);
