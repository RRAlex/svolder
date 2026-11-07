#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Proves the quick gate's refusal on a throwaway git repository: a change to a check that does not
# exercise the smoke is not refused, a change to a smoke check, a smoke file or the harness is.
set -uo pipefail

here=$(dirname "$(readlink -f "$0")")
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/scripts/web-checks/node/fixtures" "$tmp/bins/svolderd/web/themes/ember"
cp "$here/run-all.sh" "$tmp/scripts/web-checks/run-all.sh"
for f in scripts/web-checks/tone_note.py scripts/web-checks/effects_menu.py \
    scripts/web-checks/notes.py scripts/web-checks/node/director.mjs \
    bins/svolderd/web/themes/ember/effects.js bins/svolderd/web/themes/ember/smoke-engine.js \
    bins/svolderd/web/themes/ember/ember.js; do
    echo "// $f" >"$tmp/$f"
done
git -C "$tmp" init -q -b main
git -C "$tmp" -c user.name=t -c user.email=t@t add -A
git -C "$tmp" -c user.name=t -c user.email=t@t commit -q -m base

failed=0
# expect REFUSED|ALLOWED FILE: edits FILE (or creates it), runs the refusal, restores the tree.
expect() {
    local want=$1 f=$2 code
    echo "// edit" >>"$tmp/$f"
    WEB_CHECKS_BASE=main bash "$tmp/scripts/web-checks/run-all.sh" --refusal-only >/dev/null 2>&1
    code=$?
    git -C "$tmp" checkout -q -- . 2>/dev/null
    git -C "$tmp" clean -fdq
    if { [ "$want" = REFUSED ] && [ "$code" -eq 1 ]; } ||
        { [ "$want" = ALLOWED ] && [ "$code" -eq 0 ]; }; then
        echo "ok   $want $f (exit $code)"
    else
        echo "FAIL $want $f (exit $code)"
        failed=1
    fi
}
expect ALLOWED scripts/web-checks/tone_note.py
expect ALLOWED scripts/web-checks/effects_menu.py
expect ALLOWED scripts/web-checks/new_menu_check.py
expect ALLOWED bins/svolderd/web/themes/ember/ember.js
expect REFUSED scripts/web-checks/notes.py
expect REFUSED scripts/web-checks/node/director.mjs
expect ALLOWED scripts/web-checks/node/new.mjs
expect ALLOWED scripts/web-checks/node/fixtures/new.json
expect REFUSED bins/svolderd/web/themes/ember/smoke-engine.js
expect REFUSED bins/svolderd/web/themes/ember/effects.js
[ "$failed" -eq 0 ] && echo "gate_refusal: all cases as expected"
exit "$failed"
