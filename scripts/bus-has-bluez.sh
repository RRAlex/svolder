#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Asks the system bus (DBUS_SYSTEM_BUS_ADDRESS when set) whether org.bluez is there: running now
# (ListNames) or startable (ListActivatableNames, so BlueZ installed but stopped counts).
# Exit 0: it is; 1: it is not, either way; 2: cannot tell (no dbus-send, no bus, a failed call).
command -v dbus-send >/dev/null 2>&1 || exit 2
ask() {
  dbus-send --system --print-reply --dest=org.freedesktop.DBus /org/freedesktop/DBus \
    "org.freedesktop.DBus.$1" 2>/dev/null
}
running=$(ask ListNames) || exit 2
startable=$(ask ListActivatableNames) || exit 2
# A reply with no names at all is no answer worth trusting.
case $running in *'"org.freedesktop.DBus"'*) ;; *) exit 2 ;; esac
case "$running$startable" in *'"org.bluez"'*) exit 0 ;; esac
exit 1
