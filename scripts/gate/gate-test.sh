#!/bin/bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
# Self-test of scripts/gate/gate.sh's keys and stamps on a throwaway git repository (never the real
# target/gate-stamps): same tree reuses, a listed path reruns, an unlisted one reuses, a dirty tree
# reruns, GATE_FRESH reruns, a failure leaves no stamp.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/../tmp-base.sh"
tmp=$(svtmp_dir gt) || exit 1
trap 'rm -rf "$tmp"' EXIT
# the test's own git must not see the caller's repository or configuration
for v in $(compgen -e); do
    case $v in GIT_* | SVOLDER_* | RUSTFLAGS | RUSTDOCFLAGS | CARGO_ENCODED_* | CARGO_PROFILE_* | CARGO_BUILD_* | RUSTC* | RUST_TEST_* | RUST_MIN_STACK | RUST_BACKTRACE | MAKEFLAGS | MFLAGS | MAKELEVEL | T | NEXTEST) unset "$v" ;; esac
done
unset CARGO
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
r=$tmp/repo
mkdir -p "$r/in" "$r/out"
git -C "$r" init -q
git -C "$r" config user.email t@example.invalid
git -C "$r" config user.name t
echo 1 >"$r/in/a"; echo 1 >"$r/out/b"
git -C "$r" add -A
git -C "$r" commit -q -m one
cat >"$tmp/checks.conf" <<CONF
good | yes | p=\${P:-1} | echo ran >>$tmp/runs | in
bad  | yes | - | echo ran >>$tmp/runs; false | in
free | no | - | echo ran >>$tmp/runs | in
CONF
export GATE_ROOT=$r GATE_MANIFEST=$tmp/checks.conf GATE_STAMPS=$tmp/stamps GATE_RUSTC_VERSION=rustc-test
unset GATE_FRESH GATE_CHECKS GATE_REPORT
gate() { bash "$here/gate.sh" "$@" >"$tmp/out.txt" 2>&1; }
stamps_n() { ls "$tmp/stamps" 2>/dev/null | grep -c "^$1-" || true; }
runs() { [ -f "$tmp/runs" ] && wc -l <"$tmp/runs" || echo 0; }
failed=0
ok() { echo "gate-test: ok: $1"; }
bad() { echo "gate-test: FAIL: $1" >&2; failed=1; }
# expect NAME WANT: the number of runs so far is WANT
expect() { [ "$(runs)" = "$2" ] && ok "$1" || bad "$1 (runs $(runs), wanted $2)"; }

gate run good;                  expect "first run executes" 1
gate run good;                  expect "same tree is reused" 1
grep -q 'reused (passed at' "$tmp/out.txt" && ok "reuse names the commit" || bad "reuse names the commit"
grep -q 'reused' "$tmp/out.txt" && ok "table says reused" || bad "table says reused"
echo 2 >"$r/out/b"; git -C "$r" commit -q -am two
gate run good;                  expect "a change outside the paths is reused" 1
echo 2 >"$r/in/a"; git -C "$r" commit -q -am three
gate run good;                  expect "a change in a listed path reruns" 2
gate run good;                  expect "and is reused again after" 2
P=2 gate run good;              expect "another parameter reruns" 3
echo 3 >"$r/in/a"
gate run good;                  expect "a dirty tree reruns" 4
gate run good;                  expect "a dirty tree reruns again (never stamped)" 5
git -C "$r" commit -q -am four
gate run good;                  expect "committed, it runs once" 6
gate run good;                  expect "then is reused" 6
GATE_FRESH=1 gate run good;     expect "GATE_FRESH reruns" 7
echo x >"$r/in/new"
gate run good;                  expect "an untracked file in a path reruns" 8
rm "$r/in/new"
gate run bad;                   expect "a failing check runs" 9
ls "$tmp/stamps"/bad-* >/dev/null 2>&1 && bad "a failure left a stamp" || ok "a failure leaves no stamp"
gate run bad && bad "a failure fails the gate" || ok "a failure fails the gate"
gate run free; gate run free;   expect "a STAMP=no check always runs" 12
GATE_REPORT=$tmp/report gate run good
grep -q '^good ' "$tmp/report" && ok "GATE_REPORT gets the table" || bad "GATE_REPORT gets the table"

