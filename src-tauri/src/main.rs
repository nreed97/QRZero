// The desktop app: starts the QRZero server on 127.0.0.1 and opens a window on it.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

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
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(running.url().parse()?))
                .title("QRZero")
                .inner_size(1440.0, 900.0)
                .min_inner_size(960.0, 600.0)
                .build()?;
            app.manage(running);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("QRZero failed to start");
}
