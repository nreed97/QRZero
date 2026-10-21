// The desktop app: starts the QRZero server on 127.0.0.1 and opens a window on it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod window_state;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};

use tauri::webview::{NewWindowResponse, PageLoadEvent, PageLoadPayload};
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// Whether windows are frameless (QRZero draws its own title bar). Off when the user chose the system frame.
/// On macOS the change waits for the next start, since a window's title can't be hidden after it opens.
static FRAMELESS: AtomicBool = AtomicBool::new(true);
/// The platform, so the page knows which window buttons to draw (macOS keeps its own).
const OS: &str = std::env::consts::OS;
/// Windows currently maximized, so the page hears only when that changes.
static MAXIMIZED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
type Geometry = (i32, i32, u32, u32);
/// Where each window last stood while neither maximized nor minimized (x, y, width, height).
static RESTORED: OnceLock<Mutex<HashMap<String, Geometry>>> = OnceLock::new();

/// Present when the user turned frameless windows off.
fn system_frame_marker() -> PathBuf {
    qrzero_server::default_data_dir().join("system-frame")
}

fn frameless() -> bool {
    FRAMELESS.load(Ordering::Relaxed)
}

/// The page's view of its frame, as a JS object: now, and as chosen for the next start.
fn frame_js() -> String {
    format!("{{ supported: true, on: {}, saved: {}, os: '{OS}' }}", frameless(), !system_frame_marker().exists())
}

/// What every page learns at start: that it runs in the desktop app, and how its window is framed.
fn init_script() -> String {
    format!("window.__qrzeroDesktop = true; window.__qrzeroFrame = {};", frame_js())
}

/// Tells a page its frame and maximized state (after a reload, or a change).
fn tell_frame(w: &WebviewWindow) {
    let max = w.is_maximized().unwrap_or(false);
    let frame = frame_js();
    let _ = w.eval(format!("window.__qrzeroFrame = {frame}; window.dispatchEvent(new CustomEvent('qrzero-frame', {{ detail: {{ ...window.__qrzeroFrame, max: {max} }} }}))"));
}

/// Opens a window frameless or with the system frame. macOS keeps its traffic lights over QRZero's bar.
#[cfg(target_os = "macos")]
fn with_frame<'a, R: Runtime, M: Manager<R>>(b: WebviewWindowBuilder<'a, R, M>) -> WebviewWindowBuilder<'a, R, M> {
    if frameless() {
        b.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true)
    } else {
        b
    }
}

#[cfg(not(target_os = "macos"))]
fn with_frame<'a, R: Runtime, M: Manager<R>>(b: WebviewWindowBuilder<'a, R, M>) -> WebviewWindowBuilder<'a, R, M> {
    b.decorations(!frameless())
}

fn page_loaded(w: WebviewWindow, p: PageLoadPayload<'_>) {
    if matches!(p.event(), PageLoadEvent::Finished) {
        tell_frame(&w);
    }
}

/// The page asks the window for things by setting its title to `qrzero:<command>[:<arg>]...`
/// (no IPC; the same UI runs in a browser). Any other title becomes the window's title.
fn title_changed(w: WebviewWindow, title: String) {
    let Some(cmd) = title.strip_prefix("qrzero:") else {
        let _ = w.set_title(&title);
        return;
    };
    let mut parts = cmd.split(':');
    let arg = |p: Option<&str>| p == Some("1");
    match parts.next() {
        // window.close() is not reliable in these windows.
        Some("close") => {
            let _ = w.destroy();
        }
        Some("top") => {
            let _ = w.set_always_on_top(arg(parts.next()));
        }
        Some("drag") => {
            let _ = w.start_dragging();
        }
        Some("min") => {
            let _ = w.minimize();
        }
        Some("max") => {
            let _ = if w.is_maximized().unwrap_or(false) { w.unmaximize() } else { w.maximize() };
        }
        Some("frame") => {
            let on = arg(parts.next());
            let marker = system_frame_marker();
            let _ = if on { std::fs::remove_file(&marker) } else { std::fs::write(&marker, b"") };
            // macOS applies it at the next start; elsewhere every window changes now.
            if !cfg!(target_os = "macos") {
                FRAMELESS.store(on, Ordering::Relaxed);
            }
            for (_, win) in w.app_handle().webview_windows() {
                if !cfg!(target_os = "macos") {
                    let _ = win.set_decorations(!on);
                }
                tell_frame(&win);
            }
        }
        _ => {}
    }
}

/// Panes popped out of the main window (window.open) become ordinary app windows.
fn popout_handler(app: AppHandle, home: String) -> impl Fn(tauri::Url, tauri::webview::NewWindowFeatures) -> NewWindowResponse<tauri::Wry> + Send + 'static {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    move |url, features| {
        // Only QRZero's own pages get a window; web links open in the system browser.
        if !url.as_str().starts_with(&home) {
            if matches!(url.scheme(), "http" | "https") {
                let _ = tauri_plugin_opener::open_url(url.as_str(), None::<&str>);
            }
            return NewWindowResponse::Deny;
        }
        // The label carries the pane id, so the main window can be told which one closed.
        let pane: String = url
            .query_pairs()
            .find(|(k, _)| k == "popout")
            .map(|(_, v)| v.chars().filter(char::is_ascii_alphanumeric).collect())
            .unwrap_or_default();
        let label = format!("pane-{}-{pane}", NEXT.fetch_add(1, Ordering::Relaxed));
        let mut builder = with_frame(WebviewWindowBuilder::new(&app, label, WebviewUrl::External("about:blank".parse().unwrap())))
            .window_features(features)
            .title("QRZero")
            .min_inner_size(320.0, 200.0)
            .disable_drag_drop_handler()
            .on_document_title_changed(title_changed)
            .on_page_load(page_loaded)
            .initialization_script(init_script())
            .on_new_window(popout_handler(app.clone(), home.clone()));
        builder = builder.focused(true);
        match builder.build() {
            Ok(window) => {
                // The page remembers a pop-out's size and place; whether it was maximized is kept here.
                if window_state::load(&format!("pane-{pane}")).is_some_and(|s| s.max) {
                    let _ = window.maximize();
                }
                NewWindowResponse::Create { window }
            }
            Err(_) => NewWindowResponse::Deny,
        }
    }
}

