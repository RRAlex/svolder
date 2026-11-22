#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# `make host-e2e`: the computer EQ end to end, on a PRIVATE PipeWire with its own WirePlumber and a
# null sink (no sound card). Opt-in: never part of `make check`, the web checks or CI.
#
# SOUND SAFETY. Your own PipeWire session is never touched:
#   - every PipeWire tool and the daemon run in the environment private-pipewire.sh prints (a
#     run directory of its own as XDG_RUNTIME_DIR, PIPEWIRE_REMOTE, no session bus);
#   - `private-pipewire.sh proof` must pass before anything plays;
#   - this script exits 2 when that environment is missing;
#   - your own PipeWire and WirePlumber processes are listed before and after and must be the same;
#   - a trap ends everything this run started, on any exit.
# It never runs `systemctl --user` (the daemon gets SVOLDER_SYSTEMCTL=/bin/false) and never the
# installed svolder binaries: only target/debug/svolderd.
set -uo pipefail

HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
LAUNCHER="$HERE/private-pipewire.sh"
ANALYSE="$HERE/analyse.py"
BITS="$HERE/bits.py"
REPO=$(cd "$HERE/../.." && pwd)

fail() { echo "host-e2e: FAIL: $*" >&2; exit "${2:-1}"; }

# `run.sh bits` (make host-e2e-bits) runs phase B alone. No environment variable can make a run of
# only some phases pass for the whole check, so HOST_E2E_ONLY=bits is refused.
BITS_ONLY=0
case "${1:-}" in
  bits) BITS_ONLY=1 ;;
  "") ;;
  *) fail "unknown argument '$1' (only 'bits')" 2 ;;
esac
[ "${HOST_E2E_ONLY:-}" != bits ] || fail "HOST_E2E_ONLY=bits is not a mode: phase B alone is make host-e2e-bits" 2

# The listing of the user's audio processes the before/after comparison reads.
ME=$(id -un) || fail "cannot tell who runs this" 2
[ -n "$ME" ] || fail "cannot tell who runs this" 2
# By the program's own name, not its command line: another job's shell can carry a word like
# SVOLDER_WIREPLUMBER= in its arguments.
# Only the user's session services count: their cgroup is under user@UID.service/session.slice/. A fake
# `pipewire` script some other job's test starts (comm `pipewire`) lives in that job's own cgroup.
UID_NUM=$(id -u) || fail "cannot tell who runs this" 2
audio_procs() {
  local pid rest
  ps -o pid=,comm=,args= -u "$ME" | awk '$2 ~ /^(pipewire|pipewire-pulse|wireplumber)$/' | while read -r pid rest; do
    if grep -q "user@${UID_NUM}\.service/session\.slice/" "/proc/$pid/cgroup" 2>/dev/null; then echo "$pid $rest"; fi
  done || true
}

command -v pipewire >/dev/null && command -v pw-dump >/dev/null && command -v pw-cat >/dev/null \
  && command -v pw-record >/dev/null && command -v pw-link >/dev/null && command -v pw-metadata >/dev/null \
  && command -v wpctl >/dev/null && command -v wireplumber >/dev/null && command -v python3 >/dev/null \
  && command -v paplay >/dev/null && command -v pactl >/dev/null \
  || fail "needs pipewire, wireplumber and their tools (pw-dump, pw-cat, pw-record, pw-link, pw-metadata, wpctl), paplay and pactl (pulseaudio-utils) and python3 with numpy" 2
python3 -I -c 'import numpy' 2>/dev/null || fail "python3 needs numpy" 2
python3 -I "$HERE/vu_selftest.py" || fail "the VU phase's own check (vu_selftest.py) failed" 2
python3 -I "$BITS" --selftest || fail "phase B's own check (bits.py --selftest) failed" 2
# Phase B alone (make host-e2e-bits) needs no daemon (phase C runs target/debug/svolder inside
# bits.py, which the make target builds).
if [ "$BITS_ONLY" = 0 ]; then
  [ -x "$REPO/target/debug/svolderd" ] || fail "no target/debug/svolderd (make host-e2e builds it)" 2
  [ -x "$REPO/target/debug/examples/xf_expect" ] || fail "no target/debug/examples/xf_expect (make host-e2e builds it)" 2
  [ -x "$REPO/target/debug/examples/vs_expect" ] || fail "no target/debug/examples/vs_expect (make host-e2e builds it)" 2
  echo "== daemon binary: $REPO/target/debug/svolderd, built $(date -r "$REPO/target/debug/svolderd" '+%F %T')"
fi

BEFORE=$(audio_procs)
[ -n "$BEFORE" ] || fail "cannot see your own PipeWire processes (is your session's PipeWire running?): the before and after comparison would prove nothing" 2
echo "== your PipeWire and WirePlumber processes before:"
echo "$BEFORE"

