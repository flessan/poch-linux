//! Native Linux backend for Poch.
//! The UI and persistence are intentionally independent from the Win32 backend.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::Local;
use fontdue::{Font, FontSettings};
use pixels::{Pixels, SurfaceTexture};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, Event, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowLevel},
};

const IDLE_W: u32 = 264;
const IDLE_H: u32 = 48;
const OPEN_W: u32 = 600;
const OPEN_H: u32 = 244;
const ACCENT: (u8, u8, u8, u8) = (255, 69, 58, 255);
const WHITE: (u8, u8, u8, u8) = (246, 246, 248, 255);
const MUTED: (u8, u8, u8, u8) = (158, 161, 170, 255);
const FONT_REGULAR: &[u8] = include_bytes!("../assets/InstrumentSans-Regular.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../assets/InstrumentSans-Bold.ttf");

#[derive(Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Task,
    Shelf,
    Note,
}

struct App {
    window: Arc<Window>,
    pixels: Pixels<'static>,
    font: Font,
    font_bold: Font,
    expanded: bool,
    // Delay collapsing after a pointer-leave event. Wayland compositors may
    // emit pointer events while applying a client-requested resize.
    collapse_deadline: Option<Instant>,
    slide: usize,
    cursor: (f64, f64),
    water_ml: u32,
    water_goal: u32,
    todos: Vec<(bool, String)>,
    shelf: Vec<String>,
    todo_scroll: usize,
    shelf_scroll: usize,
    note: String,
    input: String,
    input_mode: Option<InputMode>,
    media_title: String,
    media_artist: String,
    media_playing: bool,
    last_refresh: Instant,
    data_dir: PathBuf,
    status: String,
}