# deleted and staged-only changes under a path
git -C "$r" checkout -q -- in 2>/dev/null; git -C "$r" reset -q --hard
n=$(runs)
rm "$r/in/a"
gate run good;                  expect "a deleted file under a path reruns" $((n + 1))
git -C "$r" checkout -q -- in/a
echo staged >"$r/in/a"; git -C "$r" add in/a
gate run good;                  expect "a staged-only change reruns" $((n + 2))
git -C "$r" reset -q --hard
gate run good;                  expect "restored tree: reused (stamped before)" $((n + 2))
gate run good;                  expect "restored tree: reused again" $((n + 2))
# the rustc version is in the key
GATE_RUSTC_VERSION=rustc-other gate run good; expect "a rustc change reruns" $((n + 3))
# the command text is in the key
sed -i "s#^good .*#good | yes | p=\${P:-1} | echo ran >>$tmp/runs; true | in#" "$tmp/checks.conf"
gate run good;                  expect "a changed command reruns" $((n + 4))
# the tree changing during the run: no stamp, and the next gate runs again
cat >>"$tmp/checks.conf" <<CONF
edits | yes | - | echo ran >>$tmp/runs; echo 7 >>$r/in/a | in
commits | yes | - | echo ran >>$tmp/runs; git -C $r commit -q --allow-empty -m mid; echo more >>$r/in/c; git -C $r add in/c; git -C $r commit -q -m mid2 | in
CONF
n=$(runs)
gate run edits
[ "$(stamps_n edits)" = 0 ] && ok "an edit during the run leaves no stamp" || bad "an edit during the run leaves no stamp"
grep -q 'tree changed\|dirty' "$tmp/out.txt" && ok "and says so" || bad "and says so"
git -C "$r" checkout -q -- in/a
gate run commits
[ "$(stamps_n commits)" = 0 ] && ok "a commit during the run leaves no stamp" || bad "a commit during the run leaves no stamp"
# git failing: never reuse, never stamp
gate run good
n=$(runs)
GIT_DIR=/nonexistent gate run good;  expect "a bogus GIT_DIR still keys on the real tree" $n
bare=$tmp/nogit; mkdir -p "$bare/in"; echo 1 >"$bare/in/a"
before=$(stamps_n good)
GATE_ROOT=$bare gate run good;  expect "no repository: runs" $((n + 1))
GATE_ROOT=$bare gate run good;  expect "no repository: runs again, never reused" $((n + 2))
[ "$(stamps_n good)" = "$before" ] && ok "no repository: no stamp" || bad "no repository: no stamp"
GATE_ROOT=$bare gate key good && bad "no repository: no key" || ok "no repository: no key"
mkdir "$tmp/unborn"; git -C "$tmp/unborn" init -q; mkdir "$tmp/unborn/in"
GATE_ROOT=$tmp/unborn gate run good; GATE_ROOT=$tmp/unborn gate run good
expect "unborn HEAD: runs every time" $((n + 4))
# make variables, flags and environment that change what runs
n=$(runs); before=$(stamps_n good)
T='-p x' gate run good;                         expect "T set: runs" $((n + 1))
T='-p x' gate run good;                         expect "T set: never reused" $((n + 2))
MAKEFLAGS=' -- T=-p\ x' MFLAGS='' gate run good;          expect "a T override on the make command line: runs" $((n + 3))
MAKEFLAGS='i' MFLAGS='-i' gate run good;        expect "make -i: runs" $((n + 4))
MAKEFLAGS='k' MFLAGS='-k' gate run good; expect "make -k: runs" $((n + 5))
CARGO='cargo +nightly' gate run good;           expect "CARGO changed: runs" $((n + 6))
RUSTFLAGS=-Cdebuginfo=0 gate run good;          expect "RUSTFLAGS set: runs" $((n + 7))
( v=SVOLDER; export "${v}_FAKE_Z=1"; gate run good ); expect "an SVOLDER_ variable set: runs" $((n + 8))
[ "$(stamps_n good)" = "$before" ] && ok "none of those stamped" || bad "none of those stamped"
# the strings GNU make 4.4.1 exports (MAKEFLAGS and MFLAGS both), from a throwaway Makefile run with
# -j6, -s, SEED=, GATE_CHECKS= and -s -j6 together
gate_real() { MAKEFLAGS=$1 MFLAGS=$2 gate run good; }
gate_real ' -j6 --jobserver-auth=fifo:/tmp/GMfifo1' '-j6 --jobserver-auth=fifo:/tmp/GMfifo1'; expect "make -j6 still reuses" $((n + 8))
gate_real 's' '-s';                                        expect "make -s still reuses" $((n + 8))
gate_real 's -j6 --jobserver-auth=fifo:/tmp/GMfifo2 -- SEED=7 GATE_CHECKS=a\ b' '-s -j6 --jobserver-auth=fifo:/tmp/GMfifo2'; expect "make -s -j6 SEED= GATE_CHECKS= still reuses" $((n + 8))
gate_real 's --no-print-directory' '-s --no-print-directory'; expect "make --no-print-directory still reuses" $((n + 8))
gate_real 'i -- T=-p\ x' '-i';                             expect "-i with a T override runs" $((n + 9))
gate_real ' -j6 --jobserver-auth=fifo:/tmp/GMfifo3 -- FOO=1' '-j6 --jobserver-auth=fifo:/tmp/GMfifo3'; expect "make -j6 FOO=1 runs" $((n + 10))
# an edit made and reverted during the run: no stamp
cat >>"$tmp/checks.conf" <<CONF
reverts | yes | - | echo ran >>$tmp/runs; cp $r/in/a $tmp/save; echo 9 >$r/in/a; cp $tmp/save $r/in/a | in
CONF
gate run reverts
[ "$(stamps_n reverts)" = 0 ] && ok "an edit reverted during the run leaves no stamp" || bad "an edit reverted during the run leaves no stamp"
git -C "$r" status --porcelain | grep -q . && bad "revert case left the tree dirty"
# a whole-tree check: target/ and __pycache__ writes still stamp, an edit-then-revert does not
w=$tmp/whole; mkdir -p "$w/src" "$w/target"
git -C "$w" init -q; git -C "$w" config user.email t@example.invalid; git -C "$w" config user.name t
printf "target/\n__pycache__/\n" >"$w/.gitignore"; echo 1 >"$w/src/a"
git -C "$w" add -A; git -C "$w" commit -q -m one
cat >"$tmp/whole.conf" <<CONF
whole | yes | - | echo ran >>$tmp/wruns; mkdir -p $w/target/x $w/src/__pycache__; echo o >$w/target/x/out; echo p >$w/src/__pycache__/m.pyc | .
wrevert | yes | - | echo ran >>$tmp/wruns; cp $w/src/a $tmp/wsave; echo 9 >$w/src/a; cp $tmp/wsave $w/src/a | .
CONF
GATE_ROOT=$w GATE_MANIFEST=$tmp/whole.conf GATE_STAMPS=$tmp/wstamps bash "$here/gate.sh" run whole >"$tmp/out.txt" 2>&1
[ "$(ls "$tmp/wstamps" 2>/dev/null | grep -c '^whole-')" = 1 ] && ok "a whole-tree check that writes target/ and __pycache__ is stamped" || bad "a whole-tree check that writes target/ and __pycache__ is stamped"
GATE_ROOT=$w GATE_MANIFEST=$tmp/whole.conf GATE_STAMPS=$tmp/wstamps bash "$here/gate.sh" run whole >"$tmp/out.txt" 2>&1
grep -q 'reused' "$tmp/out.txt" && ok "and is then reused" || bad "and is then reused"
GATE_ROOT=$w GATE_MANIFEST=$tmp/whole.conf GATE_STAMPS=$tmp/wstamps bash "$here/gate.sh" run wrevert >"$tmp/out.txt" 2>&1
[ "$(ls "$tmp/wstamps" 2>/dev/null | grep -c '^wrevert-')" = 0 ] && ok "a whole-tree edit then revert leaves no stamp" || bad "a whole-tree edit then revert leaves no stamp"
# the make a check's scripts get in MAKE is a plain command name (Makefile passes it on to scripts that run it alone)
cat >"$tmp/make.conf" <<CONF
mk | yes | - | case "\$MAKE" in *' '*) exit 1 ;; esac | in
CONF
GATE_MANIFEST=$tmp/make.conf gate run mk && ok "MAKE stays a plain command for the checks" || bad "MAKE stays a plain command for the checks"
# the real manifest
real=$here/checks.conf
bad_paths=''
grep -v '^#' "$real" | grep -q '\$MAKE' && bad "the manifest uses \$MAKE, not \$GMAKE" || ok "the manifest uses \$GMAKE"
dups=$(grep -v '^#' "$real" | grep -v '^$' | cut -d'|' -f1 | sed 's/ *$//' | sort | uniq -d)
[ -z "$dups" ] && ok "manifest names are unique" || bad "duplicate manifest names: $dups"
badlines=$(grep -v '^#' "$real" | grep -v '^$' | awk -F'|' 'NF != 5' | wc -l)
[ "$badlines" = 0 ] && ok "every manifest line has five fields" || bad "$badlines manifest lines do not have five fields"
repo=$(cd "$here/../.." && pwd)
while IFS='|' read -r name _ _ _ paths; do
    for p in $paths; do
        if [ "$p" = . ]; then spec='HEAD^{tree}'; else spec="HEAD:$p"; fi
        git -C "$repo" rev-parse --verify -q "$spec" >/dev/null 2>&1 || bad_paths="$bad_paths $name:$p"
    done
