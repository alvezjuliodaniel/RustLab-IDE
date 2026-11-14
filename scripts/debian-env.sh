#!/usr/bin/env bash
# Source this file before building against the local Debian packages.
rustlab_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
rustlab_lib="$rustlab_root/.local/sysroot/usr/lib/x86_64-linux-gnu"
export PKG_CONFIG_PATH="$rustlab_lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
unset rustlab_root rustlab_lib