pub(crate) fn run() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(1)));

    let window = Arc::new(
        event_loop.create_window(
            Window::default_attributes()
                .with_title("Poch")
                .with_decorations(false)
                .with_transparent(true)
                // This window changes size programmatically when expanded.
                // Marking it non-resizable can cause Wayland compositors to
                // ignore those size requests.
                .with_resizable(true)
                .with_window_level(WindowLevel::AlwaysOnTop)
                .with_inner_size(PhysicalSize::new(IDLE_W, IDLE_H)),
        )?,
    );
    center_window(&window, IDLE_W);
    let size = window.inner_size();
    let surface = SurfaceTexture::new(size.width.max(1), size.height.max(1), Arc::clone(&window));
    let pixels = Pixels::new(size.width.max(1), size.height.max(1), surface)?;

    let data_dir = data_dir();
    let _ = fs::create_dir_all(&data_dir);
    let (water_ml, water_goal) = load_water(&data_dir);
    let mut app = App {
        window,
        pixels,
        font: Font::from_bytes(FONT_REGULAR, FontSettings::default())
            .map_err(|e| format!("Could not load Instrument Sans regular: {e:?}"))?,
        font_bold: Font::from_bytes(FONT_BOLD, FontSettings::default())
            .map_err(|e| format!("Could not load Instrument Sans bold: {e:?}"))?,
        expanded: false,
        collapse_deadline: None,
        slide: 0,
        cursor: (0.0, 0.0),
        water_ml,
        water_goal,
        todos: load_todos(&data_dir),
        shelf: load_shelf(&data_dir),
        todo_scroll: 0,
        shelf_scroll: 0,
        note: fs::read_to_string(data_dir.join("notepad.txt")).unwrap_or_default(),
        input: String::new(),
        input_mode: None,
        media_title: String::new(),
        media_artist: String::new(),
        media_playing: false,
        last_refresh: Instant::now() - Duration::from_secs(5),
        data_dir,
        status: String::new(),
    };
    app.refresh_media();
    app.window.request_redraw();

    event_loop.run(move |event, target| {
        match event {
            Event::WindowEvent { window_id, event } if window_id == app.window.id() => match event {
                WindowEvent::CloseRequested => target.exit(),
                WindowEvent::Resized(size) => {
                    let width = size.width.max(1);
                    let height = size.height.max(1);
                    let _ = app.pixels.resize_surface(width, height);
                    let _ = app.pixels.resize_buffer(width, height);
                    app.window.request_redraw();
                }
                WindowEvent::CursorEntered { .. } => {
                    // Cancel a pending collapse if the pointer returns while
                    // the compositor is still applying a resize/configure.
                    app.collapse_deadline = None;
                    if !app.expanded {
                        app.expanded = true;
                        resize_window(&app.window, true);
                        app.window.request_redraw();
                    }
                }
                WindowEvent::CursorLeft { .. } => {
                    if app.expanded && app.input_mode.is_none() {
                        // Avoid flicker if resizing causes a transient leave
                        // event on Wayland.
                        app.collapse_deadline =
                            Some(Instant::now() + Duration::from_millis(350));
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    app.cursor = (position.x, position.y);
                    app.window.request_redraw();
                }
                WindowEvent::MouseWheel { delta, .. } if app.expanded => {
                    let direction = match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(p) => p.y as f32,
                    };
                    if direction.abs() > 0.0 && app.input_mode.is_none() {
                        let moving_down = direction < 0.0;
                        if app.slide == 2 && app.todos.len() > 4 {
                            let max = app.todos.len() - 4;
                            app.todo_scroll = if moving_down {
                                (app.todo_scroll + 1).min(max)
                            } else {
                                app.todo_scroll.saturating_sub(1)
                            };
                        } else if app.slide == 3 && app.shelf.len() > 4 {
                            let max = app.shelf.len() - 4;
                            app.shelf_scroll = if moving_down {
                                (app.shelf_scroll + 1).min(max)
                            } else {
                                app.shelf_scroll.saturating_sub(1)
                            };
                        } else {
                            app.slide = if moving_down {
                                (app.slide + 1) % 5
                            } else {
                                (app.slide + 4) % 5
                            };
                        }
                        app.window.request_redraw();
                    }
                }
                WindowEvent::MouseInput {
                    state: ElementState::Pressed,
                    button: MouseButton::Left,
                    ..
                } if app.expanded => {
                    handle_click(&mut app);
                    app.window.request_redraw();
                }
                WindowEvent::KeyboardInput { event, .. } if app.input_mode.is_some() => {
                    if event.state == ElementState::Pressed {
                        match &event.logical_key {
                            Key::Named(NamedKey::Enter) => finish_input(&mut app, true),
                            Key::Named(NamedKey::Escape) => finish_input(&mut app, false),
                            Key::Named(NamedKey::Backspace) => {
                                app.input.pop();
                            }
                            _ => {
                                if let Some(text) = event.text.as_deref() {
                                    if !text.chars().any(char::is_control) {
                                        let limit = if app.input_mode == Some(InputMode::Note) {
                                            3333
                                        } else {
                                            240
                                        };
                                        if app.input.chars().count() < limit {
                                            app.input.push_str(text);
                                        }
                                    }
                                }
                            }
                        }
                        app.window.request_redraw();
                    }
                }
                WindowEvent::KeyboardInput { event, .. }
                    if event.state == ElementState::Pressed
                        && event.logical_key == Key::Named(NamedKey::Escape) =>
                {
                    target.exit();
                }
                WindowEvent::RedrawRequested => draw(&mut app),
                _ => {}
            },
            Event::AboutToWait => {
                let now = Instant::now();
                if app.collapse_deadline.is_some_and(|deadline| now >= deadline) {
                    app.collapse_deadline = None;
                    if app.expanded && app.input_mode.is_none() {
                        app.expanded = false;
                        resize_window(&app.window, false);
                        app.window.request_redraw();
                    }
                }

                if app.last_refresh.elapsed() >= Duration::from_secs(2) {
                    app.refresh_media();
                }
                app.window.request_redraw();

                // Wake for either the regular UI refresh or the pending
                // collapse, whichever comes first.
                let next_refresh = Instant::now() + Duration::from_secs(1);
                let next_wake = app
                    .collapse_deadline
                    .map_or(next_refresh, |deadline| deadline.min(next_refresh));
                target.set_control_flow(ControlFlow::WaitUntil(next_wake));
            }
            _ => {}
        }
    })?;
    Ok(())
}

impl App {
    fn refresh_media(&mut self) {
        self.last_refresh = Instant::now();
        let metadata = Command::new("playerctl")
            .args(["metadata", "--format", "{{title}}\n{{artist}}"])
            .output();
        if let Ok(output) = metadata {
            if output.status.success() {
                let value = String::from_utf8_lossy(&output.stdout);
                let mut lines = value.lines();
                self.media_title = lines.next().unwrap_or_default().trim().to_owned();
                self.media_artist = lines.next().unwrap_or_default().trim().to_owned();
                self.status.clear();
            } else {
                self.media_title.clear();
                self.media_artist.clear();
            }
        } else {
            self.media_title.clear();
            self.media_artist.clear();
        }
        self.media_playing = Command::new("playerctl")
            .args(["status"])
            .output()
            .map(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == "Playing")
            .unwrap_or(false);
        if self.media_title.is_empty() {
            self.status = "Install playerctl for MPRIS media controls".to_owned();
        }
    }

