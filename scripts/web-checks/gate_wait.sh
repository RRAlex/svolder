#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Proves the lock-wait measure (lockwait.sh, used by run-all.sh): a check that has to wait for the
# gate lock reports the wait, a check that does not reports none, and a command with no lock
# reports none. The lock is held by a throwaway `flock` on a file of its own.
set -uo pipefail

here=$(dirname "$(readlink -f "$0")")
. "$here/../tmp-base.sh"
. "$here/lockwait.sh"
tmp=$(svtmp_dir gw) || exit 1
trap 'rm -rf "$tmp"' EXIT
lock=$tmp/lock
failed=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; failed=1; }

# no lock taken: no stamp, no wait
[ "$(waited_secs "$(now_ms)" "$tmp/none")" = 0 ] && ok "no stamp: no wait" || bad "no stamp: no wait"

# a free lock: the stamp is the launch second
launched=$(now_ms)
flock -x "$lock" bash "$here/lockwait.sh" stamp "$tmp/free" true
[ "$(waited_secs "$launched" "$tmp/free")" -le 1 ] && ok "a free lock: no wait" || bad "a free lock: no wait"

# a held lock: the wait is the time the holder keeps it
# the holder owns the lock before the waiter starts: it is taken here, on fd 9, and the background
# child keeps it after this shell lets go
exec 9>"$lock"
flock -x 9
sleep 3 &
holder=$!
exec 9>&-
launched=$(now_ms)
flock -s "$lock" bash "$here/lockwait.sh" stamp "$tmp/held" true
w=$(waited_secs "$launched" "$tmp/held")
wait "$holder"
[ "$w" -ge 2 ] && ok "a held lock: waited $w s" || bad "a held lock: waited $w s"

# the stamp does not change the command's own exit status
flock -s "$lock" bash "$here/lockwait.sh" stamp "$tmp/rc" false
[ $? -ne 0 ] && ok "the command's status passes through" || bad "the command's status passes through"

[ "$failed" -eq 0 ] && echo "gate_wait: all cases as expected"
exit "$failed"
