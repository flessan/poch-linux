#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
#[path = "windows_main.rs"]
mod windows_main;

#[cfg(target_os = "linux")]
#[path = "linux_main.rs"]
mod linux_main;

#[cfg(windows)]
fn main() {
    if let Err(error) = windows_main::run() {
        eprintln!("Poch failed to start: {error}");
    }
}

#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = linux_main::run() {
        eprintln!("Poch failed to start: {error}");
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
compile_error!("Poch currently supports Windows and Linux.");
