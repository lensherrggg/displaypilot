use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WindowEvent};

use crate::app_window;
use crate::commands;
use crate::config::AppPaths;
use crate::shortcuts;
use crate::state::AppState;
use crate::worker::WorkerClient;

pub fn run() {
    hide_console();

    let paths = AppPaths::discover();
    let worker = WorkerClient::spawn(paths.clone());
    let auto_hide = true;

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(AppState::new(worker, paths, auto_hide))
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::app_info,
            commands::list_displays,
            commands::refresh_displays,
            commands::select_display,
            commands::get_snapshot,
            commands::switch_input,
            commands::set_brightness,
            commands::set_volume,
            commands::toggle_mute,
            commands::set_mute,
            commands::diagnostics,
            commands::config_info,
            commands::reload_config,
            commands::update_general,
            commands::shortcut_status,
            commands::hide_panel,
            commands::hide_settings,
            commands::open_settings,
            commands::open_config_dir,
            commands::open_log_dir,
            commands::quit_app,
            commands::main_window_visible,
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            #[cfg(target_os = "macos")]
            app.set_dock_visibility(false);

            apply_window_effects(app);
            build_tray(app.handle())?;
            install_shortcuts(app.handle());
            app_window::show_main(app.handle());
            log::info!("DisplayPilot ready");
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::Focused(false) if window.label() == app_window::MAIN_WINDOW => {
                if window.app_handle().state::<AppState>().auto_hide() {
                    let _ = window.hide();
                }
            }
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("failed to run DisplayPilot");
}

fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开面板", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "刷新显示器", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出 DisplayPilot", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &settings, &refresh, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id("displaypilot-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip(format!("DisplayPilot {}", env!("CARGO_PKG_VERSION")))
        .icon_as_template(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => app_window::show_main(app),
            "settings" => app_window::show_settings(app),
            "refresh" => {
                app_window::show_main(app);
                let _ = app.emit("refresh-requested", ());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let focused = app_window::main_window(app)
                    .and_then(|window| window.is_focused().ok())
                    .unwrap_or(false);
                if !focused {
                    app_window::toggle_main(app);
                }
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

fn install_shortcuts(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    match state.worker.config_info() {
        Ok(config) => {
            state.set_auto_hide(config.general.auto_hide);
            let status = shortcuts::apply(app, &config);
            state.set_shortcut_status(status);
        }
        Err(error) => {
            log::warn!("Could not load shortcut configuration: {error}");
        }
    }
}

fn apply_window_effects(app: &tauri::App) {
    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial};
        if let Some(window) = app.get_webview_window(app_window::MAIN_WINDOW) {
            if let Err(error) =
                apply_vibrancy(&window, NSVisualEffectMaterial::HudWindow, None, Some(16.0))
            {
                log::warn!("Could not apply vibrancy: {error}");
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
    }
}

/// A console is required for the CLI fallback, so the GUI detaches from it on Windows
/// instead of using the `windows` subsystem.
fn hide_console() {
    #[cfg(target_os = "windows")]
    unsafe {
        #[link(name = "kernel32")]
        extern "system" {
            fn FreeConsole() -> i32;
        }
        let _ = FreeConsole();
    }
}
