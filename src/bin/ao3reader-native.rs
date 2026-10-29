// No console window in release builds; debug builds keep one for logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Err(e) = ao3reader::native::run() {
        eprintln!("AO3 Reader failed to start: {e}");
        std::process::exit(1);
    }
}
