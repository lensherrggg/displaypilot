use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

pub const MAIN_WINDOW: &str = "main";
pub const SETTINGS_WINDOW: &str = "settings";

pub fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN_WINDOW)
}

pub fn settings_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(SETTINGS_WINDOW)
}

pub fn show_main(app: &AppHandle) {
    let Some(window) = main_window(app) else {
        return;
    };
    position_near_tray(app, &window);
    let _ = window.show();
    let _ = window.set_focus();
}

pub fn hide_main(app: &AppHandle) {
    if let Some(window) = main_window(app) {
        let _ = window.hide();
    }
}

pub fn hide_settings(app: &AppHandle) {
    if let Some(window) = settings_window(app) {
        let _ = window.hide();
    }
}

pub fn toggle_main(app: &AppHandle) {
    let Some(window) = main_window(app) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
    } else {
        show_main(app);
    }
}

pub fn show_settings(app: &AppHandle) {
    if let Some(window) = settings_window(app) {
        hide_main(app);
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("settings-shown", ());
    }
}

fn position_near_tray(app: &AppHandle, window: &WebviewWindow) {
    let monitor = app
        .cursor_position()
        .ok()
        .and_then(|position| {
            app.monitor_from_point(position.x, position.y)
                .ok()
                .flatten()
        })
        .or_else(|| app.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else {
        return;
    };

    let work_area = monitor.work_area();
    let scale = monitor.scale_factor();
    let window_size = match window.outer_size() {
        Ok(size) => size,
        Err(_) => return,
    };
    let margin = (12.0 * scale).round() as i32;

    let max_x = work_area.position.x + work_area.size.width as i32 - window_size.width as i32;
    let x = (max_x - margin).max(work_area.position.x);

    #[cfg(target_os = "macos")]
    let y = work_area.position.y + margin;
    #[cfg(not(target_os = "macos"))]
    let y =
        (work_area.position.y + work_area.size.height as i32 - window_size.height as i32 - margin)
            .max(work_area.position.y);

    let _ = window.set_position(PhysicalPosition::new(x, y));
}

/// Opens a directory in the platform file manager. Only paths owned by the app
/// (config and log directories) are exposed through commands.
pub fn open_directory(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("无法打开目录 {}：{error}", path.display()))
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("无法打开目录 {}：{error}", path.display()))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| format!("无法打开目录 {}：{error}", path.display()))
    }
}
