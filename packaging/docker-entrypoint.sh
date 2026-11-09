#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# The image's entrypoint, installed as `docker-entrypoint` (starts svolderd) and as `svolder`
# (the CLI, so `docker exec svolder svolder ...` runs as the right user too).
#
# The image starts as root so it can repair the two bind-mounted folders: when ./config or ./run
# did not exist, Docker created them owned by root, and the daemon could not write into them.
# A root-owned folder is handed to PUID:PGID (default 1000:1000); a folder owned by anyone else
# is never touched, and one line on stderr says who owns it. The process then drops to that user,
# keeping the supplementary groups compose's group_add gives it (the bluetooth group) but not root's
# group 0.
#
# Left alone, with the program run as it is: a start that is not root (compose `user:` set), and
# a rootless Docker or Podman, whose container root is the host user already (its uid map is not
# the identity map), where there is nothing to chown and no one to drop to.
#
# ENTRY_TEST_UID_MAP, ENTRY_TEST_DIRS and ENTRY_TEST_CLI replace the inputs for tests.
set -eu

uid_map=${ENTRY_TEST_UID_MAP:-/proc/self/uid_map}
dirs=${ENTRY_TEST_DIRS:-/config /run/svolder}

if [ "$(basename "$0")" = svolder ]; then
    prog=${ENTRY_TEST_CLI:-/usr/local/libexec/svolder}
    chown_dirs=0
else
    prog=svolderd
    chown_dirs=1
fi

[ "$(id -u)" = 0 ] || exec "$prog" "$@"

map=$(head -n 1 "$uid_map" | tr -s ' \t' '  ' | sed 's/^ //; s/ $//')
[ "$map" = "0 0 4294967295" ] || exec "$prog" "$@"

puid=${PUID:-1000}
pgid=${PGID:-1000}
if [ "$chown_dirs" = 1 ]; then
    for dir in $dirs; do
        [ -d "$dir" ] || continue
        owner=$(stat -c %u "$dir")
        if [ "$owner" = 0 ]; then
            chown -R "$puid:$pgid" "$dir"
        elif [ "$owner" != "$puid" ]; then
            echo "svolder: $dir is owned by uid $owner but the daemon runs as $puid: set HOST_UID=$owner (and HOST_GID) in .env, or chown the folder" >&2
        fi
    done
fi
# The groups compose gave the process (group_add), without root's group 0.
groups=""
for g in $(id -G); do
    [ "$g" = 0 ] || groups="${groups:+$groups,}$g"
done
if [ -n "$groups" ]; then
    set_groups="--groups=$groups"
else
    set_groups="--clear-groups"
fi
# root's home is not writable by the user it drops to.
export HOME=/tmp
exec setpriv --reuid="$puid" --regid="$pgid" "$set_groups" "$prog" "$@"
