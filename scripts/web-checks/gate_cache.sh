#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Proves the gate's result cache (fingerprint.py, and run-all.sh's use of it) on a throwaway tree:
# the fingerprint moves with each input a check reads and stays put for what it does not read, a
# stored pass is a hit only for the same fingerprint, and the gate reports a cached check and runs
# a changed one.
set -uo pipefail

here=$(dirname "$(readlink -f "$0")")
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
c=$tmp/scripts/web-checks
mkdir -p "$c" "$tmp/bins/svolderd/web" "$tmp/target/debug" "$tmp/target/web-checks"
cp "$here/fingerprint.py" "$c/"
printf 'import common\nimport helper\n' >"$c/a.py"
printf 'import common\n' >"$c/b.py"
printf 'x = 1\n' >"$c/common.py"
printf 'y = 1\n' >"$c/helper.py"
printf 'z = 1\n' >"$c/mutate_a.py"
printf 'daemon\n' >"$tmp/target/debug/svolderd"
printf 'tui\n' >"$tmp/target/debug/svolder"
printf '<p>page</p>\n' >"$tmp/bins/svolderd/web/index.html"
export WEB_CHECKS_ROOT=$tmp WEB_CHECKS_BROWSER_VERSION=v1
unset SMOKE_WEB SVOLDERD SVOLDER SMOKE_EMBEDDED WEB_CHECKS_CACHE
fp() { python3 "$c/fingerprint.py" "$@"; }

failed=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; failed=1; }
# moves WHAT FILE ENGINE SCRIPT: editing FILE changes (or not) SCRIPT's fingerprint
moves() {
    local want=$1 what=$2 f=$3 script=$4 before after
    before=$(fp check "$script" chromium "$(fp base)")
    echo "# edit" >>"$f"
    after=$(fp check "$script" chromium "$(fp base)")
    if { [ "$want" = MOVES ] && [ "$before" != "$after" ]; } ||
        { [ "$want" = STAYS ] && [ "$before" = "$after" ]; }; then ok "$want: $what"; else bad "$want: $what"; fi
}
moves MOVES "the check file" "$c/a.py" a.py
moves MOVES "a helper it imports" "$c/helper.py" a.py
moves STAYS "a helper it does not import" "$c/helper.py" b.py
moves MOVES "the shared helper, for the check that imports it" "$c/common.py" b.py
moves STAYS "a mutation runner" "$c/mutate_a.py" a.py
moves MOVES "the daemon binary" "$tmp/target/debug/svolderd" b.py
moves MOVES "the TUI binary" "$tmp/target/debug/svolder" b.py
moves MOVES "a file of the web dir" "$tmp/bins/svolderd/web/index.html" b.py
printf 'new\n' >"$tmp/bins/svolderd/web/new.js"
b1=$(fp base)
rm "$tmp/bins/svolderd/web/new.js"
[ "$b1" != "$(fp base)" ] && ok "MOVES: a new web file" || bad "MOVES: a new web file"
e1=$(fp check b.py chromium "$(fp base)")
e2=$(fp check b.py firefox "$(fp base)")
[ "$e1" != "$e2" ] && ok "MOVES: the engine" || bad "MOVES: the engine"
v1=$(fp base)
export WEB_CHECKS_BROWSER_VERSION=v2
[ "$v1" != "$(fp base)" ] && ok "MOVES: the browser version" || bad "MOVES: the browser version"
export WEB_CHECKS_BROWSER_VERSION=v1

# the closure follows real imports and opens, not names in comments or docstrings
printf '# see helper2.py and mutate_a.py\nimport common\n' >"$c/d.py"
printf 'w = 1\n' >"$c/helper2.py"
moves STAYS "a file only named in a comment" "$c/helper2.py" d.py
printf 'import common\nopen("helper2.py")\n' >"$c/e.py"
moves MOVES "a file the check opens" "$c/helper2.py" e.py
printf 'import helper3\n' >"$c/f.py"
printf 'import helper2\n' >"$c/helper3.py"
moves MOVES "a helper's own import (transitive)" "$c/helper2.py" f.py
# a subset env moves the fingerprint and stops a pass being stored
b0=$(fp base)
b1=$(ND_ONLY=pace fp base)
[ "$b0" != "$b1" ] && ok "MOVES: a subset env var" || bad "MOVES: a subset env var"
fp store sub-chromium "$(fp check a.py chromium "$b0")"; fp hit sub-chromium "$(fp check a.py chromium "$b0")" && ok "a whole run is stored" || bad "a whole run is stored"
fp forget sub-chromium
ND_ONLY=pace fp store sub-chromium "$(fp check a.py chromium "$b1")"
fp hit sub-chromium "$(fp check a.py chromium "$b1")" && bad "a subset run is never stored" || ok "a subset run is never stored"
fp hit sub-chromium "$(fp check a.py chromium "$b0")" && bad "a subset pass does not match a whole run" || ok "a subset pass does not match a whole run"
# an atomic store leaves no temp file
ls "$tmp/target/web-checks/cache" | grep -q tmp && bad "no temp file left" || ok "no temp file left"

# an opened .json is in the closure (json is not cut to js)
printf '{}\n' >"$c/base.json"
printf 'import common\nopen("base.json")\n' >"$c/g.py"
moves MOVES "a .json file the check opens" "$c/base.json" g.py
# a browser whose version cannot be read: no fingerprint, nothing stored
mkdir -p "$tmp/badbin"
for n in chromium firefox geckodriver; do printf '#!/bin/sh\nexit 1\n' >"$tmp/badbin/$n"; chmod +x "$tmp/badbin/$n"; done
unset WEB_CHECKS_BROWSER_VERSION
nb=$(PATH="$tmp/badbin:$PATH" python3 "$c/fingerprint.py" base)
export WEB_CHECKS_BROWSER_VERSION=v1
[ -z "$nb" ] && ok "an unreadable browser version gives no base fingerprint" || bad "an unreadable browser version gives no base fingerprint"
PATH="$tmp/badbin:$PATH" fp check a.py chromium "$nb" >/dev/null && bad "no fingerprint for a check without a base" || ok "no fingerprint for a check without a base"
fp store unk-chromium "" ; fp hit unk-chromium "" && bad "an empty fingerprint is never stored" || ok "an empty fingerprint is never stored"

f=$(fp check a.py chromium "$(fp base)")
fp hit a.py-chromium "$f" && bad "a check with no record is not a hit" || ok "a check with no record is not a hit"
fp store a.py-chromium "$f"
fp hit a.py-chromium "$f" && ok "a stored pass is a hit" || bad "a stored pass is a hit"
echo "# edit" >>"$c/a.py"
fp hit a.py-chromium "$(fp check a.py chromium "$(fp base)")" && bad "a changed check is a miss" || ok "a changed check is a miss"
fp store a.py-chromium "$f"
fp forget a.py-chromium
fp hit a.py-chromium "$f" && bad "a forgotten record is a miss" || ok "a forgotten record is a miss"

[ "$failed" -eq 0 ] && echo "gate_cache: all cases as expected"
exit "$failed"
