#![windows_subsystem = "windows"]
//! Poch for Windows 10/11 (flush with the top edge of the screen).
//! - Layered per-pixel alpha window (UpdateLayeredWindow), software rendering (tiny-skia)
//! - Slides: Music, Clock, Water, To-do. Endless carousel: arrow buttons, swipe/scroll, drag, dots
//! - Busy mode: Adobe apps / maximized / fullscreen window -> notch shrinks to a thin line, hover to open
//! - Media: Windows GSMTC (Spotify, browsers, ...). Artwork from GSMTC, fallback to iTunes Search API
//! - Data: %APPDATA%\poch\todo.txt and water.txt (migrated automatically from the old %APPDATA%\notch folder)
//! - Font: Instrument Sans (embedded in the exe, see assets/)
//! - Memory: small embedded fonts, bounded glyph cache, working set trimmed when idle

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tiny_skia::{
    Color, FillRule, FilterQuality, GradientStop, LineCap, LinearGradient, Mask, Paint, Path, PathBuilder, Pattern,
    Pixmap, Point, Rect, SpreadMode, Stroke, Transform,
};
use windows::core::{w, Result, HSTRING, PCWSTR, PWSTR};
use windows::Foundation::Uri;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionMediaProperties as MediaProps,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlayStatus,
};
use windows::Storage::Streams::DataReader;
use windows::Web::Http::HttpClient;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, QueryFullProcessImageNameW, SetProcessWorkingSetSize, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

// ---------------------------------------------------------------- config
const W: i32 = 540; // window (drawing area)
const H: i32 = 180;
const EXP_W: f32 = 500.0; // expanded notch size
const EXP_H: f32 = 172.0;
const CX: f32 = 270.0; // window center
const CL: f32 = 20.0; // expanded notch left edge (CX - EXP_W/2)
const LX: f32 = CL + 48.0; // content left (room for the arrow button)
const RX: f32 = CL + EXP_W - 48.0; // content right
const CW: f32 = RX - LX; // content width
const NAVY: f32 = 92.0; // arrow buttons vertical center
const N_SLIDES: usize = 4; // 0 music, 1 clock, 2 water, 3 to-do
const ART: u32 = 88; // artwork size (px)
const ICON_PX: u32 = 40; // app icon bitmap size (drawn at 20 px in the collapsed notch)
/// App icon (icon.ico in the project root), embedded in the exe.
static ICON_BYTES: &[u8] = include_bytes!("../icon.ico");
// running title (collapsed notch)
const MARQUEE_SPEED: f32 = 38.0; // px per second
const MARQUEE_GAP: f32 = 34.0; // space between the end of the title and its repeat
const MARQUEE_PAUSE: f32 = 1.2; // seconds the title rests at the start of every loop

const THIN: (f32, f32) = (96.0, 4.0); // busy mode
const IDLE: (f32, f32) = (150.0, 32.0); // collapsed, no music (shows clock)
const PLAYING: (f32, f32) = (236.0, 32.0); // collapsed, music playing

/// A maximized / fullscreen foreground window counts as "busy" (many-tab browser, editors, ...).
const MAXIMIZED_IS_BUSY: bool = true;
/// Executable names (lowercase, partial match) that always count as "busy".
const BUSY_APPS: &[&str] = &[
    "photoshop", "illustrator", "premiere", "afterfx", "indesign", "lightroom", "acrobat", "animate", "audition",
    "adobe",
];
/// Send title + artist to the iTunes Search API when the player provides no artwork.
const ONLINE_ARTWORK: bool = true;

const ACCENT: [u8; 3] = [255, 69, 58]; // red accent
const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const DAYS_S: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

// layout shared by drawing and hit-testing
const MX: f32 = LX + 104.0; // music right column
const MW: f32 = RX - MX;
const MCX: f32 = MX + MW / 2.0;
const MCY: f32 = 128.0;
const BX: f32 = RX - 112.0; // water buttons
const LP: f32 = LX + 204.0; // to-do list left (normal mode)
const ROW_H: f32 = 30.0;
const ROW_P: f32 = 35.0;

/// (x, width, first row y, visible rows) of the to-do list.
fn rows_geom(editing: bool) -> (f32, f32, f32, usize) {
    if editing {
        (LX, CW - 10.0, 78.0, 2)
    } else {
        (LP, RX - 10.0 - LP, 44.0, 3)
    }
}

#[derive(Clone, Default, PartialEq)]
struct Media {
    title: String,
    artist: String,
    playing: bool,
    pos: u32, // seconds
    dur: u32, // seconds
}

impl Media {
    fn key(&self) -> String {
        format!("{}|{}", self.title, self.artist)
    }
}

/// Non-premultiplied RGBA -> premultiplied tiny-skia pixmap.
fn pixmap_from_rgba(rgba: &[u8], size: u32) -> Option<Pixmap> {
    let mut pm = Pixmap::new(size, size)?;
    for (d, s) in pm.data_mut().chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
        let al = s[3] as u32;
        d[0] = (s[0] as u32 * al / 255) as u8;
        d[1] = (s[1] as u32 * al / 255) as u8;
        d[2] = (s[2] as u32 * al / 255) as u8;
        d[3] = s[3];
    }
    Some(pm)
}

fn load_icon() -> Option<Pixmap> {
    let img = image::load_from_memory_with_format(ICON_BYTES, image::ImageFormat::Ico).ok()?;
    let img = img.resize_to_fill(ICON_PX, ICON_PX, image::imageops::FilterType::Lanczos3);
    pixmap_from_rgba(&img.to_rgba8().into_raw(), ICON_PX)
}

struct ArtData {
    key: String,
    rgba: Vec<u8>, // ART x ART, non-premultiplied
}

enum Cmd {
    Prev,
    Toggle,
    Next,
    Seek(f32),
}

// ---------------------------------------------------------------- text
// Instrument Sans is embedded (small Latin + Latin-Ext subsets, ~110 KB total). Glyphs missing from both
// (e.g. Cyrillic) fall back to Segoe UI, which is only loaded the first time such a character is drawn.
static FONT_REG: &[u8] = include_bytes!("../assets/InstrumentSans-Regular.ttf");
static FONT_REG_EXT: &[u8] = include_bytes!("../assets/InstrumentSans-Ext-Regular.ttf");
static FONT_BOLD: &[u8] = include_bytes!("../assets/InstrumentSans-Bold.ttf");
static FONT_BOLD_EXT: &[u8] = include_bytes!("../assets/InstrumentSans-Ext-Bold.ttf");

/// Baseline = y + ASC * size. Keeps every text position of the layout where it was tuned.
const ASC: f32 = 1.079;
/// The glyph cache is cleared when it grows past this many entries (keeps memory bounded).
const GLYPH_CACHE_MAX: usize = 400;

type GlyphKey = (char, u32, bool);

struct Text {
    reg: Vec<fontdue::Font>,
    bold: Vec<fontdue::Font>,
    fallback_tried: bool,
    cache: HashMap<GlyphKey, (fontdue::Metrics, Vec<u8>)>,
}

fn load_font(data: &[u8]) -> Option<fontdue::Font> {
    fontdue::Font::from_bytes(data, fontdue::FontSettings::default()).ok()
}

fn glyph_of<'a>(
    cache: &'a mut HashMap<GlyphKey, (fontdue::Metrics, Vec<u8>)>,
    chain: &[fontdue::Font],
    ch: char,
    size: f32,
    bold: bool,
) -> &'a (fontdue::Metrics, Vec<u8>) {
    let key = (ch, size as u32, bold);
    if !cache.contains_key(&key) && cache.len() >= GLYPH_CACHE_MAX {
        cache.clear();
    }
    cache.entry(key).or_insert_with(|| {
        let f = chain.iter().find(|f| f.lookup_glyph_index(ch) != 0).unwrap_or(&chain[0]);
        f.rasterize(ch, size)
    })
}

impl Text {
    fn load() -> Option<Text> {
        let mut reg = vec![load_font(FONT_REG)?];
        let mut bold = vec![load_font(FONT_BOLD)?];
        reg.extend(load_font(FONT_REG_EXT));
        bold.extend(load_font(FONT_BOLD_EXT));
        Some(Text { reg, bold, fallback_tried: false, cache: HashMap::new() })
    }

    /// Loads Segoe UI as a last-resort font the first time a character is not covered by Instrument Sans.
    fn ensure_fallback(&mut self, s: &str) {
        if self.fallback_tried {
            return;
        }
        let missing = s.chars().any(|c| !c.is_control() && self.reg.iter().all(|f| f.lookup_glyph_index(c) == 0));
        if !missing {
            return;
        }
        self.fallback_tried = true;
        if let Some(f) = std::fs::read(r"C:\Windows\Fonts\segoeui.ttf").ok().and_then(|d| load_font(&d)) {
            self.reg.push(f);
        }
        if let Some(f) = std::fs::read(r"C:\Windows\Fonts\segoeuib.ttf").ok().and_then(|d| load_font(&d)) {
            self.bold.push(f);
        }
        self.cache.clear();
    }

