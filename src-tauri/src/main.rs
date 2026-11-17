// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// The Win32 layer is the only `unsafe` in Zlip and lives in `zlip-platform`.
#![forbid(unsafe_code)]

mod app;
mod capture;
mod commands;
mod controller;
mod logging;
mod paste;
mod state;
mod tray;
mod windows;

fn main() {
    app::run();
}
