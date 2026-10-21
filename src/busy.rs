//! Busy mode: deteksi app Adobe / maximized / fullscreen.

use super::*;

const MAXIMIZED_IS_BUSY: bool = true;
const BUSY_APPS: &[&str] = &[
    "photoshop", "illustrator", "premiere", "afterfx", "indesign", "lightroom", "acrobat", "animate", "audition",
    "adobe",
];

// ---------------------------------------------------------------- busy mode
unsafe fn exe_name(hwnd: HWND) -> String {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 {
        return String::new();
    }
    let h = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
        Ok(h) => h,
        Err(_) => return String::new(),
    };
    let mut buf = [0u16; 520];
    let mut len = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
    let _ = CloseHandle(h);
    if !ok {
        return String::new();
    }
    let full = String::from_utf16_lossy(&buf[..len as usize]);
    full.rsplit('\\').next().unwrap_or("").to_lowercase()
}

pub(crate) unsafe fn is_busy(own: HWND) -> bool {
    let fg = GetForegroundWindow();
    if fg.0.is_null() || fg == own {
        return false;
    }
    let mut cls = [0u16; 64];
    let n = GetClassNameW(fg, &mut cls);
    let class = String::from_utf16_lossy(&cls[..(n.max(0) as usize)]);
    if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
        return false;
    }
    let exe = exe_name(fg);
    if !exe.is_empty() && BUSY_APPS.iter().any(|k| exe.contains(k)) {
        return true;
    }
    if MAXIMIZED_IS_BUSY {
        if IsZoomed(fg).as_bool() {
            return true;
        }
        let mut r = RECT::default();
        if GetWindowRect(fg, &mut r).is_ok() {
            let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
            if r.left <= 0 && r.top <= 0 && r.right >= sw && r.bottom >= sh {
                return true;
            }
        }
    }
    false
}
