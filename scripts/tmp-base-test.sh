#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
# Self-test of scripts/tmp-base.sh and scripts/tmp_base.py on planted bases under a scratch directory
# of its own (never the real /tmp/svolder).
here=$(cd "$(dirname "$0")" && pwd)
. "$here/tmp-base.sh"
root=$(svtmp_dir tbt) || exit 1
trap 'rm -rf "$root"' EXIT
fails=0
ok() { echo "tmp-base-test: ok: $1"; }
bad() { echo "tmp-base-test: FAIL: $1" >&2; fails=$((fails + 1)); }

# expect KIND NAME CMD...: CMD must succeed (ok) or fail (refuse)
expect() {
  kind=$1 name=$2; shift 2
  if "$@" >/dev/null 2>&1; then got=ok; else got=refuse; fi
  if [ "$got" = "$kind" ]; then ok "$name"; else bad "$name (wanted $kind, got $got)"; fi
}
sh_dir() { (SVOLDER_TMP_BASE=$1; . "$here/tmp-base.sh"; svtmp_dir t); }
py_dir() { SVOLDER_TMP_BASE=$1 python3 -I -c "import sys; sys.path.insert(0, sys.argv[1]); import tmp_base; tmp_base.mkdtemp('t')" "$here"; }

for lang in sh py; do
  f=${lang}_dir
  expect ok "$lang: an absent base is made" $f "$root/$lang-new"
  [ "$(stat -c %a "$root/$lang-new")" = 700 ] && ok "$lang: made 0700" || bad "$lang: made 0700"
  mkdir -m 755 "$root/$lang-open"
  expect ok "$lang: an own 0755 base is set to 0700" $f "$root/$lang-open"
  [ "$(stat -c %a "$root/$lang-open")" = 700 ] && ok "$lang: mode fixed" || bad "$lang: mode fixed"
  mkdir -m 700 "$root/$lang-real"
  ln -s "$root/$lang-real" "$root/$lang-link"
  expect refuse "$lang: a symlink base" $f "$root/$lang-link"
  : >"$root/$lang-file"
  expect refuse "$lang: a file base" $f "$root/$lang-file"
  if [ "$(id -u)" != 0 ]; then
    expect refuse "$lang: a base owned by someone else" $f /usr
  fi
done
log=$root/made.log
d=$(SVOLDER_TMP_LOG=$log sh_dir "$root/logged")
[ "$(cat "$log" 2>/dev/null)" = "$d" ] && ok "the manifest lists the dir made" || bad "the manifest lists the dir made"
[ "$fails" -eq 0 ] || exit 1
echo "tmp-base-test: the base is made 0700 and refused when planted"
