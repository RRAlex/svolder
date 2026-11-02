#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Writes what combines the two architectures' packages in target/dist: PKGBUILD (both tarballs'
# checksums in it), SHA256SUMS (every release asset but itself, written last) and
# RELEASE-NOTES.md (packaging/release-notes.md.in with the version's section of CHANGELOG.md at its @CHANGELOG@ line).
# `make packages` runs it on the host, from the repository root, with the version as its argument.
set -eu

version=$1
dist=target/dist
tar_x86=svolder-$version-x86_64-linux.tar.gz
tar_arm=svolder-$version-aarch64-linux.tar.gz
assets="svolder_${version}_amd64.deb svolder_${version}_arm64.deb svolder-$version-1.x86_64.rpm
  $tar_x86 $tar_arm svolder-$version-source.tar.gz THIRD-PARTY-LICENSES PKGBUILD"

for f in ${assets% PKGBUILD}; do
  [ -f "$dist/$f" ] || {
    echo "release-files: $dist/$f is missing; make packages ARCH=x86_64 and ARCH=aarch64 write it" >&2
    exit 1
  }
done

# The section's body: the lines after its heading up to the next version's, without the blank
# lines around it. Checked before anything is written, so a failed run leaves target/dist as it was.
section=$(awk -v h="## $version" '$0 == h { on = 1; next } /^## / { on = 0 } on' CHANGELOG.md \
  | sed '/./,$!d')
[ -n "$section" ] || { echo "release-files: CHANGELOG.md has no section '## $version'" >&2; exit 1; }
[ "$section" != 'To be written at the merge.' ] || {
  echo "release-files: CHANGELOG.md's '## $version' section is still the placeholder" >&2
  exit 1
}

# The package description is the one package.sh gives the .deb and the .rpm.
summary=$(sed -n 's/^summary="\(.*\)"$/\1/p' packaging/package.sh)
sum() { sha256sum "$dist/$1" | cut -d' ' -f1; }
sed -e "s|@VERSION@|$version|g" -e "s|@SUMMARY@|$summary|g" \
  -e "s|@SHA256_X86_64@|$(sum "$tar_x86")|g" -e "s|@SHA256_AARCH64@|$(sum "$tar_arm")|g" \
  packaging/PKGBUILD.in > "$dist/PKGBUILD"

# The template's @CHANGELOG@ line takes the version's section, so the Beta paragraph comes first.
printf '%s\n' "$section" > "$dist/.changelog-section"
sed -e '/^<!--$/,/^-->$/d' -e "s|@VERSION@|$version|g" packaging/release-notes.md.in | sed '/./,$!d' \
  | awk -v f="$dist/.changelog-section" '$0 == "@CHANGELOG@" { while ((getline l < f) > 0) print l; next } { print }' \
  > "$dist/RELEASE-NOTES.md"
rm -f "$dist/.changelog-section"

rm -f "$dist/SHA256SUMS"
(cd "$dist" && sha256sum $assets > SHA256SUMS)
ls -l "$dist"