    fn measure(&mut self, s: &str, size: f32, bold: bool) -> f32 {
        self.ensure_fallback(s);
        let chain: &[fontdue::Font] = if bold { &self.bold } else { &self.reg };
        let mut total = 0.0;
        for ch in s.chars() {
            let (m, _) = glyph_of(&mut self.cache, chain, ch, size, bold);
            total += m.advance_width;
        }
        total
    }

    /// `clip` = (left, right, fade): only pixels between left..right are drawn, fading out over `fade` px at both ends.
    fn draw(
        &mut self,
        pm: &mut Pixmap,
        s: &str,
        x: f32,
        y: f32,
        size: f32,
        col: [u8; 3],
        alpha: f32,
        max_w: f32,
        bold: bool,
        clip: Option<(f32, f32, f32)>,
    ) {
        if alpha <= 0.02 {
            return;
        }
        self.ensure_fallback(s);
        let (pw, ph) = (pm.width() as i32, pm.height() as i32);
        let chain: &[fontdue::Font] = if bold { &self.bold } else { &self.reg };
        let asc = size * ASC;
        let mut pen = 0.0f32;
        let data = pm.data_mut();
        for ch in s.chars() {
            let (m, bm) = glyph_of(&mut self.cache, chain, ch, size, bold);
            if pen + m.advance_width > max_w {
                break;
            }
            let gx = (x + pen).round() as i32 + m.xmin;
            let gy = (y + asc).round() as i32 - m.height as i32 - m.ymin;
            let skip = match clip {
                Some((cl, cr, _)) => ((gx + m.width as i32) as f32) < cl || (gx as f32) > cr,
                None => false,
            };
            if !skip {
                for row in 0..m.height {
                    for colx in 0..m.width {
                        let cov = bm[row * m.width + colx];
                        if cov == 0 {
                            continue;
                        }
                        let (px, py) = (gx + colx as i32, gy + row as i32);
                        if px < 0 || py < 0 || px >= pw || py >= ph {
                            continue;
                        }
                        let mut a = cov as f32 / 255.0 * alpha;
                        if let Some((cl, cr, fd)) = clip {
                            let fx = px as f32 + 0.5;
                            if fx < cl || fx >= cr {
                                continue;
                            }
                            if fd > 0.0 {
                                a *= ((fx - cl) / fd).clamp(0.0, 1.0) * ((cr - fx) / fd).clamp(0.0, 1.0);
                            }
                        }
                        let i = ((py * pw + px) * 4) as usize;
                        for c in 0..3 {
                            data[i + c] = (col[c] as f32 * a + data[i + c] as f32 * (1.0 - a)) as u8;
                        }
                        data[i + 3] = (255.0 * a + data[i + 3] as f32 * (1.0 - a)) as u8;
                    }
                }
            }
            pen += m.advance_width;
        }
    }
}

// ---------------------------------------------------------------- drawing primitives
struct Gfx {
    pm: Pixmap,
    text: Option<Text>,
    mask: Option<Mask>, // reused every frame instead of being re-allocated
}

impl Gfx {
    fn fill(&mut self, path: &Path, p: &Paint) {
        self.pm.fill_path(path, p, FillRule::Winding, Transform::identity(), None);
    }
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, p: &Paint) {
        if let Some(r) = Rect::from_xywh(x, y, w, h) {
            self.pm.fill_rect(r, p, Transform::identity(), None);
        }
    }
    fn circle(&mut self, x: f32, y: f32, r: f32, p: &Paint) {
        if let Some(path) = PathBuilder::from_circle(x, y, r) {
            self.fill(&path, p);
        }
    }
    fn poly(&mut self, pts: &[(f32, f32)], p: &Paint) {
        let mut pb = PathBuilder::new();
        pb.move_to(pts[0].0, pts[0].1);
        for q in &pts[1..] {
            pb.line_to(q.0, q.1);
        }
        pb.close();
        if let Some(path) = pb.finish() {
            self.fill(&path, p);
        }
    }
    /// Thick line with round ends.
    fn line(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, wd: f32, p: &Paint) {
        let (dx, dy) = (x2 - x1, y2 - y1);
        let len = (dx * dx + dy * dy).sqrt().max(0.001);
        let (nx, ny) = (-dy / len * wd / 2.0, dx / len * wd / 2.0);
        self.poly(&[(x1 + nx, y1 + ny), (x2 + nx, y2 + ny), (x2 - nx, y2 - ny), (x1 - nx, y1 - ny)], p);
        self.circle(x1, y1, wd / 2.0, p);
        self.circle(x2, y2, wd / 2.0, p);
    }
    fn txt_ex(&mut self, s: &str, x: f32, y: f32, size: f32, col: [u8; 3], al: f32, max_w: f32, bold: bool) {
        if let Some(t) = self.text.as_mut() {
            t.draw(&mut self.pm, s, x, y, size, col, al, max_w, bold, None);
        }
    }
    /// Text clipped to `cl..cr` with soft faded edges (used by the running title).
    fn txt_clip(&mut self, s: &str, x: f32, y: f32, size: f32, col: [u8; 3], al: f32, cl: f32, cr: f32, fade: f32) {
        if let Some(t) = self.text.as_mut() {
            t.draw(&mut self.pm, s, x, y, size, col, al, f32::INFINITY, false, Some((cl, cr, fade)));
        }
    }
    fn txt(&mut self, s: &str, x: f32, y: f32, size: f32, col: [u8; 3], al: f32, max_w: f32) {
        self.txt_ex(s, x, y, size, col, al, max_w, false);
    }
    fn txtb(&mut self, s: &str, x: f32, y: f32, size: f32, col: [u8; 3], al: f32, max_w: f32) {
        self.txt_ex(s, x, y, size, col, al, max_w, true);
    }
    fn width_ex(&mut self, s: &str, size: f32, bold: bool) -> f32 {
        self.text.as_mut().map(|t| t.measure(s, size, bold)).unwrap_or(0.0)
    }
    fn width(&mut self, s: &str, size: f32) -> f32 {
        self.width_ex(s, size, false)
    }
    fn txt_c(&mut self, s: &str, cx: f32, y: f32, size: f32, col: [u8; 3], al: f32, max_w: f32, bold: bool) {
        let tw = self.width_ex(s, size, bold).min(max_w);
        self.txt_ex(s, cx - tw / 2.0, y, size, col, al, max_w, bold);
    }
    /// Progress ring.
    fn ring(&mut self, cx: f32, cy: f32, r: f32, frac: f32, wd: f32, track: &Paint, fg: &Paint) {
        if let Some(path) = PathBuilder::from_circle(cx, cy, r) {
            let st = Stroke { width: wd, ..Default::default() };
            self.pm.stroke_path(&path, track, &st, Transform::identity(), None);
        }
        if frac > 0.001 {
            let steps = ((frac * 64.0).ceil() as i32).max(2);
            let mut pb = PathBuilder::new();
            for s in 0..=steps {
                let ang = -std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * frac.min(1.0) * s as f32 / steps as f32;
                let (px, py) = (cx + r * ang.cos(), cy + r * ang.sin());
                if s == 0 {
                    pb.move_to(px, py);
                } else {
                    pb.line_to(px, py);
                }
            }
            if let Some(path) = pb.finish() {
                let st = Stroke { width: wd, line_cap: LineCap::Round, ..Default::default() };
                self.pm.stroke_path(&path, fg, &st, Transform::identity(), None);
            }
        }
    }
}

fn dummy_path() -> Path {
    PathBuilder::from_rect(Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap())
}

fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Path {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish().unwrap_or_else(dummy_path)
}

/// Notch shape: top edge flush with the screen, concave "ears" at the top corners,
/// rounded bottom corners (like the MacBook notch).
fn notch_path(w: f32, h: f32) -> Path {
    let ear = (h * 0.35).min(10.0);
    let rb = (h * 0.5).min(22.0);
    let l = CX - w / 2.0;
    let r = CX + w / 2.0;
    let mut pb = PathBuilder::new();
    pb.move_to(l - ear, 0.0);
    pb.line_to(r + ear, 0.0);
    pb.quad_to(r, 0.0, r, ear);
    pb.line_to(r, h - rb);
    pb.quad_to(r, h, r - rb, h);
    pb.line_to(l + rb, h);
    pb.quad_to(l, h, l, h - rb);
    pb.line_to(l, ear);
    pb.quad_to(l, 0.0, l - ear, 0.0);
    pb.close();
    pb.finish().unwrap_or_else(dummy_path)
}

/// Color glow rising from the bottom edge and fading to transparent toward the top.
fn glow(g: &mut Gfx, shape: &Path, h: f32, rgb: (u8, u8, u8), a: f32) {
    if a <= 0.01 {
        return;
    }
    let c = |al: f32| Color::from_rgba8(rgb.0, rgb.1, rgb.2, (al.clamp(0.0, 1.0) * 255.0) as u8);
    let stops = vec![GradientStop::new(0.0, c(a)), GradientStop::new(0.55, c(a * 0.28)), GradientStop::new(1.0, c(0.0))];
    if let Some(sh) =
        LinearGradient::new(Point::from_xy(0.0, h), Point::from_xy(0.0, h * 0.08), stops, SpreadMode::Pad, Transform::identity())
    {
        let mut p = Paint::default();
        p.shader = sh;
        p.anti_alias = true;
        g.fill(shape, &p);
    }
}

