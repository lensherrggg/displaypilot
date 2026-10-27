use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use crate::app_window;
use crate::models::{ConfigInfo, ShortcutStatus};
use crate::service::QuickAction;
use crate::state::AppState;

/// Replaces the registered global shortcuts with the bindings in `config`.
/// Empty accelerators are reported as "not configured" instead of failing.
pub fn apply(app: &AppHandle, config: &ConfigInfo) -> Vec<ShortcutStatus> {
    let global_shortcut = app.global_shortcut();
    if let Err(error) = global_shortcut.unregister_all() {
        log::warn!("Failed to clear previous global shortcuts: {error}");
    }

    let mut statuses = Vec::with_capacity(config.shortcuts.len());
    for binding in &config.shortcuts {
        if binding.accelerator.trim().is_empty() {
            statuses.push(ShortcutStatus {
                action: binding.action.clone(),
                label: binding.label.clone(),
                accelerator: String::new(),
                registered: false,
                error: None,
            });
            continue;
        }

        let action = binding.action.clone();
        let app_for_handler = app.clone();
        let app_for_worker = app.clone();
        let result = global_shortcut.on_shortcut(
            binding.accelerator.as_str(),
            move |_app, _shortcut, event| {
                if event.state != ShortcutState::Pressed {
                    return;
                }
                if action == "toggle_panel" {
                    app_window::toggle_main(&app_for_handler);
                    return;
                }
                let Some(quick) = QuickAction::parse(&action) else {
                    log::warn!("Unknown shortcut action: {action}");
                    return;
                };
                let worker = app_for_worker.state::<AppState>().worker.clone();
                let app = app_for_worker.clone();
                std::thread::spawn(move || match worker.quick_action(quick) {
                    Ok(snapshot) => {
                        let _ = app.emit("snapshot-updated", snapshot);
                    }
                    Err(error) => {
                        log::warn!("Shortcut action failed: {error}");
                        let _ = app.emit("operation-failed", error);
                    }
                });
            },
        );

        statuses.push(match result {
            Ok(()) => {
                log::info!(
                    "Registered shortcut {} for {}",
                    binding.accelerator,
                    binding.action
                );
                ShortcutStatus {
                    action: binding.action.clone(),
                    label: binding.label.clone(),
                    accelerator: binding.accelerator.clone(),
                    registered: true,
                    error: None,
                }
            }
            Err(error) => {
                log::warn!(
                    "Failed to register shortcut {} for {}: {error}",
                    binding.accelerator,
                    binding.action
                );
                ShortcutStatus {
                    action: binding.action.clone(),
                    label: binding.label.clone(),
                    accelerator: binding.accelerator.clone(),
                    registered: false,
                    error: Some(error.to_string()),
                }
            }
        });
    }
    statuses
}
