#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# The self-test of sim-app.sh's preflight: each case feeds it a fake bluetoothd command line and a
# fake /sys/class/bluetooth, runs `--check-only`, and compares the exit status and what it prints.
# Nothing here reads the host's BlueZ or starts anything. `make check` runs this.
set -u

here=$(cd "$(dirname "$0")" && pwd)
script=$here/sim-app.sh
tmp=$(mktemp -d "${TMPDIR:-/tmp}/sim-app-test.XXXXXX") || exit 1
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/sys/hci0" "$tmp/sys/hci1"
fails=0

# run NAME CMDLINE-ARGS... : writes the NUL-separated command line; output in $tmp/out, status in $rc.
run() {
    adapter=$1
    shift
    : > "$tmp/cmdline"
    for a in "$@"; do printf '%s\0' "$a" >> "$tmp/cmdline"; done
    SIM_APP_CMDLINE=$tmp/cmdline SIM_APP_SYSFS=$tmp/sys sh "$script" --check-only --adapter "$adapter" \
        > "$tmp/out" 2>&1
    rc=$?
}
ok() { echo "ok: $1"; }
bad() { echo "FAIL: $1"; sed 's/^/    | /' "$tmp/out"; fails=$((fails + 1)); }
has() { grep -qF -- "$1" "$tmp/out"; }

PLUGINS=midi,vcp,micp,bass,csip,mcp,ccp

run hci0 /usr/libexec/bluetooth/bluetoothd --experimental --noplugin=$PLUGINS
{ [ "$rc" = 0 ] && ! has "ExecStart"; } && ok "all flags present passes" || bad "all flags present passes (rc $rc)"

run hci0 /usr/libexec/bluetooth/bluetoothd --noplugin=vcp,micp,bass,csip,mcp,ccp,midi,extra -E
{ [ "$rc" = 0 ]; } && ok "short -E, any order, a superset passes" || bad "short -E, any order, a superset (rc $rc)"

run hci0 /usr/libexec/bluetooth/bluetoothd -E -P midi,vcp -P micp,bass,csip,mcp,ccp
{ [ "$rc" = 0 ]; } && ok "-P twice, separate values pass" || bad "-P twice (rc $rc)"

run hci0 /usr/libexec/bluetooth/bluetoothd --noplugin=$PLUGINS
if [ "$rc" != 0 ] && has "--experimental" && has "/etc/systemd/system/bluetooth.service.d/" \
    && has "ExecStart=/usr/libexec/bluetooth/bluetoothd --experimental --noplugin=$PLUGINS" \
    && has "sudo systemctl daemon-reload" && has "sudo systemctl restart bluetooth"; then
    ok "missing --experimental prints the drop-in and fails"
else bad "missing --experimental prints the drop-in and fails (rc $rc)"; fi

run hci0 /usr/libexec/bluetooth/bluetoothd --experimental --noplugin=midi,vcp,micp,bass,csip,mcp
if [ "$rc" != 0 ] && has "ccp" && has "ExecStart=" && has "sudo systemctl restart bluetooth"; then
    ok "one missing plugin names it, prints the drop-in and fails"
else bad "one missing plugin (rc $rc)"; fi

run hci0 /opt/bz/bluetoothd --experimental
if [ "$rc" != 0 ] && has "ExecStart=/opt/bz/bluetoothd --experimental --noplugin=$PLUGINS"; then
    ok "the drop-in uses the running bluetoothd's path"
else bad "the drop-in uses the running path (rc $rc)"; fi

run hci7 /usr/libexec/bluetooth/bluetoothd --experimental --noplugin=$PLUGINS
if [ "$rc" != 0 ] && has "hci7" && has "hci0" && has "hci1"; then
    ok "missing adapter is named and the present ones listed"
else bad "missing adapter (rc $rc)"; fi

rm -f "$tmp/cmdline"
SIM_APP_CMDLINE=$tmp/cmdline SIM_APP_SYSFS=$tmp/sys sh "$script" --check-only --adapter hci0 > "$tmp/out" 2>&1
rc=$?
{ [ "$rc" != 0 ] && has "not running"; } && ok "no bluetoothd says so and fails" || bad "no bluetoothd (rc $rc)"

[ "$fails" = 0 ] && echo "sim-app-test: all passed" || { echo "sim-app-test: $fails failed"; exit 1; }