fn droplet(g: &mut Gfx, cx: f32, cy: f32, s: f32, p: &Paint) {
    let mut pb = PathBuilder::new();
    pb.move_to(cx, cy - s);
    pb.cubic_to(cx + s * 0.9, cy - s * 0.1, cx + s * 0.8, cy + s, cx, cy + s);
    pb.cubic_to(cx - s * 0.8, cy + s, cx - s * 0.9, cy - s * 0.1, cx, cy - s);
    pb.close();
    if let Some(path) = pb.finish() {
        g.fill(&path, p);
    }
}

fn paint(r: u8, g: u8, b: u8, a: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(r, g, b, (a.clamp(0.0, 1.0) * 255.0) as u8);
    p.anti_alias = true;
    p
}

fn acc(a: f32) -> Paint<'static> {
    paint(ACCENT[0], ACCENT[1], ACCENT[2], a)
}

fn in_rect(x: f32, y: f32, rx: f32, ry: f32, rw: f32, rh: f32) -> bool {
    x >= rx && x <= rx + rw && y >= ry && y <= ry + rh
}

fn ty(h: f32, size: f32) -> f32 {
    (h - size * 1.33) / 2.0
}

fn fmt_time(s: u32) -> String {
    format!("{}:{:02}", s / 60, s % 60)
}

/// Shortest signed distance on the circular slide track.
fn cyc(d: f32) -> f32 {
    let n = N_SLIDES as f32;
    d - n * (d / n).round()
}

fn dot_x(i: usize) -> f32 {
    CX + (i as f32 - (N_SLIDES as f32 - 1.0) / 2.0) * 14.0
}

const NAV_L: (f32, f32) = (CL + 22.0, NAVY);
const NAV_R: (f32, f32) = (CL + EXP_W - 22.0, NAVY);

// ---------------------------------------------------------------- time & data
fn now_tm() -> SYSTEMTIME {
    unsafe { GetLocalTime() }
}

fn today() -> String {
    let st = now_tm();
    format!("{:04}-{:02}-{:02}", st.wYear, st.wMonth, st.wDay)
}

fn data_path(name: &str) -> std::path::PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
    std::path::Path::new(&base).join("poch").join(name)
}

/// One-time copy of todo/water data from the old %APPDATA%\notch folder.
fn migrate_old_data() {
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

/// Hand unused pages back to Windows (shrinks the working set shown in Task Manager).
fn trim_memory() {
    unsafe {
        let _ = SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX);
    }
}

fn write_data(name: &str, s: String) {
    let p = data_path(name);
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(p, s);
}

fn load_todos() -> Vec<(bool, String)> {
    match std::fs::read_to_string(data_path("todo.txt")) {
        Ok(s) => s
            .lines()
            .filter_map(|l| l.split_once('|').map(|(d, t)| (d == "1", t.to_string())))
            .collect(),
        Err(_) => {
            let d = vec![(false, "Tap Add Task to create a task".to_string())];
            save_todos(&d);
            d
        }
    }
}

fn save_todos(t: &[(bool, String)]) {
    let s: String = t.iter().map(|(d, x)| format!("{}|{}\n", if *d { 1 } else { 0 }, x)).collect();
    write_data("todo.txt", s);
}

struct Water {
    day: String,
    ml: u32,
    goal: u32,
    remind_min: u32, // 0 = off
    next: Instant,
}

impl Water {
    fn load() -> Water {
        let mut w = Water { day: today(), ml: 0, goal: 2000, remind_min: 60, next: Instant::now() };
        if let Ok(s) = std::fs::read_to_string(data_path("water.txt")) {
            let p: Vec<&str> = s.trim().split('|').collect();
            if p.len() >= 4 {
                w.goal = p[2].parse().unwrap_or(2000);
                w.remind_min = p[3].parse().unwrap_or(60);
                if p[0] == w.day {
                    w.ml = p[1].parse().unwrap_or(0);
                }
            }
        }
        w.reset_timer();
        w
    }
    fn save(&self) {
        write_data("water.txt", format!("{}|{}|{}|{}\n", self.day, self.ml, self.goal, self.remind_min));
    }
    fn reset_timer(&mut self) {
        self.next = Instant::now() + Duration::from_secs(self.remind_min.max(1) as u64 * 60);
    }
    fn roll(&mut self) -> bool {
        let d = today();
        if d != self.day {
            self.day = d;
            self.ml = 0;
            self.save();
            return true;
        }
        false
    }
    fn add(&mut self, d: i32) {
        self.ml = (self.ml as i32 + d).max(0) as u32;
        self.reset_timer();
        self.save();
    }
}

// ---------------------------------------------------------------- state
struct App {
    hwnd: isize,
    wx: i32,
    mem_dc: isize,
    bits: usize,
    gfx: Option<Gfx>,
    // shape animation
    w: f32,
    h: f32,
    vw: f32,
    vh: f32,
    // slide animation (endless: `slide` is unbounded, shown index = slide mod N)
    slide: i32,
    sp: f32,
    sv: f32,
    drag: Option<(f32, f32)>,
    drag_dx: f32,
    last_wheel: Instant,
    t: f32,
    last: Instant,
    interval: u32,
    tick_n: u32,
    last_sec: u16,
    last_min: u16,
    // status
    hover: bool,
    busy: bool,
    menu_open: bool,
    peek_until: Option<Instant>,
    reminding: bool,
    // data
    shared: Arc<Mutex<Media>>,
    art_shared: Arc<Mutex<Option<ArtData>>>,
    media: Media,
    art: Option<Pixmap>,
    icon: Option<Pixmap>,
    art_key: String,
    title_t0: f32, // `t` at the moment the current title started (restarts the running text)
    todos: Vec<(bool, String)>,
    todo_scroll: usize,
    edit_mode: bool,
    editing: bool,
    edit_idx: Option<usize>,
    input: String,
    caret_on: bool,
    edit_start: Instant,
    prev_fg: isize,
    water: Water,
    tx: Sender<Cmd>,
}

static APP: Mutex<Option<App>> = Mutex::new(None);

fn with_app<F: FnOnce(&mut App)>(f: F) {
    // try_lock: avoid deadlock if Windows sends a message while we are drawing
    if let Ok(mut g) = APP.try_lock() {
        if let Some(a) = g.as_mut() {
            f(a);
        }
    }
}

impl App {
    fn expanded(&self) -> bool {
        self.hover || self.editing || self.peek_until.map_or(false, |t| Instant::now() < t)
    }

    fn ready(&self) -> bool {
        self.h >= EXP_H - 30.0
    }

    fn cur(&self) -> usize {
        self.slide.rem_euclid(N_SLIDES as i32) as usize
    }

    /// Move one slide; wraps around endlessly.
    fn step(&mut self, dir: i32) {
        self.slide += dir;
    }

    /// Jump to a slide index by the shortest way around.
    fn show(&mut self, idx: usize) {
        let n = N_SLIDES as i32;
        let mut d = (idx as i32 - self.cur() as i32).rem_euclid(n);
        if d > n / 2 {
            d -= n;
        }
        self.slide += d;
    }

    fn set_art(&mut self, rgba: &[u8]) {
        self.art = pixmap_from_rgba(rgba, ART);
    }

    fn water_add(&mut self, d: i32) {
        self.water.add(d);
        self.reminding = false;
        self.peek_until = None;
    }

    /// Unfinished tasks first, finished ones at the bottom.
    fn todo_order(&self) -> Vec<usize> {
        let mut v: Vec<usize> = (0..self.todos.len()).filter(|&i| !self.todos[i].0).collect();
        v.extend((0..self.todos.len()).filter(|&i| self.todos[i].0));
        v
    }

    fn fix_scroll(&mut self) {
        let max = self.todos.len().saturating_sub(3);
        self.todo_scroll = self.todo_scroll.min(max);
    }

    unsafe fn begin_edit(&mut self, idx: Option<usize>) {
        self.editing = true;
        self.edit_idx = idx;
        self.input = idx.map(|i| self.todos[i].1.clone()).unwrap_or_default();
        self.caret_on = true;
        self.edit_start = Instant::now();
        self.prev_fg = GetForegroundWindow().0 as isize;
        let _ = SetForegroundWindow(HWND(self.hwnd as *mut c_void));
    }

    unsafe fn finish_edit(&mut self, commit: bool, restore: bool) {
        if commit {
            let t = self.input.trim().to_string();
            if !t.is_empty() {
                match self.edit_idx {
                    Some(i) if i < self.todos.len() => self.todos[i].1 = t,
                    _ => self.todos.push((false, t)),
                }
                save_todos(&self.todos);
            }
        }
        self.editing = false;
        self.edit_idx = None;
        self.input.clear();
        if restore && self.prev_fg != 0 {
            let _ = SetForegroundWindow(HWND(self.prev_fg as *mut c_void));
        }
    }

