#!/bin/bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# A release dry run: builds the release packages, installs the amd64 .deb in a throwaway
# container, and starts the container image with the compose file a release attaches, as a first
# run does. `make release-dry-run` runs it. Every container, image tag and compose project it
# creates is named svolder-dryrun-<random> and removed at exit; no other container, image, tag,
# volume or network is created, changed or removed, and the web and MCP servers bind free high
# ports, never 8737 or 8738. Exit status: 0 when every step passed, else 1 with the failed step
# named on the last line.
#
# Environment: MAKE (default make), DEB_IMAGE (default: the Dockerfile's digest-pinned base image,
# which has no tag of its own), SKIP_PACKAGES=1 (reuse target/dist; for working on this script).
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
MAKE=${MAKE:-make}
pinned=$(sed -n 's/^FROM \(ubuntu:[^ ]*@sha256:[0-9a-f]*\).*/\1/p' "$root/Dockerfile")
DEB_IMAGE=${DEB_IMAGE:-$pinned}
if [ -z "$DEB_IMAGE" ]; then
    echo "release-dry-run: no pinned base image in the Dockerfile" >&2
    exit 1
fi
tag=svolder-dryrun-$(od -An -N4 -tx1 /dev/urandom | tr -d ' \n')
work=$(mktemp -d "/tmp/$tag.XXXX")
containers="$tag-deb $tag-app"
image=$tag:image
step=start

