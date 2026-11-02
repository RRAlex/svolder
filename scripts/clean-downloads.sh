#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Remove the crates.io packages Cargo.lock names from cargo's download cache (the archive, its
# unpacked source and its index entry), so the next build downloads them as a new checkout's
# would. `make clean-all` runs this after `cargo clean`. The cache is shared with every other
# Rust project on the machine: a crate only they use stays, one they share comes back on their
# next build.
set -eu

home=${CARGO_HOME:-$HOME/.cargo}
registry=$home/registry

# `make clean-rust` passes `all`: the whole cache goes, every project's crates and git
# dependencies with it, and each project downloads its own again on its next build. Installed
# tools in bin/ stay.
if [ "${1:-}" = all ]; then
    size=$(du -sk "$registry" "$home/git" 2>/dev/null | awk '{ k += $1 } END { print int(k / 1024) }')
    rm -rf "$registry" "$home/git"
    echo "removed cargo's whole download cache from $home ($size MB)"
    exit 0
fi

[ -d "$registry" ] || { echo "no download cache at $registry"; exit 0; }

# Where cargo keeps a crate's index entry under an index directory's .cache, by name length.
index_entry() {
    case ${#1} in
        1) echo "1/$1" ;;
        2) echo "2/$1" ;;
        3) echo "3/$(echo "$1" | cut -c1)/$1" ;;
        *) echo "$(echo "$1" | cut -c1-2)/$(echo "$1" | cut -c3-4)/$1" ;;
    esac
}

before=$(du -sk "$registry" | cut -f1)
found=0
total=0
# One "name version" line per crates.io package; `source` follows `name` and `version` in a
# Cargo.lock entry, and a workspace crate has none.
for crate in $(awk -F'"' '
    /^name = /               { name = $2 }
    /^version = /            { version = $2 }
    /^source = "registry\+/  { print name "@" version }
' Cargo.lock); do
    name=${crate%@*}
    version=${crate#*@}
    total=$((total + 1))
    for archive in "$registry"/cache/*/"$name-$version.crate"; do
        [ -e "$archive" ] && found=$((found + 1))
        rm -f "$archive"
    done
    rm -rf "$registry"/src/*/"$name-$version"
    entry=$(index_entry "$(echo "$name" | tr 'A-Z' 'a-z')")
    rm -f "$registry"/index/*/.cache/"$entry"
done
after=$(du -sk "$registry" | cut -f1)
echo "removed $found of the $total downloaded crates Cargo.lock names from $registry ($(((before - after) / 1024)) MB)"
