//! Ondes — application de diagnostic du réseau domestique Freebox.

pub mod demo;
pub mod engine;
pub mod freebox;
pub mod normalize;
pub mod store;
pub mod winnet;

pub use engine::Engine;

use serde_json::{json, Value};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

/// Point d'entrée unique de l'interface : `invoke('api', { cmd, args })`.
#[tauri::command]
async fn api(app: AppHandle, state: tauri::State<'_, Engine>, cmd: String, args: Option<Value>) -> Result<Value, String> {
    let args = args.unwrap_or(Value::Null);
    match cmd.as_str() {
        "autostart_get" => app.autolaunch().is_enabled().map(|b| json!(b)).map_err(|e| e.to_string()),
        "autostart_set" => {
            let on = args.get("on").and_then(|v| v.as_bool()).unwrap_or(false);
            let m = app.autolaunch();
            if on { m.enable() } else { m.disable() }.map(|_| json!(on)).map_err(|e| e.to_string())
        }
        _ => state.dispatch(&cmd, args).await.map_err(|e| e.to_string()),
    }
}

fn show(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--hidden"])))
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let engine = Engine::new(dir);
            engine.start();
            app.manage(engine);

            let open = MenuItem::with_id(app, "open", "Ouvrir Ondes", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quitter (arrête la collecte)", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .tooltip("Ondes — diagnostic du réseau")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "open" => show(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                        show(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            if !std::env::args().any(|a| a == "--hidden") {
                show(app.handle());
            }
            Ok(())
        })
        .on_window_event(|w, e| {
            if let WindowEvent::CloseRequested { api, .. } = e {
                if w.state::<Engine>().cfg().close_to_tray {
                    api.prevent_close();
                    let _ = w.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![api])
        .run(tauri::generate_context!())
        .expect("Ondes n'a pas pu démarrer");
}
