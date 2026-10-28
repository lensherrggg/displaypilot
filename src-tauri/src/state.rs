use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::AppPaths;
use crate::models::ShortcutStatus;
use crate::worker::WorkerClient;

/// State shared with Tauri commands. It deliberately holds no DDC handles: those live
/// exclusively on the worker thread.
pub struct AppState {
    pub worker: WorkerClient,
    pub paths: AppPaths,
    pub auto_hide: Arc<AtomicBool>,
    pub shortcut_status: Arc<Mutex<Vec<ShortcutStatus>>>,
    /// When the panel hid itself because it lost focus. A click on the status item
    /// first blurs the panel, so the tray handler uses this to avoid re-opening it.
    last_auto_hide: Arc<Mutex<Option<Instant>>>,
}

impl AppState {
    pub fn new(worker: WorkerClient, paths: AppPaths, auto_hide: bool) -> Self {
        Self {
            worker,
            paths,
            auto_hide: Arc::new(AtomicBool::new(auto_hide)),
            shortcut_status: Arc::new(Mutex::new(Vec::new())),
            last_auto_hide: Arc::new(Mutex::new(None)),
        }
    }

    pub fn note_auto_hide(&self) {
        if let Ok(mut last) = self.last_auto_hide.lock() {
            *last = Some(Instant::now());
        }
    }

    pub fn auto_hidden_within(&self, window: Duration) -> bool {
        self.last_auto_hide
            .lock()
            .ok()
            .and_then(|last| *last)
            .is_some_and(|last| last.elapsed() < window)
    }

    pub fn set_auto_hide(&self, enabled: bool) {
        self.auto_hide.store(enabled, Ordering::Relaxed);
    }

    pub fn auto_hide(&self) -> bool {
        self.auto_hide.load(Ordering::Relaxed)
    }

    pub fn set_shortcut_status(&self, status: Vec<ShortcutStatus>) {
        if let Ok(mut current) = self.shortcut_status.lock() {
            *current = status;
        }
    }

    pub fn shortcut_status(&self) -> Vec<ShortcutStatus> {
        self.shortcut_status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_default()
    }
}

/// Empty JSON payload used by commands that only report success.
#[derive(Debug, Clone, Serialize)]
pub struct Ack {
    pub ok: bool,
}