    // ------------------------------------------------------------ drawing
    fn art_at(&self, g: &mut Gfx, x: f32, y: f32, size: f32, r: f32, al: f32) {
        let path = rrect(x, y, size, size, r);
        if let Some(art) = &self.art {
            let s = size / ART as f32;
            let mut p = Paint::default();
            p.shader = Pattern::new(
                art.as_ref(),
                SpreadMode::Pad,
                FilterQuality::Bilinear,
                al,
                Transform::from_row(s, 0.0, 0.0, s, x, y),
            );
            p.anti_alias = true;
            g.fill(&path, &p);
        } else {
            g.fill(&path, &paint(46, 46, 52, al));
            let (cx, cy, s) = (x + size / 2.0, y + size / 2.0, size / 88.0);
            let n = paint(150, 150, 160, al);
            g.circle(cx - 5.0 * s, cy + 12.0 * s, 7.0 * s, &n);
            g.rect(cx + 1.0 * s, cy - 18.0 * s, 3.5 * s, 30.0 * s, &n);
            g.rect(cx + 1.0 * s, cy - 18.0 * s, 13.0 * s, 6.0 * s, &n);
        }
    }

    /// App icon (rounded), drawn instead of the song artwork in the collapsed notch.
    fn icon_at(&self, g: &mut Gfx, x: f32, y: f32, size: f32, r: f32, al: f32) {
        let path = rrect(x, y, size, size, r);
        if let Some(icon) = &self.icon {
            let s = size / ICON_PX as f32;
            let mut p = Paint::default();
            p.shader = Pattern::new(
                icon.as_ref(),
                SpreadMode::Pad,
                FilterQuality::Bilinear,
                al,
                Transform::from_row(s, 0.0, 0.0, s, x, y),
            );
            p.anti_alias = true;
            g.fill(&path, &p);
        } else {
            g.fill(&path, &acc(al));
        }
    }

    fn draw_collapsed(&self, g: &mut Gfx, w: f32, h: f32, ca: f32) {
        let x0 = CX - w / 2.0;
        if self.media.playing {
            self.icon_at(g, x0 + 14.0, (h - 20.0) / 2.0, 20.0, 5.0, ca);
            for i in 0..4 {
                let bh = 4.0 + 12.0 * (0.5 + 0.5 * (self.t * 6.0 + i as f32 * 1.7).sin());
                g.rect(x0 + w - 34.0 + i as f32 * 6.0, h / 2.0 - bh / 2.0, 3.0, bh, &acc(ca));
            }
            // title: centered between the icon and the equalizer; runs (marquee) when too long
            let (rl, rr) = (x0 + 44.0, x0 + w - 44.0);
            let rw = rr - rl;
            let title = self.media.title.as_str();
            let size = 12.0;
            let tw = g.width(title, size);
            let y = ty(h, size);
            if tw <= rw {
                g.txt(title, rl + (rw - tw) / 2.0, y, size, [255, 255, 255], ca, rw + 2.0);
            } else {
                let cycle = tw + MARQUEE_GAP;
                let period = MARQUEE_PAUSE + cycle / MARQUEE_SPEED;
                let e = (self.t - self.title_t0).max(0.0) % period;
                let off = if e < MARQUEE_PAUSE { 0.0 } else { (e - MARQUEE_PAUSE) * MARQUEE_SPEED };
                let (cl, cr) = (rl - 6.0, rr + 6.0);
                g.txt_clip(title, rl - off, y, size, [255, 255, 255], ca, cl, cr, 6.0);
                g.txt_clip(title, rl - off + cycle, y, size, [255, 255, 255], ca, cl, cr, 6.0);
            }
        } else {
            let st = now_tm();
            let s = format!("{:02}:{:02}", st.wHour, st.wMinute);
            g.txt_c(&s, CX, ty(h, 14.0), 14.0, [255, 255, 255], ca, w, true);
        }
    }

    fn slide_music(&self, g: &mut Gfx, ox: f32, al: f32) {
        g.txt("Now Playing", LX + ox, 14.0, 12.0, [140, 140, 145], al, 200.0);
        self.art_at(g, LX + ox, 44.0, 88.0, 12.0, al);
        let title = if self.media.title.is_empty() { "No media" } else { &self.media.title };
        g.txtb(title, MX + ox, 40.0, 17.0, [255, 255, 255], al, MW);
        g.txt(&self.media.artist, MX + ox, 64.0, 13.0, [170, 170, 175], al, MW);
        if self.media.dur > 0 {
            let fr = (self.media.pos as f32 / self.media.dur as f32).clamp(0.0, 1.0);
            g.fill(&rrect(MX + ox, 96.0, MW, 4.0, 2.0), &paint(255, 255, 255, 0.18 * al));
            g.fill(&rrect(MX + ox, 96.0, (MW * fr).max(4.0), 4.0, 2.0), &acc(al));
            g.txt(&fmt_time(self.media.pos), MX + ox, 104.0, 11.0, [140, 140, 145], al, 60.0);
            let ds = fmt_time(self.media.dur);
            let dw = g.width(&ds, 11.0);
            g.txt(&ds, MX + ox + MW - dw, 104.0, 11.0, [140, 140, 145], al, 60.0);
        }
        let cx = MCX + ox;
        let cy = MCY;
        let white = paint(255, 255, 255, al);
        let px = cx - 56.0;
        g.poly(&[(px + 6.0, cy - 7.0), (px + 6.0, cy + 7.0), (px - 6.0, cy)], &white);
        g.rect(px - 9.0, cy - 7.0, 2.5, 14.0, &white);
        let nx = cx + 56.0;
        g.poly(&[(nx - 6.0, cy - 7.0), (nx - 6.0, cy + 7.0), (nx + 6.0, cy)], &white);
        g.rect(nx + 6.5, cy - 7.0, 2.5, 14.0, &white);
        g.circle(cx, cy, 17.0, &white);
        let blk = paint(0, 0, 0, al);
        if self.media.playing {
            g.rect(cx - 6.0, cy - 7.0, 4.0, 14.0, &blk);
            g.rect(cx + 2.0, cy - 7.0, 4.0, 14.0, &blk);
        } else {
            g.poly(&[(cx - 4.0, cy - 8.0), (cx - 4.0, cy + 8.0), (cx + 8.0, cy)], &blk);
        }
    }

    fn slide_clock(&self, g: &mut Gfx, ox: f32, al: f32) {
        let st = now_tm();
        let hm = format!("{:02}:{:02}", st.wHour, st.wMinute);
        let sec = format!(":{:02}", st.wSecond);
        let tw = g.width_ex(&hm, 58.0, true);
        let sw = g.width_ex(&sec, 22.0, true);
        let x0 = CX + ox - (tw + 4.0 + sw) / 2.0;
        g.txtb(&hm, x0, 26.0, 58.0, [255, 255, 255], al, 320.0);
        g.txtb(&sec, x0 + tw + 4.0, 65.0, 22.0, [255, 150, 140], al, 80.0);
        let day = DAYS[(st.wDayOfWeek as usize) % 7];
        let mon = MONTHS[((st.wMonth as usize).max(1) - 1) % 12];
        let date = format!("{}, {} {} {}", day, st.wDay, mon, st.wYear);
        g.txt_c(&date, CX + ox, 108.0, 15.0, [255, 215, 210], al, 420.0, false);
    }

    fn slide_water(&self, g: &mut Gfx, ox: f32, al: f32) {
        let blue = paint(80, 170, 255, al);
        droplet(g, LX + 6.0 + ox, 23.0, 7.0, &blue);
        let (label, lc) = if self.reminding { ("Time to drink water", [120, 190, 255]) } else { ("Water", [170, 170, 175]) };
        g.txtb(label, LX + 20.0 + ox, 14.0, 13.0, lc, al, 240.0);
        let wt = &self.water;
        g.txtb(&format!("{:.2} L", wt.ml as f32 / 1000.0), LX + ox, 40.0, 38.0, [255, 255, 255], al, 230.0);
        let pct = if wt.goal > 0 { (wt.ml * 100 / wt.goal).min(999) } else { 0 };
        let sub = if wt.ml >= wt.goal {
            format!("Goal of {:.1} L reached", wt.goal as f32 / 1000.0)
        } else {
            format!("of {:.1} L  \u{b7}  {}%", wt.goal as f32 / 1000.0, pct)
        };
        g.txt(&sub, LX + ox, 92.0, 13.0, [170, 170, 175], al, 230.0);
        for (label, by) in [("+250 ml", 42.0f32), ("+500 ml", 84.0f32)] {
            g.fill(&rrect(BX + ox, by, 112.0, 34.0, 17.0), &paint(80, 170, 255, 0.24 * al));
            g.txt_c(label, BX + ox + 56.0, by + ty(34.0, 15.0), 15.0, [205, 230, 255], al, 100.0, true);
        }
        g.fill(&rrect(LX + ox, 130.0, CW, 8.0, 4.0), &paint(255, 255, 255, 0.14 * al));
        if wt.goal > 0 && wt.ml > 0 {
            let fw = (CW * (wt.ml as f32 / wt.goal as f32).min(1.0)).max(8.0);
            g.fill(&rrect(LX + ox, 130.0, fw, 8.0, 4.0), &blue);
        }
    }

