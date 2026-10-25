#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="${ROOT}/dist"
VERSION="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "${ROOT}/Cargo.toml" | head -n 1)}"
ARCH="${ARCH:-amd64}"
BUNDLE="${DIST}/poch-${VERSION}-linux-x86_64"

if [[ ! -x "${ROOT}/target/release/poch" ]]; then
  echo "Missing target/release/poch. Run 'cargo build --release' first." >&2
  exit 1
fi
if ! command -v convert >/dev/null 2>&1; then
  echo "ImageMagick is required to convert icon.ico to PNG." >&2
  exit 1
fi

rm -rf "${BUNDLE}" "${DIST}/Poch.AppDir" "${DIST}/deb-root"
mkdir -p "${DIST}" "${BUNDLE}" \
  "${DIST}/deb-root/DEBIAN" \
  "${DIST}/deb-root/usr/bin" \
  "${DIST}/deb-root/usr/share/applications" \
  "${DIST}/deb-root/usr/share/icons/hicolor/256x256/apps" \
  "${DIST}/Poch.AppDir/usr/bin" \
  "${DIST}/Poch.AppDir/usr/share/applications" \
  "${DIST}/Poch.AppDir/usr/share/icons/hicolor/256x256/apps"

convert "${ROOT}/icon.ico[0]" -resize 256x256 "PNG32:${DIST}/poch.png"

install -m 755 "${ROOT}/target/release/poch" "${BUNDLE}/poch"
install -m 644 "${ROOT}/packaging/linux/poch.desktop" "${BUNDLE}/poch.desktop"
install -m 644 "${DIST}/poch.png" "${BUNDLE}/poch.png"
install -m 644 "${ROOT}/README.md" "${BUNDLE}/README.md"
install -m 644 "${ROOT}/LICENSE" "${BUNDLE}/LICENSE"

tar -C "${DIST}" -czf "${DIST}/poch-${VERSION}-linux-x86_64.tar.gz" "poch-${VERSION}-linux-x86_64"

DEBROOT="${DIST}/deb-root"
install -m 755 "${ROOT}/target/release/poch" "${DEBROOT}/usr/bin/poch"
install -m 644 "${ROOT}/packaging/linux/poch.desktop" "${DEBROOT}/usr/share/applications/poch.desktop"
install -m 644 "${DIST}/poch.png" "${DEBROOT}/usr/share/icons/hicolor/256x256/apps/poch.png"
cat > "${DEBROOT}/DEBIAN/control" <<CONTROL
Package: poch
Version: ${VERSION}
Section: utils
Priority: optional
Architecture: ${ARCH}
Maintainer: Commonly Works
Depends: libasound2 | libasound2t64, libegl1, libvulkan1, libwayland-client0, libwayland-cursor0, libwayland-egl1, libx11-6, libxkbcommon0, libxcursor1, libxinerama1, libxrandr2, libxi6
Description: A tiny interactive notch for the Linux desktop
 Poch combines media controls, a water tracker, tasks, a file shelf and notes.
CONTROL
dpkg-deb --build --root-owner-group "${DEBROOT}" "${DIST}/poch_${VERSION}_${ARCH}.deb"

APPDIR="${DIST}/Poch.AppDir"
install -m 755 "${ROOT}/target/release/poch" "${APPDIR}/usr/bin/poch"
install -m 644 "${ROOT}/packaging/linux/poch.desktop" "${APPDIR}/poch.desktop"
install -m 644 "${ROOT}/packaging/linux/poch.desktop" "${APPDIR}/usr/share/applications/poch.desktop"
install -m 644 "${DIST}/poch.png" "${APPDIR}/poch.png"
install -m 644 "${DIST}/poch.png" "${APPDIR}/.DirIcon"
install -m 644 "${DIST}/poch.png" "${APPDIR}/usr/share/icons/hicolor/256x256/apps/poch.png"
cat > "${APPDIR}/AppRun" <<'APPRUN'
#!/usr/bin/env sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/poch" "$@"
APPRUN
chmod 755 "${APPDIR}/AppRun"

echo "Created Linux tar.gz, Debian package, and AppImage staging directory in ${DIST}."
