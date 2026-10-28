pub mod app;
pub mod app_window;
pub mod cli;
pub mod commands;
pub mod config;
pub mod error;
pub mod logging;
pub mod models;
pub mod profile;
pub mod raw_i2c;
pub mod service;
pub mod shortcuts;
pub mod state;
pub mod transport;
pub mod util;
pub mod worker;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    app::run();
}
