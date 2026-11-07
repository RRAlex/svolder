#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Build the container image, or say why not: `make docker` runs this so the target itself
# stays a one-liner and skips cleanly on a machine with no Docker or no access to its daemon.
set -eu

# `--env-only` writes the .env keys in the current directory and stops: the part that needs no
# Docker, so a test can run it anywhere.
env_only=0
[ "${1:-}" = --env-only ] && env_only=1
if [ "$env_only" = 0 ]; then
    command -v docker >/dev/null 2>&1 || { echo "SKIP: docker not found"; exit 0; }
    docker info >/dev/null 2>&1 || { echo "SKIP: docker not accessible"; exit 0; }
fi

# What docker-compose.yml needs from this host and cannot guess: the bluetooth group the
# container joins to reach bluetoothd, and the user's id and runtime directory, where the
# PipeWire socket the computer's volume and EQ use lives (read by the host-audio override).
# Each key is appended when .env lacks it; a line already there, hand-edited or not, is never rewritten.
[ -f .env ] || : > .env
# A last line with no newline would swallow the first key appended after it.
[ -z "$(tail -c1 .env)" ] || echo >> .env
env_has() { grep -q "^$1=" .env; }
if ! env_has BLUETOOTH_GID; then
    gid=$(getent group bluetooth | cut -d: -f3)
    [ -n "$gid" ] || { echo "no bluetooth group on this host; set BLUETOOTH_GID in .env by hand"; gid=0; }
    printf 'BLUETOOTH_GID=%s\n' "$gid" >> .env
    echo "wrote .env: BLUETOOTH_GID=$gid"
fi
if ! env_has HOST_UID; then
    printf 'HOST_UID=%s\n' "$(id -u)" >> .env
    echo "wrote .env: HOST_UID=$(id -u)"
fi
if ! env_has HOST_RUNTIME_DIR; then
    printf 'HOST_RUNTIME_DIR=%s\n' "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}" >> .env
    echo "wrote .env: HOST_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
fi
# The computer's volume and EQ are an override file, switched on here when this host runs
# PipeWire (its socket exists) or DOCKER_HOST_AUDIO=1 asks; a COMPOSE_FILE already in .env stays.
if ! env_has COMPOSE_FILE; then
    runtime=$(grep '^HOST_RUNTIME_DIR=' .env | tail -n 1 | cut -d= -f2-)
    if [ -S "$runtime/pipewire-0" ] || [ "${DOCKER_HOST_AUDIO:-0}" = 1 ]; then
        printf 'COMPOSE_FILE=docker-compose.yml:docker-compose.host-audio.yml\n' >> .env
        echo "wrote .env: COMPOSE_FILE with the computer's volume and EQ (docker-compose.host-audio.yml)"
    fi
fi
[ "$env_only" = 0 ] || exit 0
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
