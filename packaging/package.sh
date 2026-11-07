#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Builds one architecture's .deb and binary tarball from its static binaries and
# THIRD-PARTY-LICENSES in target/dist, and for x86_64 also the .rpm and the source tarball.
# `make packages ARCH=...` runs it in that architecture's build container, from the repository
# root, with the version, the `git archive` of the commit and the architecture (x86_64 or
# aarch64) as its arguments and SOURCE_DATE_EPOCH (that commit's time) set: every file's time
# and every package's build time is that instant.
set -eu

version=$1
archive=$2
arch=$3
: "${SOURCE_DATE_EPOCH:?SOURCE_DATE_EPOCH must be set}"
dist=target/dist
work=target/package
# Where the static binaries are, and the architecture's name for dpkg.
case "$arch" in
  x86_64) bins=$dist deb_arch=amd64 ;;
  aarch64) bins=$dist/aarch64 deb_arch=arm64 ;;
  *) echo "package.sh: the architecture is x86_64 or aarch64, not '$arch'" >&2; exit 1 ;;
esac
name=svolder
summary="Control Schiit Forkbeard devices over Bluetooth LE: a CLI, a TUI and a daemon"
homepage=https://github.com/RRAlex/svolder

rm -rf "$work"
mkdir -p "$work"

# Sorted entries owned by root, every time the commit's: a tarball two builds agree on.
pack() { # <directory under $work> <output>
  tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@$SOURCE_DATE_EPOCH" \
    -C "$work" -cf - "$1" | gzip -n > "$2"
}

# The installed tree every package carries: the binaries, the user units filled in for
# /usr/bin and the session's runtime directory (what `svolder daemon install` writes as root),
# and the documentation.
root=$work/root
for bin in svolder svolderd svolder-sim; do
  install -Dm755 "$bins/$bin" "$root/usr/bin/$bin"
done
for unit in svolderd.service svolderd.socket; do
  mkdir -p "$root/usr/lib/systemd/user"
  sed -e 's|__BINDIR__|/usr/bin|g' -e 's|__SOCKET__|%t/svolder.sock|g' \
    "systemd/$unit" > "$root/usr/lib/systemd/user/$unit"
  chmod 644 "$root/usr/lib/systemd/user/$unit"
done
doc=$root/usr/share/doc/$name
install -Dm644 README.md "$doc/README.md"
for page in docs/usage/*.md; do
  install -Dm644 "$page" "$doc/usage/${page##*/}"
done
install -Dm644 LEGAL.md "$doc/LEGAL.md"
install -Dm644 AUTHORS "$doc/AUTHORS"

# Debian: LICENSE, THIRD-PARTY-LICENSES and a DEP-5 copyright (LICENSE's text in it as the
# GPL's standalone paragraph) beside the docs, and md5sums for `dpkg --verify`.
debroot=$work/deb
cp -a "$root" "$debroot"
install -Dm644 LICENSE "$debroot/usr/share/doc/$name/LICENSE"
install -Dm644 "$dist/THIRD-PARTY-LICENSES" "$debroot/usr/share/doc/$name/THIRD-PARTY-LICENSES"
{
  printf 'Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/\n'
  printf 'Upstream-Name: svolder\n\n'
  printf 'Files: *\nCopyright: 2026 the svolder authors\nLicense: GPL-3.0-or-later\n\n'
  printf 'Files: usr/bin/*\nCopyright: 2026 the svolder authors\n'
  printf 'License: GPL-3.0-or-later\nComment: The binaries also contain the Rust crates, libdbus, the Rust standard\n'
  printf ' library and musl libc listed in /usr/share/doc/svolder/THIRD-PARTY-LICENSES, each\n'
  printf ' under the licence given there with its full text. Their complete corresponding\n'
  printf ' source is svolder-%s-source.tar.gz, published beside this package.\n\n' "$version"
  printf 'License: GPL-3.0-or-later\n'
  sed -e 's/^$/./' -e 's/^/ /' LICENSE
} > "$debroot/usr/share/doc/$name/copyright"
mkdir -p "$debroot/DEBIAN"
(cd "$debroot" && find usr -type f | LC_ALL=C sort | xargs md5sum) > "$debroot/DEBIAN/md5sums"
size=$(du -sk --apparent-size "$debroot/usr" | cut -f1)
cat > "$debroot/DEBIAN/control" <<EOF
Package: $name
Version: $version
Architecture: $deb_arch
Maintainer: the svolder authors
Homepage: $homepage
Installed-Size: $size
Depends: bluez, dbus
Recommends: pipewire-bin, wireplumber
Section: sound
Priority: optional
Description: $summary
 svolder controls Schiit Audio components that carry the Forkbeard Bluetooth
 LE module: volume, input, tone control and presets, from the command line,
 a terminal UI, or a per-user daemon other front ends share. The daemon can
 bridge to Home Assistant over MQTT. Host audio features use PipeWire's own
 tools when they are present. Schiit Audio has not made, endorsed or
 reviewed svolder.
