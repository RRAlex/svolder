#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Check that what a build of this checkout needs is installed, and say how to install what is
# missing on this distribution. `build` (the default) checks what `make build` needs: cargo, a
# rustc no older than Cargo.toml's rust-version, a C compiler to link with, make, and libdbus's
# development files, found through pkg-config as libdbus-sys finds them (a musl host builds the
# libdbus source libdbus-sys bundles instead, and needs neither). `lint` adds rustfmt and clippy,
# for `make fmt` and `make clippy`; `check` adds what `make check` runs besides: rustfmt, clippy,
# python3 with its yaml module (PyYAML, for `make workflow-check`) and git. When something is
# missing it prints what, one command that installs it, and exits 1; otherwise it prints a line
# saying so, or nothing under --quiet, and exits 0. It never installs anything itself.
#
# The distribution comes from ID and ID_LIKE in /etc/os-release (BUILD_DEPS_OS_RELEASE names
# another file, for the self-test): apt, dnf, pacman, zypper and apk families get their package
# names, NixOS a nix-shell line, any other distribution the list in words. Where rustup manages
# Rust (rustc is missing or is rustup's own), the Rust parts are rustup commands.
set -u

quiet=0
if [ "${1:-}" = --quiet ]; then
    quiet=1
    shift
fi
mode=${1:-build}
case $mode in
    build | lint | check) ;;
    *)
        echo "usage: $0 [--quiet] [build|lint|check]" >&2
        exit 2
        ;;
esac

root=$(dirname "$0")/..
cargo=${CARGO:-cargo}
rustc=${RUSTC:-rustc}
pkg_config=${PKG_CONFIG:-pkg-config}
os_release=${BUILD_DEPS_OS_RELEASE:-/etc/os-release}
want=$(sed -n 's/^rust-version = "\(.*\)"$/\1/p' "$root/Cargo.toml" | head -n 1)

found() { command -v "$1" >/dev/null 2>&1; }

# Version $1 as one number to compare, 1.88.2 as 1088002; a pre-release (1.90.0-beta.3) is its
# release. Shell arithmetic only: a host short of tools may have no awk either.
version_number() {
    v=${1%%-*}
    major=${v%%.*}
    v=${v#"$major"}
    v=${v#.}
    minor=${v%%.*}
    v=${v#"$minor"}
    v=${v#.}
    patch=${v%%.*}
    echo $((${major:-0} * 1000000 + ${minor:-0} * 1000 + ${patch:-0}))
}

# Whether version $1 is $2 or later.
at_least() { [ "$(version_number "$1")" -ge "$(version_number "$2")" ]; }

# What is missing, as keys: rust cc make pkgconfig dbus rustfmt clippy python3 pyyaml git.
missing=
# The rustc found when it is older than rust-version.
old=
need() { missing="$missing $1"; }

host=
have=
if found "$cargo" && found "$rustc"; then
    have=$("$rustc" --version 2>/dev/null | sed -n 's/^rustc \([^ ]*\).*/\1/p')
    host=$("$rustc" -vV 2>/dev/null | sed -n 's/^host: //p')
fi
if [ -z "$have" ]; then
    need rust
elif ! at_least "$have" "$want"; then
    old=$have
fi
found cc || need cc
found make || need make
case $host in
    *-musl) ;;
    *)
        if ! found "$pkg_config"; then
            need pkgconfig
            need dbus
        elif ! "$pkg_config" --exists 'dbus-1 >= 1.6'; then
            need dbus
        fi
        ;;
esac
if [ "$mode" != build ]; then
    "$cargo" fmt --version >/dev/null 2>&1 || need rustfmt
    "$cargo" clippy --version >/dev/null 2>&1 || need clippy
fi
if [ "$mode" = check ]; then
    if ! found python3; then
        need python3
        need pyyaml
    elif ! python3 -I -c 'import yaml' >/dev/null 2>&1; then
        need pyyaml
    fi
    found git || need git
fi

if [ -z "$missing" ] && [ -z "$old" ]; then
    [ "$quiet" = 1 ] || echo "build-deps: everything the $mode needs is installed"
    exit 0
fi

describe() {
    case $1 in
        rust) echo "Rust $want or later: cargo and rustc" ;;
        cc) echo "a C compiler to link with (cc)" ;;
        pkgconfig) echo "pkg-config" ;;
        dbus) echo "libdbus's development files (dbus-1.pc, dbus-1 1.6 or later)" ;;
        rustfmt) echo "rustfmt (cargo fmt)" ;;
        clippy) echo "clippy (cargo clippy)" ;;
        pyyaml) echo "PyYAML, python3's yaml module, for make workflow-check" ;;
        *) echo "$1" ;;
    esac
}

