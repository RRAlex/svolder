#!/bin/bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Every mutation runner of the web checks, one after the other, each with its counts and wall time, then a
# summary; exit 1 when a runner reports a mutant that survived (or errored), or does not run at all.
#   mutants.sh          the node runners (node/mutate_*.sh), MUTANTS_JOBS (default 4) at a time
#   mutants.sh --full   and the browser runners (mutate_*.py), each with MUT_WORKERS workers (mutpool.py's
#                       default from the cores and the CPU pressure): a long run
# A runner's own lines (KILLED / SURVIVED / ERROR, the results of its run) go to the log
# MUTANTS_LOG (default: a temp file named at the end); the lines of a runner that failed are shown.
set -u
here=$(cd "$(dirname "$0")" && pwd)
full=0
[ "${1:-}" = --full ] && full=1
. "$(dirname "$(readlink -f "$0")")/../tmp-base.sh"
log=${MUTANTS_LOG:-$(svtmp_file mutlog)} || exit 1
: >"$log"

node_runners="mutate_ember_parts mutate_computer mutate_director mutate_model mutate_nightdrive mutate_notes mutate_paper mutate_signal mutate_sources"
# a runner that times something runs its mutants alone, so it is last
browser_runners="mutate_registry mutate_webui mutate_ember mutate_paper mutate_signal mutate_nightdrive mutate_haze mutate_scroll mutate_effects mutate_smoke mutate_notes mutate_sources"

tmp=$(svtmp_dir mut) || exit 1
trap 'rm -rf "$tmp"' EXIT
jobs=${MUTANTS_JOBS:-4}
# a Ctrl-C or TERM ends the runners (the browser runners reap their own children on TERM)
trap 'trap - INT TERM; kill $(jobs -p) 2>/dev/null; pkill -TERM -P $$ 2>/dev/null; wait; exit 130' INT TERM

bad=0 total=0 killed=0 started=$(date +%s)

# go KIND LABEL COMMAND...: run it, its output, exit code and wall time kept in $tmp
go() {
  local kind=$1 label=$2 t0
  shift 2
  t0=$(date +%s)
  "$@" >"$tmp/$kind-$label.out" 2>&1
  echo $? >"$tmp/$kind-$label.rc"
  echo $(($(date +%s) - t0)) >"$tmp/$kind-$label.secs"
}

# report KIND LABEL: the runner's line, its failures, and the totals
report() {
  local kind=$1 label=$2 out line k=0 n=0 rc secs mark=''
  out=$tmp/$kind-$label.out rc=$(<"$tmp/$kind-$label.rc") secs=$(<"$tmp/$kind-$label.secs")
  line=$(grep -E '[0-9]+ of [0-9]+' "$out" | tail -n 1)
  if [[ $line =~ ([0-9]+)\ of\ ([0-9]+) ]]; then
    k=${BASH_REMATCH[1]} n=${BASH_REMATCH[2]}
  fi
  total=$((total + n)) killed=$((killed + k))
  if [ "$rc" -ne 0 ] || [ "$n" -eq 0 ] || [ "$k" -ne "$n" ]; then
    bad=$((bad + 1)) mark=' FAILED'
  fi
  printf '%-24s %-8s %-12s %ss%s\n' "$label" "$kind" "$k of $n" "$secs" "$mark"
  { echo "=== $kind $label (exit $rc)"; cat "$out"; } >>"$log"
  if [ -n "$mark" ]; then
    grep -E '^(SURVIVED|ERROR|survived)|not a kill' "$out" | head -n 20 | sed 's/^/    /'
    if [ "$n" -eq 0 ]; then tail -n 5 "$out" | sed 's/^/    /'; fi
  fi
}

printf '%-24s %-8s %-12s %s\n' runner kind killed wall
# the node runners are independent (each has its own scratch copy): MUTANTS_JOBS at a time
for r in $node_runners; do
  while [ "$(jobs -rp | wc -l)" -ge "$jobs" ]; do wait -n; done
  go node "${r#mutate_}" bash "$here/node/$r.sh" &
done
wait
for r in $node_runners; do report node "${r#mutate_}"; done
# the browser runners one after the other: each has its own pool of workers
if [ "$full" -eq 1 ]; then
  if python3 "$here/mutpool_test.py" >"$tmp/pool-test.out" 2>&1; then
    printf '%-24s %-8s %s\n' mutpool_test.py self-test ok
  else
    bad=$((bad + 1))
    printf '%-24s %-8s %s\n' mutpool_test.py self-test FAILED
    grep '^FAIL' "$tmp/pool-test.out" | sed 's/^/    /'
  fi
  for r in $browser_runners; do
    go browser "${r#mutate_}" python3 "$here/$r.py"
    report browser "${r#mutate_}"
  done
fi
echo "---"
printf 'mutants: %s of %s killed, %s runner(s) failed, %ss; log %s\n' "$killed" "$total" "$bad" "$(($(date +%s) - started))" "$log"
[ "$bad" -eq 0 ]
