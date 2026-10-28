use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

use crate::state::AppState;

pub const MAIN_WINDOW: &str = "main";
pub const SETTINGS_WINDOW: &str = "settings";
pub const TRAY_ID: &str = "displaypilot-tray";

/// Gap between the panel and the menu bar / screen edge, in logical points.
const PANEL_MARGIN: f64 = 8.0;
/// Design height of the panel from `tauri.conf.json`, in logical points.
const PANEL_HEIGHT: f64 = 620.0;
/// Clicking the status item blurs the panel before the click event arrives. Ignore a
/// tray click that happens right after an auto-hide so the panel does not reopen.
const TRAY_CLICK_GUARD: Duration = Duration::from_millis(220);

/// Screen-space rectangle of the status item that was clicked. Values are physical pixels.
#[derive(Debug, Clone, Copy)]
pub struct TrayAnchor {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl TrayAnchor {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Converts the tray rectangle reported by Tauri into a usable anchor. Tauri's
    /// tray rects are physical pixels on macOS, so the scale only normalizes units.
    pub fn from_rect(rect: tauri::Rect, scale: f64) -> Option<Self> {
        let position = rect.position.to_physical::<f64>(scale);
        let size = rect.size.to_physical::<u32>(scale);
        (size.width > 0 && size.height > 0).then(|| {
            Self::new(
                position.x,
                position.y,
                f64::from(size.width),
                f64::from(size.height),
            )
        })
    }

    fn center_x(&self) -> f64 {
        self.x + self.width / 2.0
    }

    fn center_y(&self) -> f64 {
        self.y + self.height / 2.0
    }
}

pub fn main_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(MAIN_WINDOW)
}

pub fn settings_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(SETTINGS_WINDOW)
}

pub fn show_main(app: &AppHandle) {
    show_main_at(app, tray_anchor(app));
}

/// Current screen rectangle of the status item / tray icon, if the platform
/// reports one. Used to center the panel under the icon even before the first click.
pub fn tray_anchor(app: &AppHandle) -> Option<TrayAnchor> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        log::debug!("tray icon {TRAY_ID} is not registered");
        return None;
    };
    let rect = tray.rect().ok().flatten()?;
    log::debug!("tray rect={rect:?}");
    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| monitor.scale_factor())
        .unwrap_or(1.0);
    TrayAnchor::from_rect(rect, scale)
}

/// Shows the panel anchored to the status item (macOS) or tray icon (Windows).
pub fn show_main_at(app: &AppHandle, anchor: Option<TrayAnchor>) {
    let Some(window) = main_window(app) else {
        return;
    };
    position_panel(app, &window, anchor);
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

/// Toggles the panel from a global shortcut. Shortcuts should always feel direct,
/// so they bypass the tray-click guard.
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

/// Toggles the panel from a status item / tray icon click. When the panel is open,
/// clicking the icon blurs it first; that auto-hide counts as "close", so the click
/// must not immediately reopen it.
pub fn toggle_main_from_tray(app: &AppHandle, anchor: Option<TrayAnchor>) {
    let Some(window) = main_window(app) else {
        return;
    };
    if window.is_visible().unwrap_or(false) {
        let _ = window.hide();
        return;
    }
    if app.state::<AppState>().auto_hidden_within(TRAY_CLICK_GUARD) {
        return;
    }
    show_main_at(app, anchor);
}

pub fn show_settings(app: &AppHandle) {
    if let Some(window) = settings_window(app) {
        hide_main(app);
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("settings-shown", ());
    }
}

/// Places the panel on the display that owns the status item, just below the menu bar.
/// On Windows the same calculation anchors it to the bottom-right of the work area.
fn position_panel(app: &AppHandle, window: &WebviewWindow, anchor: Option<TrayAnchor>) {
    let monitor = anchor
        .and_then(|anchor| {
            app.monitor_from_point(anchor.center_x(), anchor.center_y())
                .ok()
                .flatten()
        })
        .or_else(|| {
            app.cursor_position().ok().and_then(|position| {
                app.monitor_from_point(position.x, position.y)
                    .ok()
                    .flatten()
            })
        })
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else {
        return;
    };

    let work_area = monitor.work_area();
    let scale = monitor.scale_factor();
    let margin = (PANEL_MARGIN * scale).round() as i32;

    let Ok(mut size) = window.outer_size() else {
        return;
    };

    // Keep the panel fully visible on short displays (e.g. a 720p external monitor).
    let max_height = work_area.size.height as i32 - margin * 2;
    if max_height >= 240 {
        let desired = (PANEL_HEIGHT * scale).round() as i32;
        let target = desired.min(max_height).max(240) as u32;
        if (size.height as i32 - target as i32).abs() > 1
            && window
                .set_size(PhysicalSize::new(size.width, target))
                .is_ok()
        {
            size = window.outer_size().unwrap_or(size);
        }
    }

    let left_limit = work_area.position.x + margin;
    let right_limit =
        (work_area.position.x + work_area.size.width as i32 - size.width as i32 - margin)
            .max(left_limit);

    let x = match anchor {
        Some(anchor) => {
            let centered = (anchor.center_x() - size.width as f64 / 2.0).round() as i32;
            centered.clamp(left_limit, right_limit)
        }
        None => right_limit,
    };

    #[cfg(target_os = "macos")]
    let y = work_area.position.y;
    #[cfg(not(target_os = "macos"))]
    let y = (work_area.position.y + work_area.size.height as i32 - size.height as i32 - margin)
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
