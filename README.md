<div align="center">

<img src="icon.ico" alt="Poch" width="96" height="96">

# Poch

**A tiny notch for the top of your Windows screen. Music, clock, water, to-do, file shelf and notes, all in one place.**

![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011-0078D4?logo=windows&logoColor=white)
![Language](https://img.shields.io/badge/built%20with-Rust-DEA584?logo=rust&logoColor=black)
![License](https://img.shields.io/badge/license-MIT-green)

</div>

---

## <img src="https://api.iconify.design/lucide/info.svg?color=%23ff453a" width="20" align="top"> About

Poch sits flush against the top edge of your screen as a small black notch. Hover over it and it expands into a carousel of mini apps. Move away and it shrinks back down. It is a single, lightweight `.exe` with no runtime to install.

- Layered per-pixel alpha window, software rendered with `tiny-skia`
- Small memory footprint: embedded fonts, bounded glyph cache, working set trimmed when idle
- Your data stays local in `%APPDATA%\poch`

---

## <img src="https://api.iconify.design/lucide/layout-grid.svg?color=%23ff453a" width="20" align="top"> Slides

Swipe, scroll, drag, use the arrow buttons or click the dots to move through an endless carousel.

|                                           Icon                                            | Slide     | What it does                                                                                                                                                      |
| :---------------------------------------------------------------------------------------: | --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
|    <img src="https://api.iconify.design/lucide/music.svg?color=%23ff453a" width="22">     | **Music** | Now Playing from Spotify, browsers and other media apps, with artwork, progress bar and playback controls. The clock and date sit on the right of the same slide. |
|   <img src="https://api.iconify.design/lucide/droplets.svg?color=%23ff453a" width="22">   | **Water** | Daily water tracker with quick add buttons, a daily goal (1.5 to 3.0 L) and optional reminders.                                                                   |
| <img src="https://api.iconify.design/lucide/list-checks.svg?color=%23ff453a" width="22">  | **To-do** | Simple checklist with add, edit and delete. Scrolls when the list gets long.                                                                                      |
| <img src="https://api.iconify.design/lucide/folder-open.svg?color=%23ff453a" width="22">  | **Ledge** | A shelf for up to 5 files or folders. Drop them in, click to open, drag them back out (copy by default, hold `Shift` to move).                                    |
| <img src="https://api.iconify.design/lucide/notebook-pen.svg?color=%23ff453a" width="22"> | **Scrap** | Mini notepad, up to 3,333 characters, with caret, selection, clipboard and a live character counter.                                                              |

---

## <img src="https://api.iconify.design/lucide/sparkles.svg?color=%23ff453a" width="20" align="top"> Features

|                                          Icon                                           | Feature        | Details                                                                                                             |
| :-------------------------------------------------------------------------------------: | -------------- | ------------------------------------------------------------------------------------------------------------------- |
|   <img src="https://api.iconify.design/lucide/music.svg?color=%23ff453a" width="22">    | Media control  | Reads Windows media sessions (GSMTC). Artwork comes from the session, with a fallback to the iTunes Search API.     |
|   <img src="https://api.iconify.design/lucide/clock.svg?color=%23ff453a" width="22">    | Idle clock     | When nothing is playing, the notch shows the time, then switches to a blinking eye with a red glow, then loops.     |
| <img src="https://api.iconify.design/lucide/minimize-2.svg?color=%23ff453a" width="22"> | Busy mode      | With a maximized or fullscreen window (or Adobe apps in front), the notch shrinks to a thin line. Hover to open it. |
|  <img src="https://api.iconify.design/lucide/keyboard.svg?color=%23ff453a" width="22">  | Hotkey         | `Ctrl` + `Alt` + `H` toggles between the clock and the eye.                                                         |
|   <img src="https://api.iconify.design/lucide/waves.svg?color=%23ff453a" width="22">    | Drop animation | Dropping a file onto the notch makes its bottom edge ripple like water.                                             |
|  <img src="https://api.iconify.design/lucide/palette.svg?color=%23ff453a" width="22">   | Artwork tint   | The Music slide picks up colors from the album cover for a soft glow.                                               |

---

## <img src="https://api.iconify.design/lucide/download.svg?color=%23ff453a" width="20" align="top"> Install

| Requirement                             | Version                                                           |
| --------------------------------------- | ----------------------------------------------------------------- |
| Operating system                        | Windows 10 or Windows 11                                          |
| Build tools (only to build from source) | [Rust](https://www.rust-lang.org/tools/install) (stable)          |
| Installer (optional)                    | [Inno Setup](https://jrsoftware.org/isinfo.php), using `poch.iss` |

---

## <img src="https://api.iconify.design/lucide/hammer.svg?color=%23ff453a" width="20" align="top"> Build from source

```powershell
git clone <your-repo-url>
cd poch
cargo build --release
```

The executable is created at `target\release\poch.exe`. To make a setup installer, open `poch.iss` in Inno Setup and compile it.

---

## <img src="https://api.iconify.design/lucide/folder-tree.svg?color=%23ff453a" width="20" align="top"> Project layout

| File                                         | Purpose                                                            |
| -------------------------------------------- | ------------------------------------------------------------------ |
| `main.rs`                                    | App state, window creation, message loop                           |
| `window.rs`                                  | Win32 window procedure (mouse, keyboard, timer, hotkey, file drop) |
| `render.rs`, `gfx.rs`                        | Frame drawing, notch shape, text and shape helpers                 |
| `anim.rs`                                    | Spring animation, timers, hover, busy and media polling            |
| `music.rs`                                   | Media thread, artwork and artwork tint, Music slide                |
| `clock.rs`                                   | Clock and date helpers                                             |
| `water.rs`, `todo.rs`, `shelf.rs`, `note.rs` | Water, To-do, Ledge and Scrap slides                               |
| `dragdrop.rs`                                | OLE drag out of the Ledge                                          |
| `eye.rs`, `busy.rs`, `icon.rs`               | Eye animation, busy detection, app icons                           |
| `nav.rs`, `input.rs`, `config.rs`, `data.rs` | Carousel, clicks and menu, shared sizes, data storage              |

---

## <img src="https://api.iconify.design/lucide/database.svg?color=%23ff453a" width="20" align="top"> Your data

Everything is saved as plain text in `%APPDATA%\poch`:

| File          | Content                                  |
| ------------- | ---------------------------------------- |
| `todo.txt`    | To-do items                              |
| `water.txt`   | Water intake, goal and reminder settings |
| `shelf.txt`   | Files on the Ledge                       |
| `notepad.txt` | Scrap note                               |

Right click the notch for water options and to exit the app.

---

## <img src="https://api.iconify.design/lucide/scale.svg?color=%23ff453a" width="20" align="top"> License

Released under the [MIT License](LICENSE).

You are free to use, modify, fork and redistribute Poch, **as long as you keep the copyright notice and credit**:

> Copyright (c) 2026 **Commonly Works**

Any copy or fork, whether source or compiled, must include the [LICENSE](LICENSE) file with this credit.

The embedded font is **Instrument Sans**, used under its own open font license.

---

<div align="center">

Made by **Commonly Works**

</div>