cleanup() {
    status=$?
    # A second Ctrl-C must not cut the removal short.
    trap '' INT TERM
    set +e
    docker compose -p "$tag" -f "$work/app/docker-compose.yml" -f "$work/app/override.yml" \
        --env-file "$work/app/.env" down -t 5 >/dev/null 2>&1
    for c in $containers; do
        docker rm -f "$c" >/dev/null 2>&1
    done
    docker rmi "$image" >/dev/null 2>&1
    rm -rf "$work"
    [ ! -e "$work" ] || echo "release-dry-run: $work remains (files Docker made as root?)" >&2
    if [ "$status" != 0 ]; then
        echo "release-dry-run: FAILED at step: $step" >&2
    fi
    exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

begin() {
    step=$1
    started=$(date +%s)
    echo "== $step"
}
finish() { echo "-- $step: ok ($(($(date +%s) - started)) s)"; }
fail() { echo "release-dry-run: $*" >&2; exit 1; }

command -v docker >/dev/null 2>&1 || fail "docker not found"
docker info >/dev/null 2>&1 || fail "docker not accessible"

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/Cargo.toml" | head -n 1)
dist=$root/target/dist
total_start=$(date +%s)

# 1. The release packages, built in the release container.
begin "1 make packages"
if [ "${SKIP_PACKAGES:-0}" = 1 ]; then
    echo "SKIP_PACKAGES=1: reusing $dist"
else
    $MAKE -C "$root" packages
fi
deb=$dist/svolder_${version}_amd64.deb
[ -f "$deb" ] || fail "missing $deb"
for f in svolder svolderd THIRD-PARTY-LICENSES; do
    [ -e "$dist/$f" ] || fail "missing $dist/$f"
done
finish

# 2. The .deb installed in a clean container; its binaries answer.
begin "2 install the amd64 .deb"
docker run -d --platform linux/amd64 --name "$tag-deb" "$DEB_IMAGE" sleep 900 >/dev/null
docker exec "$tag-deb" mkdir /pkg
docker cp "$deb" "$tag-deb":/pkg/svolder.deb
install='apt-get update -qq && apt-get install -y -qq /pkg/svolder.deb'
timeout 300 docker exec "$tag-deb" sh -c "$install" \
    >"$work/deb-install.log" 2>&1 || { cat "$work/deb-install.log" >&2; fail "apt install failed"; }
scoped='mkdir -p /tmp/h/c /tmp/h/s /tmp/h/d /tmp/h/k /tmp/h/r && chmod 700 /tmp/h/r;
  export HOME=/tmp/h XDG_CONFIG_HOME=/tmp/h/c XDG_STATE_HOME=/tmp/h/s XDG_DATA_HOME=/tmp/h/d \
    XDG_CACHE_HOME=/tmp/h/k XDG_RUNTIME_DIR=/tmp/h/r SVOLDER_TRANSPORT=fake \
    SVOLDER_ADAPTER=hci-none SVOLDER_SYSTEMCTL=/tmp/h/no-systemctl;'
for cmd in "svolder --version" "svolderd --version"; do
    echo "\$ $cmd"
    out=$(timeout 60 docker exec "$tag-deb" sh -c "$scoped $cmd" 2>&1) \
        || { echo "$out" >&2; fail "'$cmd' failed in the .deb container"; }
    sed -n 1,2p <<<"$out"
    grep -qF "$version" <<<"$out" || fail "'$cmd' does not name $version"
done
# `svolder doctor` exits 2 when a check fails, as the bare container's missing bluetooth does; any
# other code is a crash, and the report must be printed whole.
echo '$ svolder doctor'
rc=0
out=$(timeout 60 docker exec "$tag-deb" sh -c "$scoped svolder doctor" 2>&1) || rc=$?
sed -n 1,20p <<<"$out"
[ "$rc" -le 2 ] || fail "svolder doctor exited $rc"
grep -qF 'Config file' <<<"$out" || fail "svolder doctor printed no report"
finish

# 3. The container first run: the release image under the compose file a release attaches.
begin "3 build the release image"
epoch=$(git -C "$root" log -1 --format=%ct)
[ -n "$epoch" ] || fail "no commit time: this needs a git checkout"
docker build -q --platform linux/amd64 --build-arg "UID=$(id -u)" --build-arg "VERSION=$version" \
    --build-arg "SOURCE_DATE_EPOCH=$epoch" \
    --build-arg "CREATED=$(date -u -d "@$epoch" +%Y-%m-%dT%H:%M:%SZ)" \
    -t "$image" "$root" >/dev/null
finish

begin "4 compose first run"
app=$work/app
mkdir -p "$app"
# The release's compose file (packaging/release-compose.sh writes it), with its container name and
# image swapped for this run's and the host's bluetooth bus left unmounted (the fake device needs
# none, and the run never asks the host's BlueZ anything); the folders it binds are missing, as on
# a first run.
sh "$root/packaging/release-compose.sh" "$version" "$app/release.yml"
sed -e "s/^    container_name: svolder\$/    container_name: $tag-app/" \
    -e "s#^    image: ghcr.io/rralex/svolder:$version\$#    image: $image#" \
    -e '\#/run/dbus/system_bus_socket:/run/dbus#d' \
    "$app/release.yml" > "$app/docker-compose.yml"
grep -qxF "    container_name: $tag-app" "$app/docker-compose.yml" \
    && grep -qxF "    image: $image" "$app/docker-compose.yml" \
    || fail "the compose file's container_name or image line changed shape"
! grep -q system_bus_socket "$app/docker-compose.yml" \
    || fail "the compose file still mounts the host's system bus"
# Two free ports, for the web page and (never enabled, but pinned) the MCP server.
ports=$(python3 -c 'import random,socket
found = []
while len(found) < 2:
    p = random.randint(20000, 60000)
    s = socket.socket()
    try:
        s.bind(("127.0.0.1", p))
        found.append(p)
    except OSError:
        pass
    finally:
        s.close()
print(*found)')
read -r port mcp_port <<<"$ports"
[ -n "${port:-}" ] && [ -n "${mcp_port:-}" ] && [ "$port" != 8737 ] && [ "$port" != 8738 ] \
    && [ "$mcp_port" != 8737 ] && [ "$mcp_port" != 8738 ] || fail "no free port"
bt_gid=$(getent group bluetooth | cut -d: -f3 || true)
{
    echo "BLUETOOTH_GID=${bt_gid:-$(id -g)}"
    echo "HOST_UID=$(id -u)"
    echo "HOST_GID=$(id -g)"
    echo "COMPOSE_PROJECT_NAME=$tag"
} > "$app/.env"
# The fake device, the web page on a free port and no adapter, through the container's own
# environment: an override file, so the shipped file stays as it is. A leak never restarts.
cat > "$app/override.yml" <<YML
services:
  svolder:
    restart: "no"
    environment:
      SVOLDER_TRANSPORT: fake
      SVOLDER_ADAPTER: hci-none
      SVOLDER_WEB_ENABLED: "1"
      SVOLDER_WEB_LISTEN: "127.0.0.1:$port"
      SVOLDER_MCP_LISTEN: "127.0.0.1:$mcp_port"
YML
docker compose -p "$tag" -f "$app/docker-compose.yml" -f "$app/override.yml" \
    --env-file "$app/.env" up -d
sleep 3
state=$(docker inspect -f '{{.State.Running}}' "$tag-app")
[ "$state" = true ] || { docker logs "$tag-app" >&2; fail "the container is not running"; }
logs=$(docker logs "$tag-app" 2>&1)
tail -n 15 <<<"$logs"
grep -qF 'svolderd: listening on /run/svolder/svolder.sock' <<<"$logs" \
    || fail "docker logs does not show the daemon listening on its socket"
finish

# svolder in the container runs as the daemon's user; SVOLDER_TRANSPORT=daemon makes the CLI ask
# the daemon instead of opening its own fake device.
sv() { timeout 60 docker exec -e SVOLDER_TRANSPORT=daemon "$tag-app" svolder "$@"; }

begin "5 svolder add, then status, answer"
# The first run has no configuration: the daemon serves nothing until a device is added.
sv add 0102030405060708 --name fake || fail "svolder add failed"
out=$(sv status) || fail "svolder status failed"
sed -n 1,5p <<<"$out"
grep -qF 'Volume' <<<"$out" || fail "svolder status shows no device state"
finish

begin "6 svolder web off, then on"
# The page starts on (SVOLDER_WEB_ENABLED): off then on proves the stop and the bind again.
off=$(sv web off 2>&1) || fail "svolder web off failed: $off"
echo "$off"
[ "$off" = 'web: off' ] || fail "svolder web off answered '$off', not 'web: off'"
on=$(sv web on 2>&1) || fail "svolder web on failed: $on"
echo "$on"
grep -qF "web: listening on 127.0.0.1:$port" <<<"$on" \
    || fail "svolder web on did not report the page on 127.0.0.1:$port"
finish

begin "7 logging size cap"
cfg=$(docker inspect -f '{{json .HostConfig.LogConfig}}' "$tag-app")
echo "$cfg"
grep -qF '"max-size":"10m"' <<<"$cfg" && grep -qF '"max-file":"3"' <<<"$cfg" \
    || fail "the effective logging config lacks max-size 10m / max-file 3"
finish

echo "release-dry-run: every step passed ($(($(date +%s) - total_start)) s)"