STARTED=
PY=
cleanup() {
  local rc=$?
  trap - EXIT INT TERM
  if [ -n "$PY" ] && kill -0 "$PY" 2>/dev/null; then
    kill -TERM "$PY" 2>/dev/null
    wait "$PY" 2>/dev/null
  fi
  if [ "$rc" != 0 ] && [ -n "${SVPW_RUN:-}" ] && [ -d "$SVPW_RUN" ]; then
    # A failed run keeps its evidence: the run directory is removed below.
    local kept="$REPO/target/host-e2e-failures/$(date +%Y%m%d-%H%M%S)"
    mkdir -p "$kept" && cp -- "$SVPW_RUN"/*.log "$SVPW_RUN"/rec*.wav "$kept"/ 2>/dev/null
    echo "== the private PipeWire's log lines about xruns and late clients:"
    grep -iE 'xrun|too slow|timeout' "$SVPW_RUN/pipewire.log" 2>/dev/null || echo "(none)"
    echo "host-e2e: the logs and recordings of this failed run are in $kept"
  fi
  if [ -n "$STARTED" ]; then
    # Anything still carrying this run's marker (a tone, a recorder, the daemon) is ours: end it
    # before the launcher ends the server.
    local d pid
    for d in /proc/[0-9]*; do
      pid=${d#/proc/}
      [ "$pid" != "$$" ] || continue
      if { tr '\0' '\n' <"$d/environ" | grep -qxF "SVPW_RUN=$SVPW_RUN"; } 2>/dev/null; then
        case "$(tr '\0' ' ' <"$d/cmdline" 2>/dev/null)" in
          *pw-cat*|*pw-record*|*svolderd*|*host-eq.conf*|*bits.conf*|*dbus-daemon*|*pulse.conf*|*paplay*|*pactl*) kill -TERM "$pid" 2>/dev/null ;;
        esac
      fi
    done
    bash "$LAUNCHER" stop || rc=1
  fi
  local after; after=$(audio_procs)
  echo "== your PipeWire and WirePlumber processes after:"
  echo "$after"
  if [ "$BEFORE" = "$after" ]; then
    echo "host-e2e: your session's processes are unchanged"
  else
    echo "host-e2e: FAIL: the process lists differ" >&2
    diff <(echo "$BEFORE") <(echo "$after") >&2
    rc=1
  fi
  [ "$rc" = 0 ] && echo "host-e2e: every process it started is gone"
  exit "$rc"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# The run directory is made here and the trap armed before the launcher starts, so an interrupt at
# any moment ends whatever was started (`launcher stop` on a directory with nothing in it just
# removes it).
. "$HERE/../tmp-base.sh"
SVPW_RUN=$(SVOLDER_TMP_BASE=/tmp/svolder svtmp_dir pw) || fail "mktemp failed" 2
export SVPW_RUN
STARTED=1
env_out=$(bash "$LAUNCHER" start) || fail "the private PipeWire did not start" 2
eval "$env_out"
# Refuse unless this is the launcher's environment, whatever else is set.
case "${SVPW_RUN:-}" in /tmp/svolder/pw.??????) ;; *) fail "the launcher's environment is missing" 2 ;; esac
[ "${XDG_RUNTIME_DIR:-}" = "$SVPW_RUN" ] && [ "${PIPEWIRE_RUNTIME_DIR:-}" = "$SVPW_RUN" ] \
  && [ "${PIPEWIRE_REMOTE:-}" = svolder-e2e-0 ] || fail "the launcher's environment is missing" 2
echo "== private instance: $SVPW_RUN"
bash "$LAUNCHER" proof || fail "proof failed: nothing played" 2
echo "== while it runs (the private pair is added):"
ps -o pid,args -u "$ME" | grep -E 'pipewire|wireplumber' | grep -v -E 'grep|host-e2e/run.sh'

# The proof that the level checks fail on a chain that does nothing: HOST_E2E_BYPASS=1 make host-e2e
# (never set otherwise). The variable goes in only when it is exactly 1.
BYPASS=0; [ "${HOST_E2E_BYPASS:-}" = 1 ] && BYPASS=1
# HOST_E2E_ONLY=L runs the loudness phases alone, HOST_E2E_ONLY=S the scenes phase alone, while they are being
# worked on: it exits 3 and never prints a PASS line. Said here so a variable left in the environment cannot pass for the whole check unnoticed.
[ -z "${HOST_E2E_ONLY:-}" ] || echo "host-e2e: HOST_E2E_ONLY=${HOST_E2E_ONLY} is set: a dev run of some phases, it will not report a pass" >&2
# Phases B and C (bits.py) run first: its own PASS is this script's, and analyse.py's `host-e2e: PASS`
# stays the last word of a full run. HOST_E2E_ONLY=L or S (a phase alone) skips it.
if [ "${HOST_E2E_ONLY:-}" != L ] && [ "${HOST_E2E_ONLY:-}" != S ]; then
  timeout -k 20 "${HOST_E2E_BITS_DEADLINE:-300}" python3 -I "$BITS" &
  PY=$!
  wait "$PY"
  rc=$?
  PY=
  [ "$rc" = 0 ] || exit "$rc"
  if [ "$BITS_ONLY" = 1 ]; then echo "host-e2e-bits: PASS"; exit 0; fi
fi
HOST_E2E_BYPASS=$BYPASS timeout -k 20 "${HOST_E2E_DEADLINE:-600}" python3 -I "$ANALYSE" &
PY=$!
wait "$PY"
rc=$?
PY=
[ "$rc" = 0 ] || exit "$rc"
exit 0
