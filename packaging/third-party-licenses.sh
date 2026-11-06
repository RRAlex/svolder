#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Writes target/dist/THIRD-PARTY-LICENSES: the licence of every crate the binaries are built
# from (cargo-about, pinned in the build container), then those of what the static build links
# in besides: libdbus, from the source libdbus-sys bundles; the Rust standard library; musl
# libc, which the Rust musl target carries; and the files the web UI serves, the IBM Plex and
# Geist, Crimson Pro, Caveat, Russo One and Mr Dafoe fonts and htmx, with the licences they ship with.
# `make third-party-licenses` runs it in the build container from the repository root. The
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
} >> "$out"