    fn save_water(&self) {
        let today = Local::now().format("%Y-%m-%d");
        let content = format!("{today};{};{}\n", self.water_ml, self.water_goal);
        let _ = fs::write(self.data_dir.join("water.txt"), content);
    }

    fn save_todos(&self) {
        let content = self
            .todos
            .iter()
            .map(|(done, text)| format!("{}\t{}\n", if *done { "1" } else { "0" }, text))
            .collect::<String>();
        let _ = fs::write(self.data_dir.join("todo.txt"), content);
    }

    fn save_shelf(&self) {
        let content = self.shelf.join("\n");
        let _ = fs::write(self.data_dir.join("shelf.txt"), content);
    }
}

fn data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(path).join("poch");
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/share/poch")
}

fn load_water(dir: &Path) -> (u32, u32) {
    let raw = fs::read_to_string(dir.join("water.txt")).unwrap_or_default();
    let mut parts = raw.trim().split(';');
    let today = Local::now().format("%Y-%m-%d").to_string();
    if parts.next() == Some(today.as_str()) {
        let amount = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
        let goal = parts.next().and_then(|v| v.parse().ok()).unwrap_or(2000);
        (amount, goal.clamp(1500, 3000))
    } else {
        (0, 2000)
    }
}

fn load_todos(dir: &Path) -> Vec<(bool, String)> {
    fs::read_to_string(dir.join("todo.txt"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (done, text) = line.split_once('\t')?;
            Some((done == "1", text.to_owned()))
        })
        .take(100)
        .collect()
}

fn load_shelf(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join("shelf.txt"))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(5)
        .map(str::to_owned)
        .collect()
}

fn center_window(window: &Window, width: u32) {
    if let Some(monitor) = window.current_monitor() {
        let screen = monitor.size();
        window.set_outer_position(PhysicalPosition::new(
            (screen.width.saturating_sub(width) / 2) as i32,
            0,
        ));
    }
}

fn resize_window(window: &Window, expanded: bool) {
    let (width, height) = if expanded {
        (OPEN_W, OPEN_H)
    } else {
        (IDLE_W, IDLE_H)
    };
    let _ = window.request_inner_size(PhysicalSize::new(width, height));
    center_window(window, width);
}

fn finish_input(app: &mut App, commit: bool) {
    if commit {
        match app.input_mode {
            Some(InputMode::Task) if !app.input.trim().is_empty() => {
                app.todos.push((false, app.input.trim().to_owned()));
                app.save_todos();
            }
            Some(InputMode::Shelf) if !app.input.trim().is_empty() => {
                let path = app.input.trim().to_owned();
                if app.shelf.len() < 5 && !app.shelf.contains(&path) {
                    app.shelf.push(path);
                    app.save_shelf();
                }
            }
            Some(InputMode::Note) => {
                app.note = app.input.chars().take(3333).collect();
                let _ = fs::write(app.data_dir.join("notepad.txt"), &app.note);
            }
            _ => {}
        }
    }
    app.input.clear();
    app.input_mode = None;
    app.expanded = true;
    resize_window(&app.window, true);
    app.window.request_redraw();
}

