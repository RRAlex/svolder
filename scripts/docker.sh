#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Build the container image, or say why not: `make docker` runs this so the target itself
# stays a one-liner and skips cleanly on a machine with no Docker or no access to its daemon.
set -eu

command -v docker >/dev/null 2>&1 || { echo "SKIP: docker not found"; exit 0; }
docker info >/dev/null 2>&1 || { echo "SKIP: docker not accessible"; exit 0; }

# What docker-compose.yml needs from this host and cannot guess: the bluetooth group the
# container joins to reach bluetoothd. Written once; a hand-edited .env is left alone.
if [ ! -f .env ]; then
    gid=$(getent group bluetooth | cut -d: -f3)
    [ -n "$gid" ] || { echo "no bluetooth group on this host; set BLUETOOTH_GID in .env by hand"; gid=0; }
    printf 'BLUETOOTH_GID=%s\n' "$gid" > .env
    echo "wrote .env: BLUETOOTH_GID=$gid"
fi
# The two bind-mounted directories, made here so they belong to this user: Docker would
# otherwise create a missing one as root, and the container's user could not write its
# socket or its configuration into it.
mkdir -p config run
for dir in config run; do
    [ -O "$dir" ] || echo "./$dir is not owned by $(id -un); remove it (rmdir ./$dir) and run make docker again"
done
# The image carries the static binaries and their licences, built first in the build container
# from a clean tree, and the last commit's time, as the packages do.
make -s release-tree static third-party-licenses ALLOW_DIRTY="${ALLOW_DIRTY:-0}"
epoch=${SOURCE_DATE_EPOCH:?}
docker build --build-arg "UID=$(id -u)" --build-arg "VERSION=${VERSION:?}" \
    --build-arg "SOURCE_DATE_EPOCH=$epoch" \
    --build-arg "CREATED=$(date -u -d "@$epoch" +%Y-%m-%dT%H:%M:%SZ)" \
    -t svolder:local -t "svolder:$VERSION" .
