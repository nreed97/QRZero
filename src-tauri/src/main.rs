// The desktop app: starts the QRZero server on 127.0.0.1 and opens a window on it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicU32, Ordering};

use tauri::webview::NewWindowResponse;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

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
        let label = format!("pane-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        let mut builder = WebviewWindowBuilder::new(&app, label, WebviewUrl::External("about:blank".parse().unwrap()))
            .window_features(features)
            .title("QRZero")
            .min_inner_size(320.0, 200.0)
            .disable_drag_drop_handler()
            .on_document_title_changed(|w, title| {
                let _ = w.set_title(&title);
            })
            .on_new_window(popout_handler(app.clone(), home.clone()));
        builder = builder.focused(true);
        match builder.build() {
            Ok(window) => NewWindowResponse::Create { window },
            Err(_) => NewWindowResponse::Deny,
        }
    }
}

fn main() {
    tauri::Builder::default()
        // A second launch focuses the existing window instead of opening another.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .setup(|app| {
            let config = qrzero_server::Config::local(qrzero_server::default_data_dir());
            let running = tauri::async_runtime::block_on(qrzero_server::start(config))?;
            let url: tauri::Url = running.url().parse()?;
            let home = url.origin().ascii_serialization();
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url))
                .title("QRZero")
                .on_new_window(popout_handler(app.handle().clone(), home))
                // Tauri's file-drop handler swallows the page's own drag and drop on
                // Windows, which stopped pane tabs from being dragged.
                .disable_drag_drop_handler()
                .inner_size(1440.0, 900.0)
                .min_inner_size(960.0, 600.0)
                .build()?;
            app.manage(running);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("QRZero failed to start");
}
