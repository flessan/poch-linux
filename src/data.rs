//! Penyimpanan data di %APPDATA%\poch dan trim memori.

use super::*;

pub(crate) fn data_path(name: &str) -> std::path::PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    std::path::Path::new(&base).join("poch").join(name)
}

pub(crate) fn migrate_old_data() {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    let old = std::path::Path::new(&base).join("notch");
    for name in ["todo.txt", "water.txt"] {
        let (src, dst) = (old.join(name), data_path(name));
        if src.exists() && !dst.exists() {
            if let Some(dir) = dst.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::copy(&src, &dst);
        }
    }
}

pub(crate) fn trim_memory() {
    unsafe {
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

pub(crate) fn write_data(name: &str, s: String) {
    let p = data_path(name);
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(p, s);
}
