#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# The self-test of build-deps.sh: each case runs it on a PATH holding only the tools the case
# gives it (stubs that answer as the real ones do) and an os-release file of the case's own, and
# compares everything it prints, and its exit status, with what the case expects. `make check`
# runs this.
set -u

here=$(cd "$(dirname "$0")" && pwd)
script=$here/build-deps.sh
# The file tool $1 runs from on PATH: busybox's `command -v` names its own applets without a path.
where() {
    for dir in $(echo "$PATH" | tr : ' '); do
        if [ -x "$dir/$1" ]; then
            echo "$dir/$1"
            return
        fi
    done
}

shell=$(where sh)
want=$(sed -n 's/^rust-version = "\(.*\)"$/\1/p' "$here/../Cargo.toml" | head -n 1)
below=$(echo "$want" | awk -F. '{ printf "%d.%d.9", $1, $2 - 1 }')
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

cases=0
failed=0

# A stub named $1 in the case's PATH that runs the shell code $2.
stub() {
    printf '#!%s\n%s\n' "$shell" "$2" > "$bin/$1"
    chmod +x "$bin/$1"
}

# Start a case: an empty PATH but for the utilities build-deps.sh itself runs (no awk, grep or
# tr: a host short of tools may not have them), an os-release with the lines given, and a uid of
# 1000. $extra is a second PATH directory a case may fill.
setup() {
    rm -rf "$work/case"
    bin=$work/case/bin
    extra=$work/case/extra
    mkdir -p "$bin" "$extra"
    for tool in dirname head sed; do
        ln -s "$(where "$tool")" "$bin/$tool"
    done
    stub id 'echo 1000'
    printf '%s\n' "$@" > "$work/case/os-release"
}

# A Rust toolchain at version $1 for host $2: cargo with fmt and clippy, unless $3 names the
# subcommands left out.
rust() {
    stub rustc "case \$1 in --version) echo 'rustc $1 (0000000 2026-01-01)' ;; -vV) echo 'host: $2' ;; esac"
    stub cargo "case \$1 in ${3:-none}) exit 101 ;; *) exit 0 ;; esac"
}

# A whole build host: Rust $want on glibc, cc, make, pkg-config that finds dbus-1, python3 and
# git.
everything() {
    rust "$want.0" x86_64-unknown-linux-gnu
    stub cc 'exit 0'
    stub make 'exit 0'
    stub pkg-config 'exit 0'
    stub python3 'exit 0'
    stub git 'exit 0'
}

# Run build-deps.sh with the arguments given and compare with the expected status, from $status,
# and the expected output, from standard input.
expect() {
    cases=$((cases + 1))
    cat > "$work/want"
    PATH=$bin:$extra BUILD_DEPS_OS_RELEASE=$work/case/os-release "$shell" "$script" "$@" > "$work/got" 2>&1
    got=$?
    if [ "$got" != "$status" ] || ! cmp -s "$work/want" "$work/got"; then
        failed=$((failed + 1))
        echo "build-deps self-test, case $cases ($name): exit $got, wanted $status; output:"
        diff "$work/want" "$work/got" | sed 's/^/  /'
    fi
}

name='all there, quiet'
setup ID=ubuntu
everything
status=0 expect --quiet check < /dev/null

name='all there'
setup ID=ubuntu
everything
status=0 expect check <<'EOF'
build-deps: everything the check needs is installed
EOF

name='Ubuntu without pkg-config'
setup ID=ubuntu ID_LIKE=debian
everything
rm "$bin/pkg-config"
status=1 expect build <<'EOF'
build-deps: the build needs what is not installed here:
  - pkg-config
  - libdbus's development files (dbus-1.pc, dbus-1 1.6 or later)
Install them with:
  sudo apt install pkg-config libdbus-1-dev
EOF

name='Mint with nothing, by its ID_LIKE'
setup ID=linuxmint 'ID_LIKE="ubuntu debian"'
status=1 expect check <<EOF
build-deps: the check needs what is not installed here:
  - Rust $want or later: cargo and rustc
  - a C compiler to link with (cc)
  - make
  - pkg-config
  - libdbus's development files (dbus-1.pc, dbus-1 1.6 or later)
  - rustfmt (cargo fmt)
  - clippy (cargo clippy)
  - python3
  - PyYAML, python3's yaml module, for make workflow-check
  - git
Install them with:
  sudo apt install cargo rustc build-essential make pkg-config libdbus-1-dev rustfmt rust-clippy python3 python3-yaml git
EOF

name='python3 without its yaml module'
setup ID=ubuntu
everything
stub python3 'exit 1'
status=1 expect check <<'EOF'
build-deps: the check needs what is not installed here:
  - PyYAML, python3's yaml module, for make workflow-check
Install them with:
  sudo apt install python3-yaml
EOF

name='Fedora without dbus-1'
setup ID=fedora
everything
stub pkg-config 'exit 1'
status=1 expect <<'EOF'
build-deps: the build needs what is not installed here:
  - libdbus's development files (dbus-1.pc, dbus-1 1.6 or later)
Install them with:
  sudo dnf install dbus-devel
EOF

name='Rocky, by its ID_LIKE, without cc'
setup ID=rocky 'ID_LIKE="rhel centos fedora"'
everything
rm "$bin/cc"
status=1 expect build <<'EOF'
build-deps: the build needs what is not installed here:
  - a C compiler to link with (cc)
Install them with:
  sudo dnf install gcc
EOF

name='Manjaro with nothing, Arch packages each named once'
setup ID=manjaro ID_LIKE=arch
status=1 expect check <<EOF
build-deps: the check needs what is not installed here:
  - Rust $want or later: cargo and rustc
  - a C compiler to link with (cc)
  - make
  - pkg-config
  - libdbus's development files (dbus-1.pc, dbus-1 1.6 or later)
  - rustfmt (cargo fmt)
  - clippy (cargo clippy)
  - python3
  - PyYAML, python3's yaml module, for make workflow-check
  - git