family=
if [ -r "$os_release" ]; then
    for id in $(. "$os_release" && echo "${ID:-} ${ID_LIKE:-}"); do
        case $id in
            debian | ubuntu) family=apt ;;
            fedora | rhel | centos) family=dnf ;;
            arch) family=pacman ;;
            opensuse* | suse | sles) family=zypper ;;
            alpine) family=apk ;;
            nixos) family=nix ;;
            *) continue ;;
        esac
        break
    done
fi

# The packages that bring key $2 in family $1, or nothing when the family is not known.
packages() {
    case $1:$2 in
        apt:rust) echo cargo rustc ;;
        apt:cc) echo build-essential ;;
        apt:dbus) echo libdbus-1-dev ;;
        apt:clippy) echo rust-clippy ;;
        apt:pyyaml) echo python3-yaml ;;
        dnf:pyyaml) echo python3-pyyaml ;;
        pacman:pyyaml) echo python-yaml ;;
        zypper:pyyaml) echo python3-PyYAML ;;
        apk:pyyaml) echo py3-yaml ;;
        nix:pyyaml) echo python3Packages.pyyaml ;;
        dnf:rust | zypper:rust | apk:rust) echo cargo rust ;;
        dnf:cc | pacman:cc | zypper:cc) echo gcc ;;
        dnf:pkgconfig) echo pkgconf-pkg-config ;;
        dnf:dbus) echo dbus-devel ;;
        pacman:rust | pacman:rustfmt | pacman:clippy | zypper:rustfmt | zypper:clippy) echo rust ;;
        pacman:pkgconfig | apk:pkgconfig) echo pkgconf ;;
        pacman:dbus) echo dbus ;;
        pacman:python3) echo python ;;
        zypper:dbus) echo dbus-1-devel ;;
        apk:cc) echo build-base ;;
        apk:dbus) echo dbus-dev ;;
        apk:clippy) echo rust-clippy ;;
        nix:rust) echo cargo rustc ;;
        nix:cc) echo gcc ;;
        nix:make) echo gnumake ;;
        :*) ;;
        *:pkgconfig) echo pkg-config ;;
        *) echo "$2" ;;
    esac
}

# rustup manages this Rust when it is on PATH and rustc is missing or one of its proxies, which
# sit beside it; a distribution's rustc first on PATH is not rustup's to update.
rustup=0
if found rustup; then
    if ! found "$rustc" ||
        [ "$(dirname "$(command -v "$rustc")")" = "$(dirname "$(command -v rustup)")" ]; then
        rustup=1
    fi
fi
echo "build-deps: the $mode needs what is not installed here:"
for key in $missing; do
    echo "  - $(describe "$key")"
done
[ -z "$old" ] || echo "  - Rust $want or later: this rustc is $old"

# The keys a package manager installs, and the packages that bring them; under rustup, Rust and
# its components are rustup's.
pm_keys=
list=
rust_keys=
for key in $missing; do
    case $rustup:$key in
        1:rust | 1:rustfmt | 1:clippy) rust_keys="$rust_keys $key" ;;
        *)
            pm_keys="$pm_keys $key"
            list="$list $(packages "$family" "$key")"
            ;;
    esac
done
# Each package once, in the order first named.
once=
for package in $list; do
    case " $once " in
        *" $package "*) ;;
        *) once="$once $package" ;;
    esac
done
list=${once# }

sudo='sudo '
[ "$(id -u)" = 0 ] && sudo=

if [ -n "$pm_keys" ]; then
    case $family in
        apt) echo "Install them with:"; echo "  ${sudo}apt install $list" ;;
        dnf) echo "Install them with:"; echo "  ${sudo}dnf install $list" ;;
        pacman) echo "Install them with:"; echo "  ${sudo}pacman -S --needed $list" ;;
        zypper) echo "Install them with:"; echo "  ${sudo}zypper install $list" ;;
        apk) echo "Install them with:"; echo "  ${sudo}apk add $list" ;;
        nix)
            echo "Run the build in a shell that has them:"
            echo "  nix-shell -p $list"
            echo "or build the package with the flake, which brings its own: nix build"
            ;;
        *) echo "Install them with your distribution's package manager." ;;
    esac
fi
for key in $rust_keys; do
    case $key in
        rust) echo "Install Rust with rustup:"; echo "  rustup default stable" ;;
    esac
done
components=
for key in $rust_keys; do
    case $key in
        rustfmt | clippy) components="$components $key" ;;
    esac
done
if [ -n "$components" ]; then
    echo "Add the components with rustup:"
    echo "  rustup component add$components"
fi
if [ -n "$old" ]; then
    if [ "$rustup" = 1 ]; then
        echo "Update Rust with rustup:"
        echo "  rustup update"
    else
        echo "This distribution's Rust is older than svolder needs; rustup installs a current one:"
        echo "  https://rustup.rs"
    fi
fi
exit 1
