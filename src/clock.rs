//! Fitur Clock: slide jam, jam pada notch kecil, helper waktu.

use super::*;

pub(crate) const CLOCK_FADE_IN: f32 = 0.35;

const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
pub(crate) const DAYS_S: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
pub(crate) const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

pub(crate) fn now_tm() -> SYSTEMTIME {
    unsafe { GetLocalTime() }
}

pub(crate) fn today() -> String {
    let st = now_tm();
    format!("{:04}-{:02}-{:02}", st.wYear, st.wMonth, st.wDay)
}

impl App {
    pub(crate) fn clock_text(&self, g: &mut Gfx, w: f32, h: f32, al: f32) {
        let st = now_tm();
        let s = format!("{:02}:{:02}", st.wHour, st.wMinute);
        g.txt_c(&s, CX, ty(h, 14.0), 14.0, [255, 255, 255], al, w, true);
    }

    pub(crate) fn draw_clock_collapsed(&self, g: &mut Gfx, w: f32, h: f32, ca: f32) {
        let fade = ((self.idle_secs() % IDLE_CYCLE_SECS) / CLOCK_FADE_IN).clamp(0.0, 1.0);
        self.clock_text(g, w, h, ca * fade);
    }

    /// Jam besar + hari/tanggal (huruf kapital) di sisi kanan slide Music.
    /// `rx` = tepi kanan blok; semua teks (jam, hari, tanggal) rata kanan di tepi itu.
    pub(crate) fn clock_side(&self, g: &mut Gfx, rx: f32, al: f32) {
        let st = now_tm();
        let hm = format!("{:02}:{:02}", st.wHour, st.wMinute);
        let day = DAYS[(st.wDayOfWeek as usize) % 7].to_uppercase();
        let mon = MONTHS[((st.wMonth as usize).max(1) - 1) % 12].to_uppercase();
        let date = format!("{} {} {}", st.wDay, mon, st.wYear);
        let (hs, ds, ts) = (46.0, 14.0, 12.0);
        let hw = g.width_ex(&hm, hs, true);
        let dw = g.width_ex(&day, ds, true);
        let tw = g.width_ex(&date, ts, true);
        g.txtb(&hm, rx - hw, 40.0, hs, [255, 255, 255], al, 200.0);
        g.txtb(&day, rx - dw, 94.0, ds, [255, 255, 255], al, 200.0);
        g.txtb(&date, rx - tw, 112.0, ts, [200, 200, 205], al, 200.0);
    }
}