Install them with:
  sudo pacman -S --needed rust gcc make pkgconf dbus python python-yaml git
EOF

name='openSUSE Tumbleweed with nothing, rustfmt and clippy in rust'
setup ID=opensuse-tumbleweed 'ID_LIKE="opensuse suse"'
status=1 expect check <<EOF
build-deps: the check needs what is not installed here:
  - Rust $want or later: cargo and rustc
  - a C compiler to link with (cc)
  - make
  - pkg-config
  - libdbus's development files (dbus-1.pc, dbus-1 1.6 or later)
  - rustfmt (cargo fmt)
  - clippy (cargo clippy)
  - python3
  - PyYAML, python3's yaml module, for make workflow-check
  - git
Install them with:
  sudo zypper install cargo rust gcc make pkg-config dbus-1-devel python3 python3-PyYAML git
EOF

name='Alpine: a musl host needs no dbus-1'
setup ID=alpine
rust "$want.0" x86_64-alpine-linux-musl 'fmt|clippy'
stub cc 'exit 0'
stub make 'exit 0'
stub python3 'exit 0'
stub git 'exit 0'
status=1 expect check <<'EOF'
build-deps: the check needs what is not installed here:
  - rustfmt (cargo fmt)
  - clippy (cargo clippy)
Install them with:
  sudo apk add rustfmt rust-clippy
EOF

name='NixOS with nothing'
setup ID=nixos
status=1 expect build <<EOF
build-deps: the build needs what is not installed here:
  - Rust $want or later: cargo and rustc
  - a C compiler to link with (cc)
  - make
  - pkg-config
  - libdbus's development files (dbus-1.pc, dbus-1 1.6 or later)
Run the build in a shell that has them:
  nix-shell -p cargo rustc gcc gnumake pkg-config dbus
or build the package with the flake, which brings its own: nix build
EOF

name='an unknown distribution'
setup ID=gentoo
everything
rm "$bin/pkg-config"
status=1 expect build <<'EOF'
build-deps: the build needs what is not installed here:
  - pkg-config
  - libdbus's development files (dbus-1.pc, dbus-1 1.6 or later)
Install them with your distribution's package manager.
EOF

name='no os-release'
setup
everything
rm "$work/case/os-release" "$bin/cc"
status=1 expect build <<'EOF'
build-deps: the build needs what is not installed here:
  - a C compiler to link with (cc)
Install them with your distribution's package manager.
EOF

name='a distribution Rust older than rust-version'
setup ID=debian
everything
rust "$below" x86_64-unknown-linux-gnu
status=1 expect build <<EOF
build-deps: the build needs what is not installed here:
  - Rust $want or later: this rustc is $below
This distribution's Rust is older than svolder needs; rustup installs a current one:
  https://rustup.rs
EOF

name='rustup: an old toolchain without clippy'
setup ID=ubuntu
everything
rust "$below" x86_64-unknown-linux-gnu clippy
stub rustup 'exit 0'
status=1 expect check <<EOF
build-deps: the check needs what is not installed here:
  - clippy (cargo clippy)
  - Rust $want or later: this rustc is $below
Add the components with rustup:
  rustup component add clippy
Update Rust with rustup:
  rustup update
EOF

name='rustup beside a distribution rustc first on PATH is not offered'
setup ID=debian
everything
rust "$below" x86_64-unknown-linux-gnu
printf '#!%s\nexit 0\n' "$shell" > "$extra/rustup"
chmod +x "$extra/rustup"
status=1 expect build <<EOF
build-deps: the build needs what is not installed here:
  - Rust $want or later: this rustc is $below
This distribution's Rust is older than svolder needs; rustup installs a current one:
  https://rustup.rs
EOF

name='lint asks for rustfmt and clippy, not python3 or git'
setup ID=ubuntu
everything
rust "$want.0" x86_64-unknown-linux-gnu clippy
rm "$bin/python3" "$bin/git"
status=1 expect lint <<'EOF'
build-deps: the lint needs what is not installed here:
  - clippy (cargo clippy)
Install them with:
  sudo apt install rust-clippy
EOF

name='rustup with no toolchain, and no git'
setup ID=fedora
everything
rm "$bin/cargo" "$bin/rustc" "$bin/git"
stub rustup 'exit 0'
status=1 expect check <<EOF
build-deps: the check needs what is not installed here:
  - Rust $want or later: cargo and rustc
  - rustfmt (cargo fmt)
  - clippy (cargo clippy)
  - git
Install them with:
  sudo dnf install git
Install Rust with rustup:
  rustup default stable
Add the components with rustup:
  rustup component add rustfmt clippy
EOF

name='root is not told to sudo'
setup ID=debian
everything
stub id 'echo 0'
stub pkg-config 'exit 1'
status=1 expect build <<'EOF'
build-deps: the build needs what is not installed here:
  - libdbus's development files (dbus-1.pc, dbus-1 1.6 or later)
Install them with:
  apt install libdbus-1-dev
EOF

name='a pre-release of rust-version counts as it'
setup ID=ubuntu
everything
rust "$want.0-beta.3" x86_64-unknown-linux-gnu
status=0 expect --quiet build < /dev/null

name='an unknown mode'
setup ID=ubuntu
status=2 expect fast <<EOF
usage: $script [--quiet] [build|lint|check]
EOF

if [ "$failed" -gt 0 ]; then
    echo "build-deps self-test: $failed of $cases cases failed"
    exit 1
fi
echo "build-deps self-test ok: $cases cases"