/// Saves a window's last restored geometry and whether it is maximized now.
fn save_window(window: &tauri::Window) {
    let label = window.label();
    let key = match label.strip_prefix("pane-").and_then(|l| l.split_once('-')) {
        Some((_, pane)) => format!("pane-{pane}"),
        None => label.to_string(),
    };
    let max = window.is_maximized().unwrap_or(false) || MAXIMIZED.get_or_init(Default::default).lock().unwrap().contains(label);
    let g = RESTORED.get_or_init(Default::default).lock().unwrap().get(label).copied();
    let g = g.or_else(|| window.app_handle().get_webview_window(label).and_then(|w| window_state::current(&w)));
    let prev = window_state::load(&key);
    let (x, y, w, h) = match (g, prev) {
        (Some(g), _) => g,
        (None, Some(p)) => (p.x, p.y, p.w, p.h),
        (None, None) => (0, 0, 0, 0),
    };
    window_state::save(&key, window_state::State { x, y, w, h, max });
}

fn main() {
    FRAMELESS.store(!system_frame_marker().exists(), Ordering::Relaxed);
    tauri::Builder::default()
        // A second launch focuses the existing window instead of opening another.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        // Closing the main window closes every popped-out window and quits the app.
        .on_window_event(|window, event| {
            // A frameless window's own title bar shows maximize or restore, so it hears when that changes.
            if let tauri::WindowEvent::Resized(_) = event {
                let max = window.is_maximized().unwrap_or(false);
                let changed = {
                    let mut set = MAXIMIZED.get_or_init(Default::default).lock().unwrap();
                    if max { set.insert(window.label().to_string()) } else { set.remove(window.label()) }
                };
                if changed {
                    if let Some(w) = window.app_handle().get_webview_window(window.label()) {
                        tell_frame(&w);
                    }
                }
            }
            // Remember where the window stands, to save when it closes.
            if matches!(event, tauri::WindowEvent::Resized(_) | tauri::WindowEvent::Moved(_)) {
                if let Some(w) = window.app_handle().get_webview_window(window.label()) {
                    if let Some(g) = window_state::current(&w) {
                        RESTORED.get_or_init(Default::default).lock().unwrap().insert(window.label().to_string(), g);
                    }
                }
            }
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) && window.label() == "main" {
                save_window(window);
            }
            // A pop-out closed with its own close button goes back into the main window.
            if let (Some(pane), tauri::WindowEvent::Destroyed) = (window.label().strip_prefix("pane-").and_then(|l| l.split_once('-')).map(|(_, p)| p), event) {
                if let Some(main) = window.app_handle().get_webview_window("main") {
                    let _ = main.eval(format!("window.dispatchEvent(new CustomEvent('qrzero-popout-closed',{{detail:'{pane}'}}))"));
                }
            }
            if matches!(event, tauri::WindowEvent::Destroyed) {
                if window.label().starts_with("pane-") {
                    save_window(window);
                }
                MAXIMIZED.get_or_init(Default::default).lock().unwrap().remove(window.label());
            }
            if window.label() == "main" && matches!(event, tauri::WindowEvent::Destroyed) {
                let app = window.app_handle();
                for (_, w) in app.webview_windows() {
                    let _ = w.destroy();
                }
                app.exit(0);
            }
        })
        .setup(|app| {
            let config = qrzero_server::Config::local(qrzero_server::default_data_dir());
            let running = tauri::async_runtime::block_on(qrzero_server::start(config))?;
            let mut url: tauri::Url = running.url().parse()?;
            url.query_pairs_mut()
                .append_pair("desktop", if frameless() { "frameless" } else { "framed" })
                .append_pair("os", OS);
            let home = url.origin().ascii_serialization();
            let main = with_frame(WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url)))
                .title("QRZero")
                .on_new_window(popout_handler(app.handle().clone(), home))
                .on_document_title_changed(title_changed)
                .on_page_load(page_loaded)
                .initialization_script(init_script())
                // Tauri's file-drop handler swallows the page's own drag and drop on
                // Windows, which stopped pane tabs from being dragged.
                .disable_drag_drop_handler()
                .inner_size(1440.0, 900.0)
                .min_inner_size(960.0, 600.0)
                .visible(false)
                .build()?;
            // Open where it was closed: the same size and place, or maximized (restore then goes back to that size).
            if let Some(s) = window_state::load("main") {
                window_state::apply(&main, s);
            }
            let _ = main.show();
            app.manage(running);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("QRZero failed to start");
}
