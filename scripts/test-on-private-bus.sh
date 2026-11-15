#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# `make test` on a private D-Bus daemon that has no BlueZ on it, as the release runner's system
# bus has none. DBUS_SYSTEM_BUS_ADDRESS points at that daemon for the run, so a test that assumes
# BlueZ answers fails here. The Makefile passes MAKE (the make to run); T is passed on.
. scripts/tmp-base.sh
command -v dbus-daemon >/dev/null || { echo 'make check-no-bluez: dbus-daemon is not installed' >&2; exit 1; }
bus=$(svtmp_dir bus) || exit 1
pid=''
trap '[ -z "$pid" ] || kill "$pid" 2>/dev/null; rm -rf "$bus"' EXIT
dbus-daemon --session --nofork --address="unix:path=$bus/bus" &
pid=$!
i=0
while [ ! -S "$bus/bus" ] && [ $i -lt 50 ]; do i=$((i + 1)); sleep 0.1; done
[ -S "$bus/bus" ] || { echo 'make check-no-bluez: the private bus did not start' >&2; exit 1; }
DBUS_SYSTEM_BUS_ADDRESS="unix:path=$bus/bus" "${MAKE:-make}" --no-print-directory test
