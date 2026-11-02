#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Writes the compose file a release attaches: docker-compose.yml with its `image:` line changed
# to the version's image on ghcr.io, and the comment above that line (which describes the image
# `make docker` builds) replaced by one for the published image. Usage: release-compose.sh
# VERSION [OUT]; OUT defaults to target/dist/docker-compose.yml, relative to the current directory.
# The repository's docker-compose.yml is found beside this script's folder, so any directory works.
set -eu

version=${1:?usage: release-compose.sh VERSION [OUT]}
out=${2:-target/dist/docker-compose.yml}
src=$(dirname "$0")/../docker-compose.yml

[ "$(grep -c '^    image: svolder:local$' "$src")" = 1 ] \
  && [ "$(grep -c '^    # The image `make docker` builds' "$src")" = 1 ] || {
  echo "release-compose: docker-compose.yml no longer has the image line and its comment this script replaces" >&2
  exit 1
}
mkdir -p "$(dirname "$out")"
# The comment runs from its first line to the line before `image:`.
awk -v image="ghcr.io/rralex/svolder:$version" '
  /^    # The image `make docker` builds/ {
    print "    # The published image, running as uid 1000: create the two bind-mounted directories"
    print "    # owned by that uid first (`mkdir -p config run && sudo chown 1000:1000 config run`),"
    print "    # then `docker compose up -d`. To update: `docker compose pull`, then `docker compose up -d`."
    skip = 1; next
  }
  /^    image: svolder:local$/ { print "    image: " image; skip = 0; next }
  !skip { print }
' "$src" > "$out"
