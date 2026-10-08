import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Popout from "./components/Popout";
import EditorWindow from "./components/EditorWindow";
import { PANES } from "./workspace";
import { loadLocalPrefs } from "./prefs";
import "./styles.css";

// A pane popped out of the main window opens this page with ?popout=<pane>; the QSO editor with ?popout=editor.
const popout = new URLSearchParams(window.location.search).get("popout");
const pane = PANES.find((p) => p.id === popout && p.popout)?.id;

// Window settings come from the database first, so a saved layout is back after a restart or update.
void loadLocalPrefs().finally(() =>
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>{popout === "editor" ? <EditorWindow /> : pane ? <Popout pane={pane} /> : <App />}</React.StrictMode>,
  ),
);
