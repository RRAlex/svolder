#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The README's two GNOME indicator pictures, docs/images/gnome-indicator-untouched.webp and
# gnome-indicator-changed.webp: the headless GNOME Shell of `make gnome-extension-check` at scale 3
# (a fake svolder, the blessed fixtures, scratch homes; never your session) shoots the untouched
# and the "DAC changes the sound" fixture, and gnome_shots.py puts each one's top-bar item above its
# open popup. Needs gnome-shell (gnome-shell-test-tool, gnome-extensions), dbus-run-session and
# python3-pil with WebP support.

set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")

missing=
for tool in gnome-shell-test-tool gnome-extensions dbus-run-session python3; do
    command -v "$tool" >/dev/null 2>&1 || missing="$missing $tool"
done
python3 -c 'from PIL import features; raise SystemExit(0 if features.check("webp") else 1)' 2>/dev/null ||
    missing="$missing python3-pil(with webp)"
[ -z "$missing" ] || {
    echo "gnome-shots: missing:$missing (install them with apt)" >&2
    exit 1
}

. "$here/tmp-base.sh"
tmp=$(svtmp_dir gnome-shots)
trap 'rm -rf "$tmp"' EXIT

sh "$here/gnome-extension.sh" shots3 "$tmp"
python3 "$here/gnome_shots.py" "$tmp" "$root/docs/images"
