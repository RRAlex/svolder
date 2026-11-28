#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Writes target/dist/THIRD-PARTY-LICENSES: the licence of every crate the binaries are built
# from (cargo-about, pinned in the build container), then those of what the static build links
# in besides: libdbus, from the source libdbus-sys bundles; the Rust standard library; musl
# libc, which the Rust musl target carries; the NOTICE of the sofar crate (the SOFA reader),
# whose code derives from libmysofa and a KD-tree, both BSD-3-Clause; and the files the web UI
# serves, the IBM Plex and Geist, Crimson Pro, Caveat, Russo One and Mr Dafoe fonts, htmx and
# three.js, with the licences they ship with.
# `make 3rdpartylicenses` runs it in the build container from the repository root. The
# crates are fetched first, then cargo-about runs --frozen, off the network, so the file depends
# only on Cargo.lock.
set -eu

out=target/dist/THIRD-PARTY-LICENSES
mkdir -p target/dist
# cargo-about reads `cargo metadata`, which resolves every platform's crates, so all are fetched.
cargo fetch --locked
cargo about generate --frozen -c packaging/about.toml -o "$out" packaging/about.hbs

version=$(sed -n '/^name = "libdbus-sys"$/{n;s/^version = "\(.*\)"$/\1/p;}' Cargo.lock)
copying=$(ls "$CARGO_HOME"/registry/src/*/"libdbus-sys-$version"/vendor/dbus/COPYING)
section() { # <heading>
  printf '\n%s\n%s\n\n' \
    '================================================================================' "$1"
}
{
  section 'libdbus, bundled by libdbus-sys (AFL-2.1 OR GPL-2.0-or-later, used here under GPL-2.0-or-later)'
  cat "$copying"
  # The toolchain's own notice for the standard library and what it is built from, as text.
  section 'The Rust standard library (MIT OR Apache-2.0; its notices and their licences follow)'
  sed -e 's/<[^>]*>//g' -e 's/&lt;/</g' -e 's/&gt;/>/g' -e 's/&quot;/"/g' -e "s/&#39;/'/g" \
    -e 's/&amp;/\&/g' "$RUSTUP_HOME"/toolchains/*/share/doc/rust/COPYRIGHT-library.html \
    | cat -s
  # musl's terms as Ubuntu's musl package records them, without the stanza for its packaging.
  section 'musl libc, carried by the Rust musl target (MIT)'
  awk '/^Files: debian\//{skip=1} skip && /^$/{skip=0; next} !skip' /usr/share/doc/musl/copyright
  # sofar's NOTICE as the crate ships it, then the BSD-3-Clause terms it names, under both
  # copyrights: its HDF5 reader (src/hdf/), the part svolder links, derives from libmysofa.
  section 'sofar, the SOFA reader: code derived from libmysofa and a KD-tree (BSD-3-Clause)'
  cat <<'SOFAR'
NOTICE

This software contains code derived from the libmysofa project.

libmysofa
---------
Copyright (c) 2016-2017, Symonics GmbH, Christian Hoene
Licensed under the BSD 3-Clause License
https://github.com/hoene/libmysofa

The following components are derived from libmysofa:

  - HDF5 file format parser (src/hdf/)
  - SOFA/HRTF algorithms (src/sofa/): spatial lookup, interpolation,
    coordinate conversion, loudness normalization, validation

The KD-tree implementation is based on work by:

  Copyright (C) 2007-2011 John Tsiombikas <nuclear@member.fsf.org>
  Licensed under BSD 3-Clause License

The BSD 3-Clause License, as it applies to both:

Copyright (c) 2016-2017, Symonics GmbH, Christian Hoene
Copyright (C) 2007-2011 John Tsiombikas <nuclear@member.fsf.org>

Redistribution and use in source and binary forms, with or without modification, are permitted
provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this list of conditions
   and the following disclaimer.
2. Redistributions in binary form must reproduce the above copyright notice, this list of
   conditions and the following disclaimer in the documentation and/or other materials provided
   with the distribution.
3. Neither the name of the copyright holder nor the names of its contributors may be used to
   endorse or promote products derived from this software without specific prior written
   permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR
IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND
FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR
CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER
IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT
OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
SOFAR
  section 'IBM Plex Sans and IBM Plex Mono, served by the web UI (SIL Open Font License 1.1)'
  cat bins/svolderd/web/fonts/OFL.txt
  section 'Geist, served by the web UI (SIL Open Font License 1.1)'
  cat bins/svolderd/web/fonts/OFL-Geist.txt
  section 'Crimson Pro, served by the web UI (SIL Open Font License 1.1)'
  cat bins/svolderd/web/fonts/OFL-CrimsonPro.txt
  section 'Caveat, served by the web UI (SIL Open Font License 1.1)'
  cat bins/svolderd/web/fonts/OFL-Caveat.txt
  section 'Russo One, served by the web UI (SIL Open Font License 1.1)'
  cat bins/svolderd/web/fonts/OFL-RussoOne.txt
  section 'Mr Dafoe, served by the web UI (SIL Open Font License 1.1)'
  cat bins/svolderd/web/fonts/OFL-MrDafoe.txt
  section 'htmx, served by the web UI (0BSD)'
  cat bins/svolderd/web/themes/ember/htmx.LICENSE
  section 'three.js r187, served by the web UI (MIT)'
  cat bins/svolderd/web/themes/voidgarden/vendor/three.LICENSE
} >> "$out"