fn handle_click(app: &mut App) {
    let (x, y) = app.cursor;
    if y < 44.0 {
        for i in 0..5 {
            let left = 128.0 + i as f64 * 88.0;
            if x >= left && x <= left + 80.0 {
                app.slide = i;
                return;
            }
        }
    }

    match app.slide {
        0 => {
            if y > 140.0 && y < 190.0 {
                let action = if x < 245.0 {
                    "previous"
                } else if x < 350.0 {
                    "play-pause"
                } else {
                    "next"
                };
                let _ = Command::new("playerctl").arg(action).status();
                app.refresh_media();
            }
        }
        1 => {
            if y >= 154.0 && y <= 202.0 {
                if x < 155.0 {
                    app.water_ml = app.water_ml.saturating_add(250);
                } else if x < 290.0 {
                    app.water_ml = app.water_ml.saturating_add(500);
                } else if x > 425.0 {
                    app.water_ml = 0;
                } else {
                    app.water_goal = match app.water_goal {
                        1500 => 2000,
                        2000 => 2500,
                        2500 => 3000,
                        _ => 1500,
                    };
                }
                app.save_water();
            }
        }
        2 => {
            if y >= 70.0 && y <= 103.0 && x > 440.0 {
                app.input_mode = Some(InputMode::Task);
                app.input.clear();
            } else if y >= 102.0 && y < 200.0 {
                let index = app.todo_scroll + ((y - 102.0) / 24.0) as usize;
                if let Some(item) = app.todos.get_mut(index) {
                    item.0 = !item.0;
                    app.save_todos();
                }
            }
        }
        3 => {
            if y >= 70.0 && y <= 103.0 && x > 440.0 {
                app.input_mode = Some(InputMode::Shelf);
                app.input.clear();
            } else if y >= 102.0 && y < 200.0 {
                let index = app.shelf_scroll + ((y - 102.0) / 26.0) as usize;
                if let Some(path) = app.shelf.get(index).cloned() {
                    if Path::new(&path).exists() {
                        let _ = Command::new("xdg-open").arg(path).spawn();
                    } else {
                        app.status = "That path no longer exists".to_owned();
                    }
                }
            }
        }
        4 => {
            app.input_mode = Some(InputMode::Note);
            app.input = app.note.clone();
        }
        _ => {}
    }
}

fn draw(app: &mut App) {
    let size = app.window.inner_size();
    let width = size.width.max(1);
    let height = size.height.max(1);
    let Some(mut pixmap) = Pixmap::new(width, height) else {
        return;
    };
    pixmap.fill(Color::from_rgba8(0, 0, 0, 0));

    if !app.expanded {
        round_rect(&mut pixmap, 4.0, 2.0, width as f32 - 8.0, height as f32 - 4.0, 22.0, (8, 9, 11, 248));
        draw_text(&mut pixmap, &app.font_bold, "Poch", 20.0, 13.0, 18.0, WHITE);
        draw_text(
            &mut pixmap,
            &app.font,
            &Local::now().format("%H:%M").to_string(),
            width as f32 - 65.0,
            15.0,
            16.0,
            WHITE,
        );
    } else {
        round_rect(&mut pixmap, 1.0, 1.0, width as f32 - 2.0, height as f32 - 2.0, 24.0, (12, 13, 16, 248));
        draw_text(&mut pixmap, &app.font_bold, "POCH", 22.0, 13.0, 15.0, ACCENT);

        let tabs = ["MUSIC", "WATER", "TO-DO", "LEDGE", "SCRAP"];
        for (i, tab) in tabs.iter().enumerate() {
            let x = 128.0 + i as f32 * 88.0;
            round_rect(
                &mut pixmap,
                x,
                9.0,
                80.0,
                30.0,
                12.0,
                if app.slide == i { ACCENT } else { (35, 37, 42, 255) },
            );
            draw_text(
                &mut pixmap,
                &app.font_bold,
                tab,
                x + 14.0,
                17.0,
                10.0,
                if app.slide == i { WHITE } else { MUTED },
            );
        }
        match app.slide {
            0 => draw_music(&mut pixmap, app),
            1 => draw_water(&mut pixmap, app),
            2 => draw_todos(&mut pixmap, app),
            3 => draw_shelf(&mut pixmap, app),
            4 => draw_note(&mut pixmap, app),
            _ => {}
        }
        draw_text(&mut pixmap, &app.font, "Scroll to switch slides  |  Esc to quit", 22.0, height as f32 - 23.0, 11.0, MUTED);
    }

    let frame = app.pixels.frame_mut();
    if frame.len() == pixmap.data().len() {
        frame.copy_from_slice(pixmap.data());
        let _ = app.pixels.render();
    }
}

