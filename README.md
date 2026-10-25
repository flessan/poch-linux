<div align="center">

<img src="icon.ico" alt="Poch" width="96" height="96">

# Poch

**A tiny desktop notch for music, clock, water tracking, to-do, a file shelf and notes. Windows is the mature build; Linux support is an early native port.**

![Platform](https://img.shields.io/badge/platform-Windows%2010%2F11%20%7C%20Linux%20(early)-0078D4?logo=linux&logoColor=white)
![Language](https://img.shields.io/badge/built%20with-Rust-DEA584?logo=rust&logoColor=black)
![License](https://img.shields.io/badge/license-MIT-green)

</div>

---

## <img src="https://api.iconify.design/lucide/info.svg?color=%23ff453a" width="20" align="top"> About

On Windows, Poch sits flush against the top edge of your screen as a small black notch. Hover over it and it expands into a carousel of mini apps, then shrinks back down when you move away. The Windows build is a single `.exe`. Linux has a separate native backend and early support for the same core ideas.

- Layered per-pixel alpha window, software rendered with `tiny-skia`
- Small memory footprint: embedded fonts, bounded glyph cache, working set trimmed when idle
- Your data stays local in `%APPDATA%\poch`

---

## <img src="https://api.iconify.design/lucide/layout-grid.svg?color=%23ff453a" width="20" align="top"> Slides

The feature table below describes the full Windows build. Linux support and current gaps are listed in the Install section.

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

The details below refer to Windows unless explicitly stated otherwise.

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
| Operating system                        | Windows 10/11, or Linux x86_64 (early support)                    |
| Build tools (only to build from source) | [Rust](https://www.rust-lang.org/tools/install) (stable)          |
| Installer (optional)                    | [Inno Setup](https://jrsoftware.org/isinfo.php), using `poch.iss` |

Linux packages are built automatically by [the Linux workflow](.github/workflows/linux.yml). Available formats are:

| Format | Use |
| --- | --- |
| `.AppImage` | Portable app bundle |
| `.tar.gz` | Standalone executable, desktop entry, icon and Linux instructions |
| `.deb` | Debian and Ubuntu-based distributions |\n| `.rpm` | Fedora, RHEL-compatible and openSUSE-family distributions |

Linux is an early port. Clock, MPRIS media controls through optional `playerctl`, water tracking, tasks, file-path shelf and notes are implemented. Windows-only busy detection, global hotkey, drag-out support, media artwork and some animations are not ported yet. Compositor behavior for transparent, always-on-top windows and positioning varies, especially on Wayland. See [Linux usage and limitations](packaging/linux/README.md).

---

## <img src="https://api.iconify.design/lucide/hammer.svg?color=%23ff453a" width="20" align="top"> Build from source

```powershell
git clone https://github.com/flessan/poch-linux.git
cd poch-linux
cargo build --release
```

On Windows, the executable is created at `target\release\poch.exe`. To make a setup installer, open `poch.iss` in Inno Setup and compile it.

On Linux, Cargo automatically selects the native Linux backend:

```sh
git clone https://github.com/flessan/poch-linux.git
cd poch-linux
cargo build --release
sudo apt install imagemagick dpkg-dev
bash scripts/package-linux.sh
```

The script creates the `.tar.gz`, `.deb`, and `.rpm` packages plus the AppImage staging directory. The workflow also runs `linuxdeploy` and `appimagetool` to generate the final AppImage.

---

## <img src="https://api.iconify.design/lucide/folder-tree.svg?color=%23ff453a" width="20" align="top"> Project layout

| File                                         | Purpose                                                            |
| -------------------------------------------- | ------------------------------------------------------------------ |
| `main.rs`                                    | Dispatches to the platform-specific backend                        |
| `windows_main.rs`                             | Original Win32 application and message loop                        |
| `linux_main.rs`                               | Native Linux window, rendering and core interactions               |
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

Windows data is saved as plain text in `%APPDATA%\\poch`. Linux data lives in `$XDG_DATA_HOME/poch`, or `~/.local/share/poch` when `XDG_DATA_HOME` is not set:

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
