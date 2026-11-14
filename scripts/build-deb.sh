#!/usr/bin/env bash
set -euo pipefail

root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd -- "$root"
arch=$(dpkg --print-architecture)
if [[ "$arch" != amd64 ]]; then
    echo "Este paquete se genera en Debian amd64." >&2
    exit 1
fi
if ! pkg-config --exists gtk4 libadwaita-1 gtksourceview-5; then
    source scripts/debian-env.sh
fi

cargo build --release --locked -p rustlab-app -j "${RUSTLAB_BUILD_JOBS:-2}"
version=$(cargo pkgid --locked -p rustlab-app)
version=${version##*#}
version=${version##*@}
package="rustlab-ide_${version}-1_${arch}.deb"

stage=$(mktemp -d)
trap 'rm -rf -- "$stage"' EXIT
dest="$stage/package"
install -Dm755 "${CARGO_TARGET_DIR:-target}/release/rustlab-app" "$dest/usr/lib/rustlab-ide/rustlab-app"
strip --strip-unneeded "$dest/usr/lib/rustlab-ide/rustlab-app"

install -d "$dest/usr/bin" "$dest/usr/share/applications" "$dest/DEBIAN" "$stage/debian"
cat >"$dest/usr/bin/rustlab-ide" <<'LAUNCHER'
#!/bin/sh
cargo_bin=${CARGO_HOME:-"$HOME/.cargo"}/bin
if [ -d "$cargo_bin" ]; then
    PATH="$cargo_bin:$PATH"
    export PATH
fi
exec /usr/lib/rustlab-ide/rustlab-app "$@"
LAUNCHER
chmod 755 "$dest/usr/bin/rustlab-ide"
cat >"$dest/usr/share/applications/org.rustlab.IDE.desktop" <<'DESKTOP'
[Desktop Entry]
Type=Application
Name=RustLab IDE
Comment=Entorno gráfico de desarrollo para Rust
Exec=rustlab-ide
Icon=applications-development
Terminal=false
Categories=Development;IDE;
DESKTOP

cat >"$stage/debian/control" <<'CONTROL'
Source: rustlab-ide

Package: rustlab-ide
Architecture: any
Description: Entorno gráfico de desarrollo para Rust
CONTROL
deps=$(cd "$stage" && dpkg-shlibdeps -O package/usr/lib/rustlab-ide/rustlab-app)
deps=${deps#shlibs:Depends=}
# Keep the explicit API floors when symbol metadata gives a lower development version.
deps=$(printf '%s\n' "$deps" | sed -E 's/, libgtk-4-1 \([^)]*\)//; s/, libgtksourceview-5-0 \([^)]*\)//')
size=$(du -sk --exclude=DEBIAN "$dest" | cut -f1)
cat >"$dest/DEBIAN/control" <<CONTROL
Package: rustlab-ide
Version: ${version}-1
Architecture: $arch
Section: devel
Priority: optional
Maintainer: RustLab IDE <rustlab@localhost>
Installed-Size: $size
Depends: $deps, libgtk-4-1 (>= 4.10), libgtksourceview-5-0 (>= 5.6)
Suggests: cargo, rust-analyzer, git
Description: Entorno gráfico de desarrollo para Rust
 Editor nativo con explorador de proyectos Cargo, resaltado de sintaxis,
 autocompletado mediante rust-analyzer y acciones de compilación.
CONTROL

if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$dest/usr/share/applications/org.rustlab.IDE.desktop"
fi
mkdir -p target/debian
dpkg-deb --root-owner-group --build "$dest" "target/debian/$package"