    fn todo_row(&self, g: &mut Gfx, x: f32, y: f32, w: f32, ri: usize, al: f32) {
        let (dn, s) = (self.todos[ri].0, &self.todos[ri].1);
        if dn {
            g.fill(&rrect(x, y, w, ROW_H, 10.0), &acc(0.20 * al));
        } else {
            g.fill(&rrect(x, y, w, ROW_H, 10.0), &paint(255, 255, 255, 0.08 * al));
        }
        let (cx, cy) = (x + 17.0, y + ROW_H / 2.0);
        if dn {
            g.circle(cx, cy, 8.0, &acc(al));
            let wh = paint(255, 255, 255, al);
            g.line(cx - 3.6, cy + 0.4, cx - 1.0, cy + 3.0, 1.9, &wh);
            g.line(cx - 1.0, cy + 3.0, cx + 3.8, cy - 2.8, 1.9, &wh);
        } else {
            g.circle(cx, cy, 8.0, &paint(255, 255, 255, 0.32 * al));
            g.circle(cx, cy, 6.7, &paint(22, 22, 24, al));
        }
        let col = if dn { [150, 150, 150] } else { [242, 242, 242] };
        let tx = x + 33.0;
        let maxw = w - 33.0 - 10.0 - if self.edit_mode { 24.0 } else { 0.0 };
        g.txt(s, tx, y + ty(ROW_H, 13.0), 13.0, col, al, maxw);
        if dn {
            let tw = g.width(s, 13.0).min(maxw);
            g.rect(tx, cy, tw, 1.3, &paint(175, 175, 175, 0.9 * al));
        }
        if self.edit_mode {
            let (bx, by) = (x + w - 17.0, cy);
            g.circle(bx, by, 8.0, &acc(0.9 * al));
            let wh = paint(255, 255, 255, al);
            g.line(bx - 2.8, by - 2.8, bx + 2.8, by + 2.8, 1.6, &wh);
            g.line(bx - 2.8, by + 2.8, bx + 2.8, by - 2.8, 1.6, &wh);
        }
    }

    fn pill(&self, g: &mut Gfx, x: f32, y: f32, label: &str, edit_icon: bool, active: bool, al: f32) {
        let bg = if active { acc(0.95 * al) } else { paint(255, 255, 255, 0.11 * al) };
        g.fill(&rrect(x, y, 92.0, 28.0, 14.0), &bg);
        let (ix, iy) = (x + 16.0, y + 14.0);
        let wh = paint(255, 255, 255, al);
        if edit_icon {
            g.line(ix - 4.0, iy + 4.5, ix + 4.0, iy - 4.0, 2.4, &wh);
            g.poly(&[(ix - 5.5, iy + 6.0), (ix - 5.0, iy + 2.5), (ix - 2.5, iy + 5.0)], &wh);
        } else {
            g.circle(ix, iy, 7.0, &wh);
            let bk = paint(0, 0, 0, al);
            g.rect(ix - 3.5, iy - 0.8, 7.0, 1.6, &bk);
            g.rect(ix - 0.8, iy - 3.5, 1.6, 7.0, &bk);
        }
        g.txt(label, x + 30.0, y + ty(28.0, 12.0), 12.0, [255, 255, 255], al, 60.0);
    }

    fn slide_todo(&self, g: &mut Gfx, ox: f32, al: f32) {
        // header: "TodoList" + ring + date + time
        let x = LX + ox;
        g.txtb("Todo", x, 12.0, 15.0, [255, 255, 255], al, 80.0);
        let tw = g.width_ex("Todo", 15.0, true);
        g.txtb("List", x + tw, 12.0, 15.0, ACCENT, al, 80.0);
        let st = now_tm();
        let time = format!("{:02}:{:02}", st.wHour, st.wMinute);
        let date = format!(
            "{} {} {} {}",
            DAYS_S[(st.wDayOfWeek as usize) % 7],
            st.wDay,
            MONTHS[((st.wMonth as usize).max(1) - 1) % 12],
            st.wYear
        );
        let timew = g.width_ex(&time, 13.0, true);
        let datew = g.width(&date, 13.0);
        let tx = RX + ox - timew;
        let dx = tx - 12.0 - datew;
        g.txt(&date, dx, 14.0, 13.0, [235, 235, 235], al, 200.0);
        g.txtb(&time, tx, 14.0, 13.0, [255, 255, 255], al, 80.0);
        let done = self.todos.iter().filter(|t| t.0).count();
        let total = self.todos.len();
        let frac = if total > 0 { done as f32 / total as f32 } else { 0.0 };
        g.ring(dx - 16.0, 23.0, 8.0, frac, 3.0, &paint(255, 255, 255, 0.16 * al), &acc(al));

        let (rx, rw, y0, max) = rows_geom(self.editing);
        if self.editing {
            g.fill(&rrect(LX - 1.0 + ox, 39.0, CW + 2.0, 32.0, 11.0), &acc(0.55 * al));
            g.fill(&rrect(LX + ox, 40.0, CW, 30.0, 10.0), &paint(26, 26, 29, al));
            let shown: &str = if self.input.is_empty() { "Type a task, Enter to save, Esc to cancel" } else { &self.input };
            let col = if self.input.is_empty() { [120, 120, 125] } else { [255, 255, 255] };
            g.txt(shown, LX + 12.0 + ox, 40.0 + ty(30.0, 13.0), 13.0, col, al, CW - 30.0);
            if self.caret_on {
                let cw = if self.input.is_empty() { 0.0 } else { g.width(&self.input, 13.0).min(CW - 30.0) };
                g.rect(LX + 12.0 + ox + cw + 1.0, 46.0, 1.5, 18.0, &paint(255, 255, 255, al));
            }
        } else {
            // left panel: big count + buttons
            g.txtb(&format!("{}/{}", done, total), x, 36.0, 42.0, [255, 255, 255], al, 190.0);
            g.txt("Tasks done today", x, 92.0, 12.0, [150, 150, 155], al, 190.0);
            self.pill(g, x, 116.0, "Add Task", false, false, al);
            self.pill(g, x + 98.0, 116.0, if self.edit_mode { "Done" } else { "Edit Task" }, true, self.edit_mode, al);
            if self.todos.is_empty() {
                g.txt("No tasks yet", rx + ox + 8.0, 56.0, 13.0, [140, 140, 145], al, 180.0);
            }
        }
        let order = self.todo_order();
        for (vi, &ri) in order.iter().skip(self.todo_scroll).take(max).enumerate() {
            self.todo_row(g, rx + ox, y0 + vi as f32 * ROW_P, rw, ri, al);
        }
        // scrollbar
        let n = order.len();
        if n > max {
            let th = max as f32 * ROW_P - (ROW_P - ROW_H);
            let thumb = (th * max as f32 / n as f32).max(14.0);
            let ty0 = y0 + (th - thumb) * self.todo_scroll as f32 / (n - max) as f32;
            g.fill(&rrect(RX - 4.0 + ox, y0, 3.0, th, 1.5), &paint(255, 255, 255, 0.10 * al));
            g.fill(&rrect(RX - 4.0 + ox, ty0, 3.0, thumb, 1.5), &paint(255, 255, 255, 0.55 * al));
        }
    }

    fn draw_nav(&self, g: &mut Gfx, p: f32) {
        let bg = paint(255, 255, 255, 0.12 * p);
        let wh = paint(255, 255, 255, 0.95 * p);
        let (lx, ly) = NAV_L;
        g.circle(lx, ly, 13.0, &bg);
        g.line(lx + 2.5, ly - 5.5, lx - 2.5, ly, 2.2, &wh);
        g.line(lx - 2.5, ly, lx + 2.5, ly + 5.5, 2.2, &wh);
        let (rx, ry) = NAV_R;
        g.circle(rx, ry, 13.0, &bg);
        g.line(rx - 2.5, ry - 5.5, rx + 2.5, ry, 2.2, &wh);
        g.line(rx + 2.5, ry, rx - 2.5, ry + 5.5, 2.2, &wh);
    }

    fn draw_dots(&self, g: &mut Gfx, pos: f32, p: f32) {
        for i in 0..N_SLIDES {
            let near = (1.0 - cyc(i as f32 - pos).abs()).clamp(0.0, 1.0);
            let r = 2.3 + 1.2 * near;
            let a = (0.28 + 0.72 * near) * p;
            if near > 0.5 {
                g.circle(dot_x(i), EXP_H - 11.0, r, &acc(a));
            } else {
                g.circle(dot_x(i), EXP_H - 11.0, r, &paint(255, 255, 255, a));
            }
        }
    }
}

