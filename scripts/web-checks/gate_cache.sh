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
. "$(dirname "$(readlink -f "$0")")/../tmp-base.sh"
tmp=$(svtmp_dir gc) || exit 1
trap 'rm -rf "$tmp"' EXIT
c=$tmp/scripts/web-checks
mkdir -p "$c" "$tmp/bins/svolderd/web" "$tmp/target/debug" "$tmp/target/web-checks"
cp "$here/fingerprint.py" "$here/touched.py" "$c/"
printf 'import common\nimport helper\n' >"$c/a.py"
printf 'import common\n' >"$c/b.py"
printf 'x = 1\n' >"$c/common.py"
printf 'y = 1\n' >"$c/helper.py"
printf 'z = 1\n' >"$c/mutate_a.py"
printf 'daemon\n' >"$tmp/target/debug/svolderd"
printf 'tui\n' >"$tmp/target/debug/svolder"
printf '<p>page</p>\n' >"$tmp/bins/svolderd/web/index.html"
# the Rust inputs, as cargo's dep-info lists them (paths with `..` as the profiles' include_str! has)
mkdir -p "$tmp/crates/x/src" "$tmp/bins/svolderd/src" "$tmp/bins/svolderd/templates" "$tmp/profiles"
printf 'fn a() {}\n' >"$tmp/crates/x/src/lib.rs"
printf 'fn main() {}\n' >"$tmp/bins/svolderd/src/main.rs"
printf '<p>t</p>\n' >"$tmp/bins/svolderd/templates/page.html"
printf 'name = "p"\n' >"$tmp/profiles/p.toml"
printf 'wm\n' >"$tmp/bins/svolderd/web/wordmark.svg"
mkdir -p "$tmp/bins/svolderd/web/core"
printf 'v\n' >"$tmp/bins/svolderd/web/core/embedded.js"
for b in svolderd svolder; do
    printf '%s/target/debug/%s: %s/bins/svolderd/src/main.rs %s/crates/x/src/../../../profiles/p.toml %s/bins/svolderd/templates/page.html %s/crates/x/src/lib.rs %s/bins/svolderd/web/wordmark.svg %s/bins/svolderd/web/core/embedded.js\n' \
        "$tmp" "$b" "$tmp" "$tmp" "$tmp" "$tmp" "$tmp" "$tmp" >"$tmp/target/debug/$b.d"
done
# the binaries are newer than every source: a build just ran
rebuilt() { touch -d '+1 hour' "$tmp/target/debug/svolderd" "$tmp/target/debug/svolder"; }
rebuilt
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
    rebuilt
    after=$(fp check "$script" chromium "$(fp base)")
    if { [ "$want" = MOVES ] && [ "$before" != "$after" ]; } ||
        { [ "$want" = STAYS ] && [ "$before" = "$after" ]; }; then ok "$want: $what"; else bad "$want: $what"; fi
}
moves MOVES "the check file" "$c/a.py" a.py
moves MOVES "a helper it imports" "$c/helper.py" a.py
moves STAYS "a helper it does not import" "$c/helper.py" b.py
moves MOVES "the shared helper, for the check that imports it" "$c/common.py" b.py
moves STAYS "a mutation runner" "$c/mutate_a.py" a.py
moves MOVES "a Rust source of a crate" "$tmp/crates/x/src/lib.rs" b.py
moves MOVES "a Rust source of the daemon" "$tmp/bins/svolderd/src/main.rs" b.py
moves MOVES "a profile compiled into the daemon (profiles/*.toml)" "$tmp/profiles/p.toml" b.py
moves MOVES "a template compiled into the daemon" "$tmp/bins/svolderd/templates/page.html" b.py
moves MOVES "the wordmark the daemon's own code uses" "$tmp/bins/svolderd/web/wordmark.svg" b.py
e0=$(fp base); echo "# edit" >>"$tmp/bins/svolderd/web/core/embedded.js"; rebuilt
[ "$e0" = "$(fp base)" ] && ok "STAYS: a web file the daemon embeds (hashed per check, not in the base)" || bad "STAYS: a web file the daemon embeds (hashed per check, not in the base)"
moves STAYS "the built binary's bytes (the dep-info's files are what counts)" "$tmp/target/debug/svolderd" b.py
export SVOLDERD=$tmp/target/debug/svolderd
moves MOVES "a daemon binary a check is given (SVOLDERD)" "$tmp/target/debug/svolderd" b.py
unset SVOLDERD
export SMOKE_EMBEDDED=1
moves MOVES "the daemon binary when its embedded web is served" "$tmp/target/debug/svolderd" b.py
unset SMOKE_EMBEDDED
# a source newer than its binary (an edit or a merge after the build): nothing is fingerprinted
echo "// newer" >>"$tmp/crates/x/src/lib.rs"; touch -d '+2 hours' "$tmp/crates/x/src/lib.rs"
[ -z "$(fp base)" ] && ok "a source newer than the binary gives no fingerprint" || bad "a source newer than the binary gives no fingerprint"
touch "$tmp/crates/x/src/lib.rs"; rebuilt
rm "$tmp/target/debug/svolder.d"
[ -z "$(fp base)" ] && ok "a missing dep-info gives no fingerprint" || bad "a missing dep-info gives no fingerprint"
sed "s#svolderd:#svolder:#" "$tmp/target/debug/svolderd.d" >"$tmp/target/debug/svolder.d"
# the checks' fixtures (the EQ library's fake AutoEq tree) are inputs of every check
mkdir -p "$c/fixtures/autoeq"
printf 'index\n' >"$c/fixtures/autoeq/INDEX.md"
moves MOVES "a fixture file (fixtures/autoeq)" "$c/fixtures/autoeq/INDEX.md" b.py
moves MOVES "a file of the web dir" "$tmp/bins/svolderd/web/index.html" b.py
printf 'new\n' >"$tmp/bins/svolderd/web/new.js"
b1=$(fp check b.py chromium "$(fp base)")
rm "$tmp/bins/svolderd/web/new.js"
[ "$b1" != "$(fp check b.py chromium "$(fp base)")" ] && ok "MOVES: a new web file" || bad "MOVES: a new web file"
moves MOVES "the gate's run-all.sh" "$c/run-all.sh" b.py
moves MOVES "touched.py (the scope rule)" "$c/touched.py" b.py
e1=$(fp check b.py chromium "$(fp base)")
e2=$(fp check b.py firefox "$(fp base)")
[ "$e1" != "$e2" ] && ok "MOVES: the engine" || bad "MOVES: the engine"
v1=$(fp base)
export WEB_CHECKS_BROWSER_VERSION=v2
[ "$v1" != "$(fp base)" ] && ok "MOVES: the browser version" || bad "MOVES: the browser version"
export WEB_CHECKS_BROWSER_VERSION=v1

