#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# `make vendor-three`: put three.js r187's three build files, unmodified, in the theme Void Garden's
# vendor folder. The tarball comes from the npm registry (the one fetch), is kept in
# target/vendor-three/ and is checked against the sha256 pinned below before anything is read from
# it; a file is written only when its bytes differ from the one in the folder, so a second run
# writes nothing. Run it from the repository root.
set -eu

version=0.187.0
url=https://registry.npmjs.org/three/-/three-$version.tgz
sha256=ff540832ded0cfe919764d24a3d88f3e2a05f23303f876423e4f893841e90728
work=target/vendor-three
dest=bins/svolderd/web/themes/voidgarden/vendor

mkdir -p "$work"
tarball=$work/three-$version.tgz
sum() { sha256sum "$1" | cut -d' ' -f1; }

if [ ! -f "$tarball" ] || [ "$(sum "$tarball")" != "$sha256" ]; then
    echo "vendor-three: fetching $url"
    trap 'rm -f "$tarball.part"' EXIT
    curl -fsSL -o "$tarball.part" "$url"
    mv "$tarball.part" "$tarball"
    trap - EXIT
fi
got=$(sum "$tarball")
if [ "$got" != "$sha256" ]; then
    rm -f "$tarball"
    echo "vendor-three: the tarball's sha256 is $got, not the pinned $sha256" >&2
    exit 1
fi

rm -rf "$work/package"
tar -xzf "$tarball" -C "$work" package/build/three.core.js package/build/three.tsl.js \
    package/build/three.webgpu.js package/LICENSE

mkdir -p "$dest"
wrote=0
put() {
    if [ -f "$2" ] && cmp -s "$1" "$2"; then
        return
    fi
    cp "$1" "$2"
    echo "vendor-three: wrote $2"
    wrote=$((wrote + 1))
}
for name in three.core.js three.tsl.js three.webgpu.js; do
    put "$work/package/build/$name" "$dest/$name"
done
put "$work/package/LICENSE" "$dest/three.LICENSE"
rm -rf "$work/package"

if [ "$wrote" -eq 0 ]; then
    echo "vendor-three: $dest is up to date, nothing written"
fi