EOF
find "$debroot" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
dpkg-deb --root-owner-group -Zxz --build "$debroot" "$dist/${name}_${version}_${deb_arch}.deb"

# The binary tarball: the binaries with the licences and docs beside them, for any
# distribution. `svolder daemon install` writes the units for wherever the binaries end up;
# the templates it fills in ride along for a package built from the tarball.
tarname=$name-$version-$arch-linux
tardir=$work/$tarname
mkdir -p "$tardir/systemd"
cp "$bins/svolder" "$bins/svolderd" "$bins/svolder-sim" LICENSE AUTHORS README.md LEGAL.md \
  "$dist/THIRD-PARTY-LICENSES" "$tardir/"
mkdir -p "$tardir/usage"
cp docs/usage/*.md "$tardir/usage/"
cp systemd/svolderd.service systemd/svolderd.socket "$tardir/systemd/"
pack "$tarname" "$dist/$tarname.tar.gz"

# What the rest builds once, in the x86_64 run: the .rpm and the source tarball.
[ "$arch" = x86_64 ] || { ls -l "$dist"; exit 0; }

# RPM: the same tree, LICENSE and THIRD-PARTY-LICENSES as %license, the build time and host fixed
# so two builds of one commit match.
rpmtop=$(pwd)/$work/rpm
mkdir -p "$rpmtop/SPECS" "$rpmtop/licenses"
cp LICENSE "$dist/THIRD-PARTY-LICENSES" "$rpmtop/licenses/"
cat > "$rpmtop/SPECS/$name.spec" <<EOF
Name: $name
Version: $version
Release: 1
Summary: $summary
License: GPL-3.0-or-later
URL: $homepage
BuildArch: x86_64
# The binaries are static: there is no shared library for rpm to find a dependency in.
AutoReqProv: no
Requires: bluez, dbus
Recommends: pipewire-utils, wireplumber

%description
svolder controls Schiit Audio components that carry the Forkbeard Bluetooth LE
module: volume, input, tone control and presets, from the command line, a
terminal UI, or a per-user daemon other front ends share. The daemon can bridge
to Home Assistant over MQTT. Schiit Audio has not made, endorsed or reviewed
svolder.

%install
cp -a $(pwd)/$root/. %{buildroot}/
mkdir -p %{buildroot}/usr/share/licenses/$name
cp $rpmtop/licenses/LICENSE $rpmtop/licenses/THIRD-PARTY-LICENSES %{buildroot}/usr/share/licenses/$name/

%files
/usr/bin/svolder
/usr/bin/svolderd
/usr/bin/svolder-sim
/usr/lib/systemd/user/svolderd.service
/usr/lib/systemd/user/svolderd.socket
%dir /usr/share/doc/$name
%doc /usr/share/doc/$name/README.md
%doc /usr/share/doc/$name/usage
%doc /usr/share/doc/$name/LEGAL.md
%doc /usr/share/doc/$name/AUTHORS
%dir /usr/share/licenses/$name
%license /usr/share/licenses/$name/LICENSE
%license /usr/share/licenses/$name/THIRD-PARTY-LICENSES
EOF
HOME=$rpmtop rpmbuild -bb --quiet --nodeps \
  --define "_topdir $rpmtop" \
  --define "_buildhost svolder" \
  --define "use_source_date_epoch_as_buildtime 1" \
  --define "build_mtime_policy clamp_to_source_date_epoch" \
  --define "_dbpath $rpmtop/db" \
  --define "debug_package %{nil}" \
  --define "__strip /bin/true" \
  "$rpmtop/SPECS/$name.spec"
cp "$rpmtop/RPMS/x86_64/$name-$version-1.x86_64.rpm" "$dist/"

# The source tarball (once, from the x86_64 run): the commit as `git archive` wrote it, with every
# crate the build uses vendored beside it (libdbus's source inside libdbus-sys), so it is the
# complete corresponding source of the binaries and builds offline (`cargo build --offline`).
srcname=$name-$version
srcdir=$work/$srcname
mkdir -p "$srcdir/.cargo"
tar -xf "$archive" -C "$work"
cargo vendor --locked --offline --manifest-path "$srcdir/Cargo.toml" "$srcdir/vendor" \
  | sed -e "s|$(pwd)/||" -e "s|$srcdir/||" > "$srcdir/.cargo/config.toml"
pack "$srcname" "$dist/$srcname-source.tar.gz"

ls -l "$dist"