fn draw(a: &mut App) {
    let mut g = a.gfx.take().unwrap();
    g.pm.fill(Color::TRANSPARENT);
    let (w, h) = (a.w, a.h);
    let shape = notch_path(w, h);
    g.fill(&shape, &paint(0, 0, 0, 1.0));

    let p = ((h - 40.0) / (EXP_H - 40.0)).clamp(0.0, 1.0); // expand progress
    let ca = (1.0 - p * 2.0).clamp(0.0, 1.0) * ((h - 16.0) / 12.0).clamp(0.0, 1.0);
    if ca > 0.03 {
        a.draw_collapsed(&mut g, w, h, ca);
    }
    if p > 0.04 {
        let pos = a.sp - a.drag_dx / 160.0;
        let mut vis: Vec<(usize, f32, f32)> = Vec::new(); // (slide, x offset, alpha)
        for i in 0..N_SLIDES {
            let d = cyc(i as f32 - pos);
            if d.abs() >= 1.0 {
                continue;
            }
            vis.push((i, d * 48.0, (1.0 - d.abs()) * p));
        }
        // background glow (red gradient from the bottom)
        for &(i, _, al) in &vis {
            match i {
                1 => glow(&mut g, &shape, h, (235, 30, 55), 0.85 * al),
                3 => glow(&mut g, &shape, h, (230, 30, 50), 0.42 * al),
                2 => glow(&mut g, &shape, h, (30, 110, 235), 0.40 * al),
                _ => {}
            }
        }
        for &(i, ox, al) in &vis {
            match i {
                0 => a.slide_music(&mut g, ox, al),
                1 => a.slide_clock(&mut g, ox, al),
                2 => a.slide_water(&mut g, ox, al),
                _ => a.slide_todo(&mut g, ox, al),
            }
        }
        a.draw_nav(&mut g, p);
        a.draw_dots(&mut g, pos, p);
        // clip anything outside the notch silhouette
        if let Some(mut mask) = g.mask.take() {
            mask.data_mut().fill(0);
            mask.fill_path(&shape, FillRule::Winding, true, Transform::identity());
            for (px, &c) in g.pm.data_mut().chunks_exact_mut(4).zip(mask.data().iter()) {
                if c < 255 {
                    let c = c as u32;
                    for v in px.iter_mut() {
                        *v = ((*v as u32 * c + 127) / 255) as u8;
                    }
                }
            }
            g.mask = Some(mask);
        }
    }
    a.gfx = Some(g);
}

unsafe fn present(a: &mut App) {
    draw(a);
    let g = a.gfx.as_ref().unwrap();
    let dst = std::slice::from_raw_parts_mut(a.bits as *mut u8, (W * H * 4) as usize);
    for (d, s) in dst.chunks_exact_mut(4).zip(g.pm.data().chunks_exact(4)) {
        d[0] = s[2];
        d[1] = s[1];
        d[2] = s[0];
        d[3] = s[3];
    }
    let size = SIZE { cx: W, cy: H };
    let src = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let _ = UpdateLayeredWindow(
        HWND(a.hwnd as *mut c_void),
        HDC::default(),
        None,
        Some(&size),
        HDC(a.mem_dc as *mut c_void),
        Some(&src),
        COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    );
}

// ---------------------------------------------------------------- media thread
fn read_thumbnail(props: &MediaProps) -> Option<Vec<u8>> {
    let r = props.Thumbnail().ok()?;
    let s = r.OpenReadAsync().ok()?.get().ok()?;
    let size = s.Size().ok()? as u32;
    if size == 0 || size > 8_000_000 {
        return None;
    }
    let reader = DataReader::CreateDataReader(&s).ok()?;
    reader.LoadAsync(size).ok()?.get().ok()?;
    let mut buf = vec![0u8; size as usize];
    reader.ReadBytes(&mut buf).ok()?;
    Some(buf)
}

fn http_get(url: &str) -> Option<Vec<u8>> {
    let client = HttpClient::new().ok()?;
    let uri = Uri::CreateUri(&HSTRING::from(url)).ok()?;
    let buf = client.GetBufferAsync(&uri).ok()?.get().ok()?;
    let len = buf.Length().ok()? as usize;
    if len == 0 || len > 8_000_000 {
        return None;
    }
    let r = DataReader::FromBuffer(&buf).ok()?;
    let mut v = vec![0u8; len];
    r.ReadBytes(&mut v).ok()?;
    Some(v)
}

fn urlenc(s: &str) -> String {
    let mut o = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => o.push(b as char),
            b' ' => o.push('+'),
            _ => o.push_str(&format!("%{:02X}", b)),
        }
    }
    o
}

/// Artwork from the iTunes Search API (free, no API key).
fn fetch_online(title: &str, artist: &str) -> Option<Vec<u8>> {
    let term = format!("{} {}", artist, title);
    let url = format!("https://itunes.apple.com/search?media=music&entity=song&limit=1&term={}", urlenc(&term));
    let json = String::from_utf8(http_get(&url)?).ok()?;
    let k = json.find("\"artworkUrl100\"")? + "\"artworkUrl100\"".len();
    let rest = &json[k..];
    let a = rest.find('"')? + 1;
    let rest = &rest[a..];
    let b = rest.find('"')?;
    let art = rest[..b].replace("100x100bb", "300x300bb");
    http_get(&art)
}

fn decode_art(bytes: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = img.resize_to_fill(ART, ART, image::imageops::FilterType::Triangle);
    Some(img.to_rgba8().into_raw())
}

fn post_refresh(hwnd: isize) {
    unsafe {
        let _ = PostMessageW(HWND(hwnd as *mut c_void), WM_APP, WPARAM(0), LPARAM(0));
    }
}