fn draw_music(pm: &mut Pixmap, app: &App) {
    draw_text(pm, &app.font, "NOW PLAYING", 28.0, 61.0, 10.0, MUTED);
    let title = if app.media_title.is_empty() { "Nothing is playing" } else { &app.media_title };
    draw_text(pm, &app.font_bold, title, 28.0, 83.0, 21.0, WHITE);
    let artist = if app.media_artist.is_empty() {
        "Spotify, browsers and other MPRIS players"
    } else {
        &app.media_artist
    };
    draw_text(pm, &app.font, artist, 28.0, 112.0, 13.0, MUTED);
    let state = if app.media_playing { "PLAYING" } else { "PAUSED" };
    draw_text(pm, &app.font_bold, state, 28.0, 142.0, 10.0, ACCENT);
    for (x, label) in [(185.0, "PREV"), (270.0, if app.media_playing { "PAUSE" } else { "PLAY" }), (355.0, "NEXT")] {
        round_rect(pm, x, 153.0, 74.0, 34.0, 14.0, (40, 42, 47, 255));
        draw_text(pm, &app.font_bold, label, x + 17.0, 164.0, 11.0, WHITE);
    }
    if !app.status.is_empty() {
        draw_text(pm, &app.font, &app.status, 28.0, 203.0, 10.0, MUTED);
    } else {
        draw_text(pm, &app.font, &Local::now().format("%A, %d %B").to_string(), 414.0, 203.0, 11.0, MUTED);
    }
}

fn draw_water(pm: &mut Pixmap, app: &App) {
    draw_text(pm, &app.font, "DAILY WATER", 28.0, 62.0, 10.0, MUTED);
    draw_text(pm, &app.font_bold, &format!("{:.2} L", app.water_ml as f32 / 1000.0), 28.0, 83.0, 27.0, WHITE);
    draw_text(pm, &app.font, &format!("Goal {:.1} L", app.water_goal as f32 / 1000.0), 170.0, 92.0, 13.0, MUTED);
    round_rect(pm, 28.0, 119.0, 544.0, 8.0, 4.0, (48, 50, 56, 255));
    round_rect(
        pm,
        28.0,
        119.0,
        544.0 * (app.water_ml as f32 / app.water_goal.max(1) as f32).clamp(0.0, 1.0),
        8.0,
        4.0,
        ACCENT,
    );
    for (x, label) in [(28.0, "+250 ml"), (165.0, "+500 ml"), (302.0, "Goal"), (439.0, "Reset")] {
        round_rect(pm, x, 154.0, 122.0, 39.0, 14.0, (39, 41, 46, 255));
        draw_text(pm, &app.font_bold, label, x + 24.0, 168.0, 12.0, WHITE);
    }
    draw_text(pm, &app.font, "Right-click options are not available in the Linux build yet.", 28.0, 202.0, 10.0, MUTED);
}

fn draw_todos(pm: &mut Pixmap, app: &App) {
    draw_text(pm, &app.font, "YOUR TASKS", 28.0, 63.0, 10.0, MUTED);
    round_rect(pm, 448.0, 57.0, 122.0, 31.0, 12.0, ACCENT);
    draw_text(pm, &app.font_bold, "+ ADD TASK", 466.0, 66.0, 11.0, WHITE);
    if app.todos.is_empty() {
        draw_text(pm, &app.font, "Nothing here yet. Add a task to get started.", 29.0, 119.0, 14.0, MUTED);
    }
    for (i, (done, item)) in app.todos.iter().skip(app.todo_scroll).take(4).enumerate() {
        let y = 102.0 + i as f32 * 24.0;
        round_rect(pm, 28.0, y, 544.0, 21.0, 7.0, (31, 33, 38, 255));
        draw_text(pm, &app.font_bold, if *done { "x" } else { " " }, 39.0, y + 4.0, 12.0, ACCENT);
        draw_text(pm, &app.font, item, 61.0, y + 4.0, 12.0, if *done { MUTED } else { WHITE });
    }
    if app.input_mode == Some(InputMode::Task) {
        input_bar(pm, app, "Task name, then Enter to save");
    }
}

fn draw_shelf(pm: &mut Pixmap, app: &App) {
    draw_text(pm, &app.font, "FILES & FOLDERS", 28.0, 63.0, 10.0, MUTED);
    round_rect(pm, 448.0, 57.0, 122.0, 31.0, 12.0, ACCENT);
    draw_text(pm, &app.font_bold, "+ ADD PATH", 466.0, 66.0, 11.0, WHITE);
    if app.shelf.is_empty() {
        draw_text(pm, &app.font, "Save a file or folder path here for quick access.", 29.0, 119.0, 14.0, MUTED);
    }
    for (i, path) in app.shelf.iter().skip(app.shelf_scroll).take(4).enumerate() {
        let y = 102.0 + i as f32 * 26.0;
        round_rect(pm, 28.0, y, 544.0, 23.0, 7.0, (31, 33, 38, 255));
        let label = Path::new(path).file_name().and_then(|s| s.to_str()).unwrap_or(path);
        draw_text(pm, &app.font, label, 40.0, y + 5.0, 12.0, WHITE);
    }
    if app.input_mode == Some(InputMode::Shelf) {
        input_bar(pm, app, "Absolute path, then Enter to save");
    }
}

