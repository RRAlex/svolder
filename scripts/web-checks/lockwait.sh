#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# How long a check waited for the gate lock, kept apart from how long it ran (run-all.sh).
#
#   lockwait.sh stamp FILE COMMAND...   write the epoch millisecond into FILE, then run COMMAND; run-all.sh
#                                       puts it after `flock` on the command line, so the stamp is
#                                       the second the lock was won
#   . lockwait.sh; waited_secs LAUNCHED_MS FILE
#                                       whole seconds from the launch epoch (milliseconds, see
#                                       now_ms) to the stamp (0 when there is no stamp: no lock was
#                                       taken)
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    [ "${1:-}" = stamp ] && [ $# -ge 3 ] || { echo "usage: lockwait.sh stamp FILE COMMAND..." >&2; exit 2; }
    printf '%s\n' "$(date +%s%3N)" >"$2"
    shift 2
    exec "$@"
fi

waited_secs() {
    local launched=$1 stamp=$2 got
    got=$(cat "$stamp" 2>/dev/null) || got=
    case $got in '' | *[!0-9]*) echo 0; return ;; esac
    if [ "$got" -gt "$launched" ]; then echo $(((got - launched) / 1000)); else echo 0; fi
}

# now_ms: the epoch in milliseconds
# (the separator of EPOCHREALTIME follows the locale: a dot or a comma)
now_ms() { local t=$EPOCHREALTIME; t=${t/[.,]/}; echo $((10#$t / 1000)); }