fn media_thread(hwnd: isize, shared: Arc<Mutex<Media>>, art_shared: Arc<Mutex<Option<ArtData>>>, rx: Receiver<Cmd>) {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let mgr = match SessionManager::RequestAsync().and_then(|a| a.get()) {
        Ok(m) => m,
        Err(_) => return,
    };
    let mut art_key = String::new();
    let mut tries = 0u32;
    let mut art_done = false;
    let mut online_started = false;
    let mut last_dur: u32 = 0;

    loop {
        if let Ok(cmd) = rx.recv_timeout(Duration::from_millis(1000)) {
            if let Ok(s) = mgr.GetCurrentSession() {
                match cmd {
                    Cmd::Prev => {
                        let _ = s.TrySkipPreviousAsync();
                    }
                    Cmd::Toggle => {
                        let _ = s.TryTogglePlayPauseAsync();
                    }
                    Cmd::Next => {
                        let _ = s.TrySkipNextAsync();
                    }
                    Cmd::Seek(f) => {
                        let ticks = (f as f64 * last_dur as f64 * 10_000_000.0) as i64;
                        let _ = s.TryChangePlaybackPositionAsync(ticks);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(250));
        }

        let mut new = Media::default();
        let mut props: Option<MediaProps> = None;
        if let Ok(s) = mgr.GetCurrentSession() {
            if let Ok(p) = s.TryGetMediaPropertiesAsync().and_then(|a| a.get()) {
                new.title = p.Title().map(|h| h.to_string()).unwrap_or_default();
                new.artist = p.Artist().map(|h| h.to_string()).unwrap_or_default();
                props = Some(p);
            }
            if let Ok(i) = s.GetPlaybackInfo() {
                if let Ok(st) = i.PlaybackStatus() {
                    new.playing = st == PlayStatus::Playing;
                }
            }
            if let Ok(tl) = s.GetTimelineProperties() {
                let pos = tl.Position().map(|t| t.Duration).unwrap_or(0);
                let start = tl.StartTime().map(|t| t.Duration).unwrap_or(0);
                let end = tl.EndTime().map(|t| t.Duration).unwrap_or(0);
                new.pos = ((pos - start).max(0) / 10_000_000) as u32;
                new.dur = ((end - start).max(0) / 10_000_000) as u32;
            }
        }
        last_dur = new.dur;

        // ---- artwork: GSMTC first, then online fallback
        let key = new.key();
        if key != art_key {
            art_key = key.clone();
            tries = 0;
            art_done = new.title.is_empty();
            online_started = false;
        }
        if !art_done && tries < 6 {
            tries += 1;
            let bytes = props.as_ref().and_then(|p| read_thumbnail(p));
            if let Some(rgba) = bytes.and_then(|b| decode_art(&b)) {
                *art_shared.lock().unwrap() = Some(ArtData { key: key.clone(), rgba });
                art_done = true;
                post_refresh(hwnd);
            } else if ONLINE_ARTWORK && tries >= 3 && !online_started {
                online_started = true;
                let (t, ar, k) = (new.title.clone(), new.artist.clone(), key.clone());
                let slot = art_shared.clone();
                std::thread::spawn(move || {
                    unsafe {
                        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                    }
                    if let Some(rgba) = fetch_online(&t, &ar).and_then(|b| decode_art(&b)) {
                        *slot.lock().unwrap() = Some(ArtData { key: k, rgba });
                        post_refresh(hwnd);
                    }
                    trim_memory();
                });
            }
        }

        let changed = {
            let mut g = shared.lock().unwrap();
            if *g != new {
                *g = new;
                true
            } else {
                false
            }
        };
        if changed {
            post_refresh(hwnd);
        }
    }
}

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

unsafe fn is_busy(own: HWND) -> bool {
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

// ---------------------------------------------------------------- animation
fn target(a: &App) -> (f32, f32) {
    if a.expanded() {
        (EXP_W, EXP_H)
    } else if a.busy {
        THIN
    } else if a.media.playing {
        PLAYING
    } else {
        IDLE
    }
}

fn settled(a: &App) -> bool {
    let (tw, th) = target(a);
    (a.w - tw).abs() < 0.3
        && (a.h - th).abs() < 0.3
        && a.vw.abs() < 2.0
        && a.vh.abs() < 2.0
        && (a.sp - a.slide as f32).abs() < 0.003
        && a.sv.abs() < 0.05
}

/// Timer: 16ms while moving, 33ms for the equalizer, off when idle.
unsafe fn schedule(a: &mut App) {
    let want = if !settled(a) {
        16
    } else if a.media.playing && !a.expanded() && !a.busy {
        33
    } else {
        0
    };
    if want != a.interval {
        let hwnd = HWND(a.hwnd as *mut c_void);
        if want == 0 {
            let _ = KillTimer(hwnd, 1);
            trim_memory();
        } else {
            SetTimer(hwnd, 1, want, None);
        }
        a.interval = want;
    }
}

fn check_reminder(a: &mut App) -> bool {
    if a.water.remind_min == 0 {
        return false;
    }
    let now = Instant::now();
    if now < a.water.next {
        return false;
    }
    a.water.reset_timer();
    let st = now_tm();
    if st.wHour < 8 || st.wHour >= 22 || a.water.ml >= a.water.goal {
        return false;
    }
    a.show(2);
    a.reminding = true;
    a.peek_until = Some(now + Duration::from_secs(7));
    true
}

/// Called every 100ms: hover, busy mode, reminders, clock, text caret.
unsafe fn poll(a: &mut App) {
    a.tick_n = a.tick_n.wrapping_add(1);
    let mut changed = false;
    let mut redraw = false;

    if !a.menu_open {
        let mut p = POINT::default();
        if GetCursorPos(&mut p).is_ok() {
            let (zw, zh) = if a.hover || a.expanded() {
                (EXP_W + 20.0, EXP_H + 8.0)
            } else if a.busy {
                (a.w.max(THIN.0) + 60.0, 10.0)
            } else {
                (a.w.max(IDLE.0) + 12.0, a.h.max(IDLE.1) + 6.0)
            };
            let x = (p.x - a.wx) as f32;
            let y = p.y as f32;
            let inside = (x - CX).abs() <= zw / 2.0 && y >= 0.0 && y <= zh;
            if inside != a.hover {
                a.hover = inside;
                changed = true;
            }
        }
    }
    if a.tick_n % 5 == 0 {
        let b = is_busy(HWND(a.hwnd as *mut c_void));
        if b != a.busy {
            a.busy = b;
            changed = true;
        }
    }
    if a.tick_n % 50 == 0 && a.water.roll() {
        redraw = true;
    }
    if a.tick_n % 150 == 0 {
        trim_memory();
    }
    if let Some(t) = a.peek_until {
        if Instant::now() >= t {
            a.peek_until = None;
            a.reminding = false;
            changed = true;
        }
    }
    if check_reminder(a) {
        changed = true;
    }

    let st = now_tm();
    let expanded = a.expanded();
    let cur = a.cur();
    if expanded && cur == 1 && st.wSecond != a.last_sec {
        redraw = true;
    }
    if expanded && cur == 3 && st.wMinute != a.last_min {
        redraw = true;
    }
    if !expanded && !a.busy && !a.media.playing && st.wMinute != a.last_min {
        redraw = true;
    }
    a.last_sec = st.wSecond;
    a.last_min = st.wMinute;

    if a.editing {
        let on = (a.edit_start.elapsed().as_millis() / 500) % 2 == 0;
        if on != a.caret_on {
            a.caret_on = on;
            redraw = true;
        }
    }

    if changed || redraw {
        present(a);
    }
    if changed {
        a.last = Instant::now();
        schedule(a);
    }
}

unsafe fn tick(id: usize) {
    with_app(|a| {
        if id == 2 {
            poll(a);
            return;
        }
        let now = Instant::now();
        let dt = (now - a.last).as_secs_f32().min(0.05);
        a.last = now;
        a.t += dt;
        let (tw, th) = target(a);
        const K: f32 = 380.0; // near critically damped: no bounce, notch style
        const C: f32 = 40.0;
        const SK: f32 = 320.0;
        const SC: f32 = 36.0;
        let n = ((dt / 0.008).ceil() as i32).max(1);
        let sdt = dt / n as f32;
        for _ in 0..n {
            a.vw += (K * (tw - a.w) - C * a.vw) * sdt;
            a.vh += (K * (th - a.h) - C * a.vh) * sdt;
            a.w += a.vw * sdt;
            a.h += a.vh * sdt;
            a.sv += (SK * (a.slide as f32 - a.sp) - SC * a.sv) * sdt;
            a.sp += a.sv * sdt;
        }
        a.w = a.w.max(20.0);
        a.h = a.h.max(2.0);
        if settled(a) {
            a.w = tw;
            a.h = th;
            a.vw = 0.0;
            a.vh = 0.0;
            a.sp = a.slide as f32;
            a.sv = 0.0;
            // keep the endless counter small
            let k = a.slide.div_euclid(N_SLIDES as i32) * N_SLIDES as i32;
            a.slide -= k;
            a.sp -= k as f32;
        }
        present(a);
        schedule(a);
    });
}

// ---------------------------------------------------------------- input
unsafe fn click(x: f32, y: f32) {
    with_app(|a| {
        if !a.ready() {
            return;
        }
        // arrow buttons
        let near = |c: (f32, f32)| ((x - c.0).powi(2) + (y - c.1).powi(2)).sqrt() < 16.0;
        if near(NAV_L) || near(NAV_R) {
            if a.editing {
                return;
            }
            a.step(if near(NAV_L) { -1 } else { 1 });
            a.last = Instant::now();
            present(a);
            schedule(a);
            return;
        }
        // dots
        if y >= EXP_H - 22.0 {
            for i in 0..N_SLIDES {
                if (x - dot_x(i)).abs() < 8.0 {
                    a.show(i);
                    a.last = Instant::now();
                    schedule(a);
                    return;
                }
            }
        }
        match a.cur() {
            0 => {
                let d = |px: f32, py: f32| ((x - px).powi(2) + (y - py).powi(2)).sqrt();
                if d(MCX - 56.0, MCY) < 17.0 {
                    let _ = a.tx.send(Cmd::Prev);
                } else if d(MCX, MCY) < 19.0 {
                    let _ = a.tx.send(Cmd::Toggle);
                } else if d(MCX + 56.0, MCY) < 17.0 {
                    let _ = a.tx.send(Cmd::Next);
                } else if a.media.dur > 0 && in_rect(x, y, MX, 88.0, MW, 16.0) {
                    let _ = a.tx.send(Cmd::Seek(((x - MX) / MW).clamp(0.0, 1.0)));
                }
            }
            2 => {
                if in_rect(x, y, BX, 42.0, 112.0, 34.0) {
                    a.water_add(250);
                } else if in_rect(x, y, BX, 84.0, 112.0, 34.0) {
                    a.water_add(500);
                }
                present(a);
            }
            3 => {
                if a.editing {
                    return;
                }
                if in_rect(x, y, LX, 116.0, 92.0, 28.0) {
                    a.begin_edit(None);
                    schedule(a);
                } else if in_rect(x, y, LX + 98.0, 116.0, 92.0, 28.0) {
                    a.edit_mode = !a.edit_mode;
                } else {
                    let (rx, rw, y0, max) = rows_geom(false);
                    let order = a.todo_order();
                    for vi in 0..max {
                        let idx = a.todo_scroll + vi;
                        if idx >= order.len() {
                            break;
                        }
                        let ri = order[idx];
                        let ry = y0 + vi as f32 * ROW_P;
                        if in_rect(x, y, rx, ry, rw, ROW_H) {
                            if a.edit_mode {
                                if x > rx + rw - 30.0 {
                                    a.todos.remove(ri);
                                    save_todos(&a.todos);
                                    a.fix_scroll();
                                } else {
                                    a.begin_edit(Some(ri));
                                    schedule(a);
                                }
                            } else {
                                a.todos[ri].0 = !a.todos[ri].0;
                                save_todos(&a.todos);
                            }
                            break;
                        }
                    }
                }
                present(a);
            }
            _ => {}
        }
    });
}

unsafe fn context_menu(hwnd: HWND, goal: u32, remind: u32) -> i32 {
    let menu = match CreatePopupMenu() {
        Ok(m) => m,
        Err(_) => return 0,
    };
    let chk = |on: bool| if on { MF_STRING | MF_CHECKED } else { MF_STRING };
    let _ = AppendMenuW(menu, MF_STRING, 11, w!("Remove 250 ml"));
    let _ = AppendMenuW(menu, MF_STRING, 10, w!("Reset today's water"));
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let goals = [
        (20usize, w!("Daily goal 1.5 L"), 1500u32),
        (21, w!("Daily goal 2.0 L"), 2000),
        (22, w!("Daily goal 2.5 L"), 2500),
        (23, w!("Daily goal 3.0 L"), 3000),
    ];
    for (id, label, v) in goals {
        let _ = AppendMenuW(menu, chk(goal == v), id, label);
    }
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let rem = [
        (30usize, w!("Remind every 30 min"), 30u32),
        (31, w!("Remind every 60 min"), 60),
        (32, w!("Remind every 90 min"), 90),
        (33, w!("Reminders off"), 0),
    ];
    for (id, label, v) in rem {
        let _ = AppendMenuW(menu, chk(remind == v), id, label);
    }
    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(menu, MF_STRING, 1, w!("Exit"));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let _ = SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, None);
    let _ = PostMessageW(hwnd, WM_NULL, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(menu);
    cmd.0
}

// ---------------------------------------------------------------- window
unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER => {
            tick(wp.0);
            LRESULT(0)
        }
        WM_APP => {
            with_app(|a| {
                let was = a.media.playing;
                a.media = a.shared.lock().unwrap().clone();
                let key = a.media.key();
                if key != a.art_key {
                    a.art = None;
                    a.art_key = key;
                    a.title_t0 = a.t;
                }
                let slot = a.art_shared.lock().unwrap().take();
                if let Some(d) = slot {
                    if d.key == a.art_key {
                        a.set_art(&d.rgba);
                    }
                }
                if a.media.playing && !was && !a.expanded() {
                    a.show(0);
                }
                a.last = Instant::now();
                present(a);
                schedule(a);
            });
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let x = (lp.0 & 0xffff) as i16 as f32;
            let y = ((lp.0 >> 16) & 0xffff) as i16 as f32;
            let _ = SetCapture(hwnd);
            with_app(|a| {
                a.drag = Some((x, y));
                a.drag_dx = 0.0;
            });
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let x = (lp.0 & 0xffff) as i16 as f32;
            with_app(|a| {
                if let Some((sx, _)) = a.drag {
                    let dx = x - sx;
                    let nd = if dx.abs() > 8.0 { dx } else { 0.0 };
                    if nd != a.drag_dx {
                        a.drag_dx = nd;
                        present(a);
                    }
                }
            });
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let _ = ReleaseCapture();
            let mut click_at: Option<(f32, f32)> = None;
            with_app(|a| {
                if let Some((sx, sy)) = a.drag.take() {
                    let dx = a.drag_dx;
                    a.drag_dx = 0.0;
                    if dx.abs() >= 40.0 && !a.editing {
                        a.sp -= dx / 160.0;
                        a.sv = 0.0;
                        a.step(if dx < 0.0 { 1 } else { -1 });
                        a.last = Instant::now();
                    } else if dx.abs() < 8.0 {
                        click_at = Some((sx, sy));
                    } else {
                        a.sp -= dx / 160.0;
                        a.sv = 0.0;
                        a.last = Instant::now();
                    }
                    present(a);
                    schedule(a);
                }
            });
            if let Some((x, y)) = click_at {
                click(x, y);
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            let d = ((wp.0 >> 16) & 0xffff) as i16 as i32;
            with_app(|a| {
                if d == 0 || !a.ready() || a.editing {
                    return;
                }
                let vertical = msg == WM_MOUSEWHEEL;
                // vertical wheel scrolls a long to-do list; otherwise it changes slide
                if vertical && a.cur() == 3 && a.todos.len() > 3 {
                    if a.last_wheel.elapsed() < Duration::from_millis(90) {
                        return;
                    }
                    a.last_wheel = Instant::now();
                    let max = a.todos.len() - 3;
                    a.todo_scroll = if d > 0 { a.todo_scroll.saturating_sub(1) } else { (a.todo_scroll + 1).min(max) };
                    present(a);
                    return;
                }
                if a.last_wheel.elapsed() < Duration::from_millis(260) {
                    return;
                }
                let dir = if vertical {
                    if d > 0 { -1 } else { 1 }
                } else if d > 0 {
                    1
                } else {
                    -1
                };
                a.last_wheel = Instant::now();
                a.step(dir);
                a.last = Instant::now();
                schedule(a);
            });
            LRESULT(0)
        }
        WM_CHAR => {
            let c = wp.0 as u32;
            with_app(|a| {
                if !a.editing {
                    return;
                }
                match c {
                    13 => a.finish_edit(true, true),
                    27 => a.finish_edit(false, true),
                    8 => {
                        a.input.pop();
                    }
                    c if c >= 32 => {
                        if let Some(ch) = char::from_u32(c) {
                            if a.input.chars().count() < 60 {
                                a.input.push(ch);
                            }
                        }
                    }
                    _ => {}
                }
                a.edit_start = Instant::now();
                a.caret_on = true;
                a.last = Instant::now();
                present(a);
                schedule(a);
            });
            LRESULT(0)
        }
        WM_KILLFOCUS => {
            with_app(|a| {
                if a.editing {
                    a.finish_edit(false, false);
                    a.last = Instant::now();
                    present(a);
                    schedule(a);
                }
            });
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let mut goal = 2000;
            let mut remind = 60;
            with_app(|a| {
                a.menu_open = true;
                goal = a.water.goal;
                remind = a.water.remind_min;
            });
            let cmd = context_menu(hwnd, goal, remind);
            with_app(|a| {
                a.menu_open = false;
                match cmd {
                    10 => {
                        a.water.ml = 0;
                        a.water.save();
                    }
                    11 => a.water_add(-250),
                    20..=23 => {
                        a.water.goal = [1500, 2000, 2500, 3000][(cmd - 20) as usize];
                        a.water.save();
                    }
                    30..=33 => {
                        a.water.remind_min = [30, 60, 90, 0][(cmd - 30) as usize];
                        a.water.reset_timer();
                        a.water.save();
                    }
                    _ => {}
                }
                a.last = Instant::now();
                present(a);
                schedule(a);
            });
            if cmd == 1 {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(3), // MA_NOACTIVATE
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

fn main() -> Result<()> {
    unsafe {
        let _ = SetProcessDPIAware();
        let hinst = GetModuleHandleW(None)?;
        let class = w!("PochWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            ..Default::default()
        };
        RegisterClassW(&wc);

        let sw = GetSystemMetrics(SM_CXSCREEN);
        let wx = (sw - W) / 2;
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("Poch"),
            WS_POPUP,
            wx,
            0, // flush with the top edge of the screen
            W,
            H,
            None,
            None,
            hinst,
            None,
        )?;

        // DIB section created once and reused every frame
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(screen);
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: W,
                biHeight: -H,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)?;
        SelectObject(mem, bmp);
        ReleaseDC(None, screen);

        let shared = Arc::new(Mutex::new(Media::default()));
        let art_shared: Arc<Mutex<Option<ArtData>>> = Arc::new(Mutex::new(None));
        let (tx, rx) = channel();
        {
            let sh = shared.clone();
            let ar = art_shared.clone();
            let h = hwnd.0 as isize;
            std::thread::spawn(move || media_thread(h, sh, ar, rx));
        }

        migrate_old_data();
        let st = now_tm();
        let mut app = App {
            hwnd: hwnd.0 as isize,
            wx,
            mem_dc: mem.0 as isize,
            bits: bits as usize,
            gfx: Some(Gfx { pm: Pixmap::new(W as u32, H as u32).unwrap(), text: Text::load(), mask: Mask::new(W as u32, H as u32) }),
            w: IDLE.0,
            h: IDLE.1,
            vw: 0.0,
            vh: 0.0,
            slide: 1,
            sp: 1.0,
            sv: 0.0,
            drag: None,
            drag_dx: 0.0,
            last_wheel: Instant::now(),
            t: 0.0,
            last: Instant::now(),
            interval: 0,
            tick_n: 0,
            last_sec: st.wSecond,
            last_min: st.wMinute,
            hover: false,
            busy: is_busy(hwnd),
            menu_open: false,
            peek_until: None,
            reminding: false,
            shared,
            art_shared,
            media: Media::default(),
            art: None,
            icon: load_icon(),
            art_key: String::new(),
            title_t0: 0.0,
            todos: load_todos(),
            todo_scroll: 0,
            edit_mode: false,
            editing: false,
            edit_idx: None,
            input: String::new(),
            caret_on: true,
            edit_start: Instant::now(),
            prev_fg: 0,
            water: Water::load(),
            tx,
        };
        let (tw, th) = target(&app);
        app.w = tw;
        app.h = th;
        present(&mut app);
        *APP.lock().unwrap() = Some(app);

        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(hwnd, 2, 100, None); // poll hover / busy / reminders / clock
        trim_memory();

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}