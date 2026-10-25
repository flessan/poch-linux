# Poch on Linux

Poch has an early native Linux backend for x86_64 desktops. It is a floating, top-centred widget with Music, Water, To-do, Ledge and Scrap views. The existing Windows backend remains separate and keeps its Win32 implementation.

## Downloads

The Linux workflow produces:

- **AppImage** for a portable download.
- **tar.gz** with the executable, desktop entry, icon and license.
- **Debian package** for Debian/Ubuntu-based distributions.\n- **RPM package** for Fedora/RHEL-compatible and openSUSE-family distributions.

Builds are published as workflow artifacts on pushes and pull requests. Release builds attach the packages to the GitHub Release.

## Run

The AppImage is intended to run without installation. Make it executable if your file manager does not do this automatically:

```sh
chmod +x Poch-*-x86_64.AppImage
./Poch-*-x86_64.AppImage
```

For the tarball:

```sh
tar -xzf poch-*-linux-x86_64.tar.gz
cd poch-*-linux-x86_64
./poch
```

Install the Debian package with:

```sh
sudo apt install ./poch_*_amd64.deb
```

## Requirements and limitations

Poch uses your desktop's native display stack and graphics drivers. The AppImage bundles application-side libraries where possible, but host graphics drivers are not bundled. The optional Music integration uses the MPRIS interface through `playerctl`; install `playerctl` from your distribution's package manager to control compatible players.

The Linux backend is an initial port, not feature parity with Windows yet. Clock, basic media controls, water tracking, task checklist, a local file-path shelf and notes are implemented. Windows-specific busy detection, the Ctrl+Alt+H hotkey, drag-out file handling, Windows media-session artwork and some animations are not yet ported. Desktop compositors vary in how they handle transparent, always-on-top windows and positioning, especially on Wayland.

Data is stored under `$XDG_DATA_HOME/poch`, or `~/.local/share/poch` when `XDG_DATA_HOME` is not set.

## Build from source

Install Rust stable and your distribution's build dependencies for `winit`/graphics. Then run:

```sh
cargo build --release
```

On Linux, Cargo selects the native Linux backend automatically. To prepare a tarball, Debian/RPM packages and AppImage staging directory:

```sh
sudo apt install imagemagick dpkg-dev rpm
bash scripts/package-linux.sh
```

The AppImage step additionally uses `linuxdeploy` and `appimagetool`, as shown in `.github/workflows/linux.yml`.
