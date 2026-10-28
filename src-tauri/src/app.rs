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
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "settings" {
                app_window::show_settings(app);
            }
        })
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
            #[cfg(target_os = "macos")]
            install_app_menu(app)?;
            if let Some(window) = app_window::main_window(app.handle()) {
                // The panel fills its transparent window on macOS; a native window
                // shadow would be drawn as a square rectangle around the rounded card.
                #[cfg(target_os = "macos")]
                let _ = window.set_shadow(false);
                // The panel belongs to the active space, like a menu bar popover.
                let _ = window.set_visible_on_all_workspaces(true);
            }
            build_tray(app.handle())?;
            install_shortcuts(app.handle());
            app_window::show_main(app.handle());
            log::info!("DisplayPilot ready");
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::Focused(false) if window.label() == app_window::MAIN_WINDOW => {
                let state = window.app_handle().state::<AppState>();
                if state.auto_hide() {
                    let _ = window.hide();
                    state.note_auto_hide();
                }
            }
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("failed to build DisplayPilot")
        .run(|app, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                // Clicking the app icon while it is already running reopens the panel.
                app_window::show_main(app);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}

fn build_tray(app: &tauri::AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开面板", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "设置…", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "刷新显示器", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出 DisplayPilot", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &settings, &refresh, &separator, &quit])?;

    let mut builder = TrayIconBuilder::with_id(app_window::TRAY_ID)
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
                rect,
                ..
            } = event
            {
                let app = tray.app_handle();
                let scale = app
                    .primary_monitor()
                    .ok()
                    .flatten()
                    .map(|monitor| monitor.scale_factor())
                    .unwrap_or(1.0);
                // Skip empty rectangles (some Linux backends) and fall back to the cursor.
                let anchor = app_window::TrayAnchor::from_rect(rect, scale);
                app_window::toggle_main_from_tray(app, anchor);
            }
        });

    #[cfg(target_os = "macos")]
    {
        // A monochrome template image lets the system tint the icon for the
        // light/dark menu bar. tray-icon scales it to 18pt tall.
        match tauri::image::Image::from_bytes(include_bytes!("../icons/tray-template.png")) {
            Ok(icon) => builder = builder.icon(icon),
            Err(error) => log::warn!("Could not load the menu bar template icon: {error}"),
        }
    }

    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }

    builder.build(app)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn install_app_menu(app: &tauri::App) -> tauri::Result<()> {
    use tauri::menu::{AboutMetadataBuilder, Menu, MenuItemBuilder, SubmenuBuilder};

    let about = AboutMetadataBuilder::new()
        .name(Some("DisplayPilot"))
        .version(Some(env!("CARGO_PKG_VERSION")))
        .copyright(Some("MIT License"))
        .credits(Some("通过 DDC/CI 控制显示器输入源、亮度与音频。"))
        .build();

    let settings = MenuItemBuilder::with_id("settings", "设置…")
        .accelerator("CmdOrCtrl+,")
        .build(app)?;

    let app_menu = SubmenuBuilder::new(app, "DisplayPilot")
        .about(Some(about))
        .separator()
        .item(&settings)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "编辑")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let window_menu = SubmenuBuilder::new(app, "窗口")
        .minimize()
        .separator()
        .close_window()
        .build()?;

    let menu = Menu::with_items(app, &[&app_menu, &edit_menu, &window_menu])?;
    app.set_menu(menu)?;
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
                // Match --radius-panel so the native rounded corners line up with the card.
                apply_vibrancy(&window, NSVisualEffectMaterial::HudWindow, None, Some(18.0))
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
