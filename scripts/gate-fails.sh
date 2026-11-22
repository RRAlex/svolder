#!/bin/bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
# Every check that is still red at the end of a web gate, with its FAIL lines.
# Usage: gate-fails.sh <gate.log>   (exit 1 when something is red)
#        gate-fails.sh --self-test  (a here-doc log, no browser)
fails() {
  awk '
    /^(ok|FAIL|retry|again|flaky|cached) / {
      cur=$2; sub(/:$/, "", cur)
      if (!(cur in final)) order[++n]=cur
      final[cur]=$1; buf[cur]=""; next }
    /^        FAIL / && cur!="" { buf[cur]=buf[cur] $0 "\n" }
    END { red=0
          for (i=1; i<=n; i++) { c=order[i]; if (final[c]=="FAIL") { red++; printf "FAIL %s\n%s", c, buf[c] } }
          printf "== %d red check(s)\n", red; exit (red>0) }
  ' "$1"
}

self_test() {
  local out rc want
  dir=$(mktemp -d /tmp/svolder/gate-fails.XXXXXX 2>/dev/null) || { mkdir -p /tmp/svolder && dir=$(mktemp -d /tmp/svolder/gate-fails.XXXXXX); } || exit 1
  trap 'rm -rf "$dir"' EXIT
  cat > "$dir/red.log" <<'LOG'
ok a-chromium 12s
FAIL b-chromium 30s
        FAIL one went wrong
        FAIL two went wrong
retry c-firefox 9s
        FAIL a load failure of c
again c-firefox (alone)
ok c-firefox 8s
flaky c-firefox
cached d-chromium
retry e-chromium 20s
        FAIL first run of e
again e-chromium (alone)
FAIL  e-chromium 21s
        FAIL second run of e
FAIL  f-chromium: lost, no verdict (killed)
LOG
  out=$(fails "$dir/red.log"); rc=$?
  want=$'FAIL b-chromium\n        FAIL one went wrong\n        FAIL two went wrong\nFAIL e-chromium\n        FAIL second run of e\nFAIL f-chromium\n== 3 red check(s)'
  [ "$rc" = 1 ] && [ "$out" = "$want" ] || { echo "gate-fails self-test: red log: exit $rc, got: $out" >&2; exit 1; }
  cat > "$dir/green.log" <<'LOG'
ok a-chromium 12s
retry c-firefox 9s
        FAIL a load failure of c
again c-firefox (alone)
ok c-firefox 8s
flaky c-firefox
cached d-chromium
LOG
  out=$(fails "$dir/green.log"); rc=$?
  [ "$rc" = 0 ] && [ "$out" = "== 0 red check(s)" ] || { echo "gate-fails self-test: green log: exit $rc, got: $out" >&2; exit 1; }
  echo "gate-fails self-test: ok"
}

case ${1:-} in
  --self-test) self_test ;;
  '') echo "usage: gate-fails.sh <gate.log> | --self-test" >&2; exit 2 ;;
  *) fails "$1" ;;
esac
