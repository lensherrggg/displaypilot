use tauri::{AppHandle, Manager, State};

use crate::app_window;
use crate::error::AppError;
use crate::models::{
    AppInfo, BootstrapPayload, ConfigInfo, DiagnosticReport, DisplayList, DisplaySnapshot,
    ShortcutStatus,
};
use crate::shortcuts;
use crate::state::{Ack, AppState};
use crate::worker::WorkerClient;

async fn run<T, F>(worker: WorkerClient, task: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce(WorkerClient) -> Result<T, AppError> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || task(worker))
        .await
        .map_err(|error| AppError::internal(format!("后台任务执行失败：{error}")))?
}

#[tauri::command]
pub async fn bootstrap(state: State<'_, AppState>) -> Result<BootstrapPayload, AppError> {
    let worker = state.worker.clone();
    run(worker, |worker| worker.bootstrap()).await
}

#[tauri::command]
pub async fn app_info(state: State<'_, AppState>) -> Result<AppInfo, AppError> {
    let worker = state.worker.clone();
    run(worker, |worker| worker.app_info()).await
}

#[tauri::command]
pub async fn list_displays(
    state: State<'_, AppState>,
    force: Option<bool>,
) -> Result<DisplayList, AppError> {
    let worker = state.worker.clone();
    run(worker, move |worker| {
        worker.list_displays(force.unwrap_or(false))
    })
    .await
}

#[tauri::command]
pub async fn refresh_displays(state: State<'_, AppState>) -> Result<DisplayList, AppError> {
    let worker = state.worker.clone();
    run(worker, |worker| worker.list_displays(true)).await
}

#[tauri::command]
pub async fn select_display(
    state: State<'_, AppState>,
    display_id: String,
) -> Result<Ack, AppError> {
    let worker = state.worker.clone();
    run(worker, move |worker| worker.select_display(&display_id)).await?;
    Ok(Ack { ok: true })
}

#[tauri::command]
pub async fn get_snapshot(
    state: State<'_, AppState>,
    display_id: String,
    force: Option<bool>,
) -> Result<DisplaySnapshot, AppError> {
    let worker = state.worker.clone();
    run(worker, move |worker| {
        worker.snapshot(&display_id, force.unwrap_or(false))
    })
    .await
}

#[tauri::command]
pub async fn switch_input(
    state: State<'_, AppState>,
    display_id: String,
    input_id: String,
) -> Result<DisplaySnapshot, AppError> {
    let worker = state.worker.clone();
    run(worker, move |worker| {
        worker.switch_input(&display_id, &input_id)
    })
    .await
}

#[tauri::command]
pub async fn set_brightness(
    state: State<'_, AppState>,
    display_id: String,
    percent: u8,
) -> Result<DisplaySnapshot, AppError> {
    let worker = state.worker.clone();
    run(worker, move |worker| {
        worker.set_brightness(&display_id, percent)
    })
    .await
}

#[tauri::command]
pub async fn set_volume(
    state: State<'_, AppState>,
    display_id: String,
    percent: u8,
) -> Result<DisplaySnapshot, AppError> {
    let worker = state.worker.clone();
    run(worker, move |worker| {
        worker.set_volume(&display_id, percent)
    })
    .await
}

#[tauri::command]
pub async fn toggle_mute(
    state: State<'_, AppState>,
    display_id: String,
) -> Result<DisplaySnapshot, AppError> {
    let worker = state.worker.clone();
    run(worker, move |worker| worker.toggle_mute(&display_id)).await
}

#[tauri::command]
pub async fn set_mute(
    state: State<'_, AppState>,
    display_id: String,
    muted: bool,
) -> Result<DisplaySnapshot, AppError> {
    let worker = state.worker.clone();
    run(worker, move |worker| worker.set_mute(&display_id, muted)).await
}

#[tauri::command]
pub async fn diagnostics(
    state: State<'_, AppState>,
    display_id: String,
) -> Result<DiagnosticReport, AppError> {
    let worker = state.worker.clone();
    run(worker, move |worker| worker.diagnostics(&display_id)).await
}

#[tauri::command]
pub async fn config_info(state: State<'_, AppState>) -> Result<ConfigInfo, AppError> {
    let worker = state.worker.clone();
    run(worker, |worker| worker.config_info()).await
}

#[tauri::command]
pub async fn reload_config(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ConfigInfo, AppError> {
    let worker = state.worker.clone();
    let config = run(worker, |worker| worker.reload_config()).await?;
    state.set_auto_hide(config.general.auto_hide);
    let status = shortcuts::apply(&app, &config);
    state.set_shortcut_status(status);
    Ok(config)
}

#[tauri::command]
pub async fn update_general(
    app: AppHandle,
    state: State<'_, AppState>,
    general: crate::config::GeneralConfig,
) -> Result<ConfigInfo, AppError> {
    crate::config::update_general(&state.paths.config_file, &general).map_err(AppError::config)?;
    let worker = state.worker.clone();
    let config = run(worker, |worker| worker.reload_config()).await?;
    state.set_auto_hide(config.general.auto_hide);
    let status = shortcuts::apply(&app, &config);
    state.set_shortcut_status(status);
    Ok(config)
}

#[tauri::command]
pub fn shortcut_status(state: State<'_, AppState>) -> Vec<ShortcutStatus> {
    state.shortcut_status()
}

#[tauri::command]
pub fn hide_panel(app: AppHandle) {
    app_window::hide_main(&app);
}

#[tauri::command]
pub fn hide_settings(app: AppHandle) {
    app_window::hide_settings(&app);
}

#[tauri::command]
pub fn open_settings(app: AppHandle) {
    app_window::show_settings(&app);
}

#[tauri::command]
pub fn open_config_dir(state: State<'_, AppState>) -> Result<(), AppError> {
    app_window::open_directory(&state.paths.config_dir).map_err(AppError::config)
}

#[tauri::command]
pub fn open_log_dir(state: State<'_, AppState>) -> Result<(), AppError> {
    app_window::open_directory(&state.paths.log_dir).map_err(AppError::config)
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
}

#[tauri::command]
pub fn main_window_visible(app: AppHandle) -> bool {
    app.get_webview_window(app_window::MAIN_WINDOW)
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}
