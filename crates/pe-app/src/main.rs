// Suppress the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(err) = pe_app::run() {
        eprintln!("PolarExplorer failed to start: {err:#}");
        std::process::exit(1);
    }
}