done < <(grep -v '^#' "$real" | grep -v '^$')
[ -z "$bad_paths" ] && ok "every path in the manifest exists at HEAD" || bad "manifest paths missing at HEAD:$bad_paths"
def=$(GATE_ROOT=$repo bash "$here/gate.sh" list)
case " $(echo $def) " in *preamp-lab-fresh*) bad "the default list has the fresh lab" ;; *) ok "the default list leaves out the fresh lab" ;; esac
msg=$(GATE_ROOT=$repo GATE_MANIFEST=$real bash "$here/gate.sh" key preamp-lab-fresh 2>&1)
case $msg in *'is never stamped'*) ok "the fresh lab is never stamped" ;; *) bad "the fresh lab is not reported as never stamped" ;; esac
# the lab's path list covers every workspace crate svolder-host depends on, transitively
if command -v cargo >/dev/null 2>&1; then
    missing=$(cargo metadata --no-deps --format-version 1 --offline --manifest-path "$repo/Cargo.toml" 2>/dev/null | python3 -I -c '
import json, os, sys
m = json.load(sys.stdin)
repo = sys.argv[1]
pk = {p["name"]: p for p in m["packages"]}
seen, todo = set(), ["svolder-host"]
while todo:
    n = todo.pop()
    if n in seen or n not in pk:
        continue
    seen.add(n)
    todo += [d["name"] for d in pk[n]["dependencies"] if d["name"] in pk]
listed = sys.argv[2].split()
for n in sorted(seen):
    d = os.path.relpath(os.path.dirname(pk[n]["manifest_path"]), repo)
    if not any(d == q or d.startswith(q.rstrip("/") + "/") or q == "." for q in listed):
        print(d)
' "$repo" "$(grep '^preamp-lab ' "$real" | cut -d'|' -f5)") || missing=cargo-metadata-failed
    [ -z "$missing" ] && ok "the lab's paths cover svolder-host's workspace dependencies" || bad "the lab's paths miss: $missing"
else
    ok "cargo not installed: lab dependency check skipped"
fi
[ $failed = 0 ] && echo "gate-test: all ok"
exit $failed