fn draw_note(pm: &mut Pixmap, app: &App) {
    draw_text(pm, &app.font, "SCRAP NOTE", 28.0, 63.0, 10.0, MUTED);
    round_rect(pm, 28.0, 83.0, 544.0, 116.0, 13.0, (31, 33, 38, 255));
    let value = if app.input_mode == Some(InputMode::Note) { &app.input } else { &app.note };
    if value.trim().is_empty() {
        draw_text(pm, &app.font, "Click here to write a note.", 44.0, 101.0, 14.0, MUTED);
    } else {
        for (i, line) in value.lines().take(5).enumerate() {
            draw_text(pm, &app.font, line, 44.0, 100.0 + i as f32 * 18.0, 12.0, WHITE);
        }
    }
    if app.input_mode == Some(InputMode::Note) {
        draw_text(pm, &app.font, "Enter saves  |  Esc cancels", 28.0, 207.0, 10.0, ACCENT);
    } else {
        draw_text(pm, &app.font, "Up to 3,333 characters. Stored locally.", 28.0, 207.0, 10.0, MUTED);
    }
}

fn input_bar(pm: &mut Pixmap, app: &App, hint: &str) {
    round_rect(pm, 28.0, 172.0, 544.0, 31.0, 10.0, (50, 52, 58, 255));
    let display = if app.input.is_empty() { hint } else { &app.input };
    draw_text(pm, &app.font, display, 40.0, 181.0, 11.0, if app.input.is_empty() { MUTED } else { WHITE });
    draw_text(pm, &app.font, "Enter: save", 480.0, 207.0, 10.0, ACCENT);
}

fn round_rect(
    pm: &mut Pixmap,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    color: (u8, u8, u8, u8),
) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let r = radius.min(w / 2.0).min(h / 2.0);
    let mut path = PathBuilder::new();
    path.move_to(x + r, y);
    path.line_to(x + w - r, y);
    path.quad_to(x + w, y, x + w, y + r);
    path.line_to(x + w, y + h - r);
    path.quad_to(x + w, y + h, x + w - r, y + h);
    path.line_to(x + r, y + h);
    path.quad_to(x, y + h, x, y + h - r);
    path.line_to(x, y + r);
    path.quad_to(x, y, x + r, y);
    path.close();
    if let Some(path) = path.finish() {
        let mut paint = Paint::default();
        paint.set_color(Color::from_rgba8(color.0, color.1, color.2, color.3));
        paint.anti_alias = true;
        pm.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
}

fn draw_text(
    pm: &mut Pixmap,
    font: &Font,
    value: &str,
    x: f32,
    y: f32,
    size: f32,
    color: (u8, u8, u8, u8),
) {
    let mut pen_x = x;
    for ch in value.chars() {
        if ch == '\n' {
            break;
        }
        let (metrics, bitmap) = font.rasterize(ch, size);
        let gx = pen_x.round() as i32 + metrics.xmin;
        let gy = (y + size * 0.85).round() as i32 - metrics.height as i32 - metrics.ymin;
        let width = pm.width() as i32;
        let height = pm.height() as i32;
        let data = pm.data_mut();
        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let px = gx + col as i32;
                let py = gy + row as i32;
                if px < 0 || py < 0 || px >= width || py >= height {
                    continue;
                }
                let coverage = bitmap[row * metrics.width + col] as f32 / 255.0;
                let alpha = coverage * color.3 as f32 / 255.0;
                if alpha <= 0.0 {
                    continue;
                }
                let index = ((py * width + px) * 4) as usize;
                for (channel, source) in [color.0, color.1, color.2].iter().enumerate() {
                    data[index + channel] =
                        (*source as f32 * alpha + data[index + channel] as f32 * (1.0 - alpha)) as u8;
                }
                data[index + 3] =
                    (255.0 * alpha + data[index + 3] as f32 * (1.0 - alpha)) as u8;
            }
        }
        pen_x += metrics.advance_width;
    }
}