# a check with a label hashes the web files its map entry names (by area), not the whole web dir
w=$tmp/bins/svolderd/web
mkdir -p "$w/core" "$w/themes/paper" "$w/themes/signal"
for f in core/a.js themes/paper/p.js themes/paper/q.js themes/signal/s.js; do printf 'v\n' >"$w/$f"; done
printf '{"licence_1": "x", "licence_2": "y", "web_files": ["core/a.js", "themes/paper/p.js", "themes/paper/q.js", "themes/signal/s.js"], "checks": {"b-chromium": ["core/a.js", "themes/paper/p.js"], "d-chromium": ["core/a.js", "themes/signal/s.js"]}}\n' >"$c/load-map.json"
printf 'import common\n' >"$c/b.py"
lab() { fp check b.py chromium "$(fp base)" "${1:-b-chromium}"; }
labmoves() {
    local want=$1 what=$2 f=$3 before after
    before=$(lab "${4:-b-chromium}")
    echo "# edit" >>"$f"
    rebuilt
    after=$(lab "${4:-b-chromium}")
    if { [ "$want" = MOVES ] && [ "$before" != "$after" ]; } ||
        { [ "$want" = STAYS ] && [ "$before" = "$after" ]; }; then ok "$want: $what"; else bad "$want: $what"; fi
}
labmoves MOVES "a file the map names" "$w/core/a.js"
labmoves MOVES "a file of an area the map's entry reaches (a line missing from the entry)" "$w/themes/paper/q.js"
labmoves STAYS "a theme the check does not load" "$w/themes/signal/s.js"
labmoves MOVES "the same theme file for a check that loads it" "$w/themes/signal/s.js" d-chromium
printf 'n\n' >"$w/themes/paper/new.js"
labmoves MOVES "a new file of the area" "$w/themes/paper/new.js"
printf 'n\n' >"$w/orphan.js"
labmoves MOVES "a web file no check loads" "$w/orphan.js"
labmoves MOVES "a check with no map entry hashes the whole web dir" "$w/themes/signal/s.js" fresh-chromium
labmoves MOVES "a Rust source, for a mapped check" "$tmp/crates/x/src/lib.rs"
labmoves MOVES "a profile, for a mapped check" "$tmp/profiles/p.toml"
rm -f "$w/themes/paper/new.js" "$w/orphan.js"
# a stale pass is never a hit: stored, then the file edited, then looked up with the new fingerprint
f1=$(lab); fp store b-chromium "$f1"
echo "# stale" >>"$w/core/a.js"
fp hit b-chromium "$(lab)" && bad "an edited file voids a stored pass" || ok "an edited file voids a stored pass"
fp forget b-chromium

