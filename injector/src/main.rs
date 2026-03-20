#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;
mod gui;
mod hotkeys;
mod native;

use std::env;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

fn main() {
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let args: Vec<String> = env::args().collect();
    if args.len() <= 1 {
        // Start hotkey listener and pass its receiver to the GUI.
        let (hotkey_tx, hotkey_rx) = crossbeam_channel::unbounded();
        hotkeys::start(hotkey_tx, std::process::id());
        gui::start(hotkey_rx);
    } else {
        cli::start();
    }
}
