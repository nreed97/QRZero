// Where the main window (and each pop-out's maximized flag) was left, so the next start opens it the same way.
// One line per window in `window-state` in the data folder: `key x y width height maximized`.
// Sizes and positions are physical pixels of the restored (not maximized) window.

use std::collections::HashMap;
use std::path::PathBuf;

use tauri::{PhysicalPosition, PhysicalSize, WebviewWindow};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct State {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub max: bool,
}

fn file() -> PathBuf {
    qrzero_server::default_data_dir().join("window-state")
}

fn read() -> HashMap<String, State> {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read_to_string(file()) else { return out };
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if let [key, x, y, w, h, max] = f[..] {
            if let (Ok(x), Ok(y), Ok(w), Ok(h)) = (x.parse(), y.parse(), w.parse(), h.parse()) {
                out.insert(key.to_string(), State { x, y, w, h, max: max == "1" });
            }
        }
    }
    out
}

/// Remembers one window's state; the others stay as they were.
pub fn save(key: &str, s: State) {
    let mut all = read();
    all.insert(key.to_string(), s);
    let mut keys: Vec<_> = all.keys().cloned().collect();
    keys.sort();
    let text: String = keys
        .iter()
        .map(|k| {
            let s = all[k];
            format!("{k} {} {} {} {} {}\n", s.x, s.y, s.w, s.h, u8::from(s.max))
        })
        .collect();
    let _ = std::fs::write(file(), text);
}

pub fn load(key: &str) -> Option<State> {
    read().get(key).copied()
}

/// The window's restored geometry now, or None while it is maximized, minimized or full screen.
pub fn current(w: &WebviewWindow) -> Option<(i32, i32, u32, u32)> {
    if w.is_maximized().unwrap_or(false) || w.is_minimized().unwrap_or(false) || w.is_fullscreen().unwrap_or(false) {
        return None;
    }
    let p = w.outer_position().ok()?;
    let s = w.inner_size().ok()?;
    Some((p.x, p.y, s.width, s.height))
}

/// Whether the window's title bar would land on a connected screen, so it can always be grabbed.
fn on_screen(w: &WebviewWindow, s: &State) -> bool {
    let Ok(monitors) = w.available_monitors() else { return true };
    monitors.iter().any(|m| {
        let (mp, ms) = (m.position(), m.size());
        s.x + 100 > mp.x && s.x + 100 < mp.x + ms.width as i32 && s.y >= mp.y && s.y + 40 < mp.y + ms.height as i32
    })
}

/// Puts a (still hidden) window back where it was: size and position if they fit the screens, then maximized.
pub fn apply(w: &WebviewWindow, s: State) {
    if s.w >= 300 && s.h >= 200 {
        let _ = w.set_size(PhysicalSize::new(s.w, s.h));
        if on_screen(w, &s) {
            let _ = w.set_position(PhysicalPosition::new(s.x, s.y));
        }
    }
    if s.max {
        let _ = w.maximize();
    }
}