# store_pending (run-all.sh): a pass is stored only when the Rust sources, tooling and browsers are as
# they were after the build and the check's own files as they were at its start
eval "$(sed -n '/^store_pending() {/,/^}/p' "$here/run-all.sh")"
fpy=(python3 "$c/fingerprint.py")
out=$tmp/gateout; mkdir -p "$out"
fp_base=$(fp base)
pend() { printf 'sp-chromium\tb.py\tchromium\t%s\n' "$(fp check b.py chromium "$fp_base")" >"$out/pending.tsv"; fp forget sp-chromium; }
pend; store_pending
fp hit sp-chromium "$(fp check b.py chromium "$fp_base" sp-chromium)" && ok "a pending pass is stored at the end of the gate" || bad "a pending pass is stored at the end of the gate"
pend; echo "// merge after the build" >>"$tmp/crates/x/src/lib.rs"; store_pending
fp hit sp-chromium "$(fp check b.py chromium "$fp_base" sp-chromium)" && bad "a Rust edit after the build stores nothing" || ok "a Rust edit after the build stores nothing"
rebuilt; fp_base=$(fp base)
pend; echo "# edit" >>"$c/b.py"; store_pending
fp hit sp-chromium "$(fp check b.py chromium "$fp_base" sp-chromium)" && bad "a check file edited during the gate stores nothing" || ok "a check file edited during the gate stores nothing"
fp_base=$(fp base)
# a pass that needed the alone retry is never stored: run the retry loop of run-all.sh itself with
# stub checks, and see what it asks the cache to do
loop=$(sed -n '/^flaky=()/,/^done$/p' "$here/run-all.sh")
rl=$tmp/retrylog; : >"$rl"
rfpy() { echo "fpy $*" >>"$rl"; }
check_cmd() { CMD=(true); }
one_check() { echo "one_check CFP=[$CFP]" >>"$rl"; printf 'ok 1\n' >"$out/$1.verdict"; }
(
    fpy=(rfpy); out=$tmp/gateout; ordered=("rt-chromium|rt.py|chromium"); declare -A fps=([rt-chromium]=abc)
    printf 'retry 1\n' >"$out/rt-chromium.verdict"
    eval "$loop" >/dev/null
)
grep -q "fpy forget rt-chromium" "$rl" && ok "the alone retry drops the check's record" || bad "the alone retry drops the check's record"
grep -q "one_check CFP=\[\]" "$rl" && ok "the alone retry runs with no fingerprint to store" || bad "the alone retry runs with no fingerprint to store"
grep -q "fpy store" "$rl" && bad "the alone retry stores nothing" || ok "the alone retry stores nothing"
# every env var a check reads is a subset switch (a pass is never stored) or on the neutral list
python3 - "$here" <<'PY' && ok "every env var a check reads is classified" || bad "every env var a check reads is classified"
import os, re, sys
here = sys.argv[1]
sys.path.insert(0, here)
import fingerprint as f
neutral = {'PATH', 'HOME', 'SMOKE_WEB', 'SMOKE_EMBEDDED', 'SVOLDERD', 'SVOLDER', 'SVOLDER_AUTOEQ_URL', 'SVOLDER_SPEAKERS_URL',
           'SVOLDER_WEB_ASSETS', 'BASELINE_COMMIT', 'WEB_CHECKS_BASE', 'WEB_CHECKS_BROWSER_VERSION',
           'WEB_CHECKS_CACHE', 'WEB_CHECKS_ROOT'}
bad = set()
for n in os.listdir(here):
    if not n.endswith('.py') or n.startswith('mutate_') or n in ('fingerprint.py', 'touched.py'):
        continue
    names = []
    for line in open(os.path.join(here, n)).read().splitlines():
        if 'environ' in line or 'getenv' in line:
            names += re.findall(r"['\"]([A-Z][A-Z0-9_]{3,})['\"]", line)
    for name in names:
        if name not in f.SUBSET_EXACT and not name.startswith(f.SUBSET_PREFIX) and name not in neutral \
                and not name.startswith('WEB_CHECKS_'):
            bad.add(f'{n}: {name}')
for b in sorted(bad):
    print('unclassified', b)
sys.exit(1 if bad else 0)
PY
DEVICES_POPUP_ONLY=add_back fp store dp-chromium "$(fp check b.py chromium "$fp_base")"
fp hit dp-chromium "$(fp check b.py chromium "$fp_base")" && bad "a DEVICES_POPUP_ONLY run is never stored" || ok "a DEVICES_POPUP_ONLY run is never stored"

# the full gate's state: a hash that moves with a file and a directory, and a marker that round-trips
s1=$(fp state "$c/a.py" "$w")
echo "# s" >>"$w/core/a.js"
[ "$s1" != "$(fp state "$c/a.py" "$w")" ] && ok "MOVES: the state of a directory" || bad "MOVES: the state of a directory"
[ -z "$(fp marker get)" ] && ok "no marker at first" || bad "no marker at first"
fp marker set "t m"; [ "$(fp marker get)" = "t m" ] && ok "a marker round-trips" || bad "a marker round-trips"

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
