#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# `make sim-app`: start svolder-sim the way the official Schiit phone app can see it. The app scans
# legacy advertising PDUs only, and bluetoothd needs --experimental and the LE Audio plugins off
# for that (docs/usage/bluetooth.md, "The simulator against the official phone app"). This checks
# the running bluetoothd's command line first, read-only, and prints the systemd drop-in that fixes
# it when something is missing; it never changes BlueZ itself.
#
#   sim-app.sh [--check-only] [--adapter hciN] [--model PROFILE]
#
# Inputs a test can replace: SIM_APP_CMDLINE (a file holding bluetoothd's NUL-separated command
# line, default /proc/<pid>/cmdline of the running bluetoothd), SIM_APP_SYSFS (default
# /sys/class/bluetooth), SIM_APP_BIN (the svolder-sim binary, default target/debug/svolder-sim).
set -u

adapter=hci0
model=gungnir2-v2
check_only=0
while [ $# -gt 0 ]; do
    case $1 in
        --check-only) check_only=1 ;;
        --adapter) adapter=${2:?--adapter needs a name}; shift ;;
        --model) model=${2:?--model needs a profile name}; shift ;;
        *) echo "sim-app.sh: unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done

sysfs=${SIM_APP_SYSFS:-/sys/class/bluetooth}
plugins="midi vcp micp bass csip mcp ccp"
problems=0

if [ ! -e "$sysfs/$adapter" ]; then
    problems=1
    echo "No Bluetooth adapter $adapter on this computer."
    present=$(ls "$sysfs" 2>/dev/null | grep '^hci[0-9][0-9]*$' | tr '\n' ' ')
    if [ -n "$present" ]; then echo "Present: $present(pick one with A=<name>)"; else echo "No adapter is present."; fi
    echo
fi

# bluetoothd's command line, one argument per line.
cmdline=${SIM_APP_CMDLINE:-}
if [ -z "$cmdline" ]; then
    for comm in /proc/[0-9]*/comm; do
        [ "$(cat "$comm" 2>/dev/null)" = bluetoothd ] || continue
        cmdline=${comm%comm}cmdline
        break
    done
fi
if [ -z "$cmdline" ] || [ ! -r "$cmdline" ]; then
    problems=1
    echo "bluetoothd is not running (or its command line is unreadable); start the Bluetooth service first."
    exit 1
fi
args=$(tr '\0' '\n' < "$cmdline")
bluetoothd=$(echo "$args" | sed -n 1p)

has_experimental=0
excluded=,
want_value=0
for arg in $args; do
    if [ "$want_value" = 1 ]; then excluded=$excluded$arg,; want_value=0; continue; fi
    case $arg in
        --experimental | -E) has_experimental=1 ;;
        --noplugin=*) excluded=$excluded${arg#--noplugin=}, ;;
        --noplugin | -P) want_value=1 ;;
    esac
done
missing=
for p in $plugins; do
    case $excluded in *",$p,"*) ;; *) missing="$missing $p" ;; esac
done

if [ "$has_experimental" = 0 ] || [ -n "$missing" ]; then
    problems=1
    echo "bluetoothd (${bluetoothd}) is not set up for the phone app:"
    [ "$has_experimental" = 1 ] || echo "  - --experimental is missing"
    [ -z "$missing" ] || echo "  - --noplugin= lacks:$missing"
    echo
    echo "Create /etc/systemd/system/bluetooth.service.d/svolder-sim.conf with this content (if you"
    echo "already have a drop-in there that sets ExecStart, edit that one instead):"
    echo
    echo "    [Service]"
    echo "    ExecStart="
    echo "    ExecStart=$bluetoothd --experimental --noplugin=$(echo $plugins | tr ' ' ',')"
    echo
    echo "then run:"
    echo
    echo "    sudo systemctl daemon-reload"
    echo "    sudo systemctl restart bluetooth"
    echo
    echo "This changes BlueZ for the whole computer: the LE Audio plugins stay off until you remove"
    echo "the drop-in. Nothing has been changed and nothing was started."
fi

[ "$problems" = 0 ] || exit 1
if [ "$check_only" = 1 ]; then
    echo "bluetoothd is set up and $adapter exists."
    exit 0
fi

echo "svolder-sim on $adapter as the model $model, passphrase 1234."
echo "Open the Schiit app: it should list svolder-sim."
echo "After restarting the sim, turn the phone's Bluetooth off and on (it caches what it saw)."
echo
exec "${SIM_APP_BIN:-target/debug/svolder-sim}" --adapter "$adapter" --layout legacy --trace --profile "$model"
