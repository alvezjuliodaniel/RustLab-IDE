#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
arch=$(dpkg --print-architecture)
if [[ "$arch" != amd64 ]]; then
    echo "Este script prepara Debian 13 amd64." >&2
    exit 1
fi
mkdir -p "$root/.local/debs" "$root/.local/sysroot"
cd -- "$root/.local/debs"
apt-get download gir1.2-appstream-1.0 gir1.2-gtksource-5 \
    libappstream-dev libgraphene-1.0-dev libgtk-4-dev libadwaita-1-dev libgtksourceview-5-dev
for package in ./*.deb; do
    dpkg-deb -x "$package" ../sysroot
done
libdir="$root/.local/sysroot/usr/lib/x86_64-linux-gnu"
for metadata in "$libdir"/pkgconfig/*.pc; do
    # pkg-config quoting preserves checkout paths with spaces.
    RUSTLAB_LOCAL_PREFIX="$root/.local/sysroot/usr" python3 - "$metadata" <<'PY'
import os
from pathlib import Path
import sys
path = Path(sys.argv[1])
prefix = os.environ["RUSTLAB_LOCAL_PREFIX"].replace(" ", r"\ ")
path.write_text(path.read_text().replace("prefix=/usr\n", f"prefix={prefix}\n", 1))
PY
done
for link in "$libdir"/*.so; do
    if [[ -L "$link" && ! -e "$link" ]]; then
        soname=$(readlink "$link")
        if [[ -e "/usr/lib/x86_64-linux-gnu/$soname" ]]; then
            ln -sfn "/usr/lib/x86_64-linux-gnu/$soname" "$link"
        fi
    fi
done
source "$root/scripts/debian-env.sh"
pkg-config --modversion gtk4 libadwaita-1 gtksourceview-5
