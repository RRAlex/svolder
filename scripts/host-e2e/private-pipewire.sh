#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# A private PipeWire server with its own WirePlumber, for checks that must never
# touch the user's real audio session.
#
#   private-pipewire.sh start   start both, print `export` lines (use with eval)
#   private-pipewire.sh env     print the same lines for a running instance
#   private-pipewire.sh proof   exit 0 only when the server lists nothing but private objects
#   private-pipewire.sh stop    end every process started, remove the run directory
#
# The run directory is /tmp/svpw.XXXXXX exactly: SVPW_RUN, or the XDG_RUNTIME_DIR of the caller
# when that has the shape. `start` makes a new one unless SVPW_RUN names an empty one; `start`
# and `stop` accept the same values. <run>/pids holds "pid start-time" per daemon; `stop`
# signals a pid only while it is still that process (start time and SVPW_RUN in its
# environment), and fails loudly, keeping the directory, if one outlives its deadline.
set -euo pipefail
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
CORE=svolder-e2e-0
SINK=svolder_e2e_sink
DEADLINE=${SVPW_DEADLINE:-15}

die() { echo "private-pipewire: $*" >&2; exit "${2:-1}"; }

# A run directory is exactly /tmp/svpw.XXXXXX, as `realpath -m` resolves it (no `..`, no links).
valid_run() {
  local r=$1
  [[ "$r" =~ ^/tmp/svpw\.[A-Za-z0-9]{6}$ ]] && [ "$(realpath -m -- "$r")" = "$r" ]
}

run_dir() {
  local r
  if [ -n "${SVPW_RUN:-}" ]; then r=$SVPW_RUN
  elif [[ "${XDG_RUNTIME_DIR:-}" == /tmp/svpw.* ]]; then r=$XDG_RUNTIME_DIR
  else return 1; fi
  valid_run "$r" || die "$r is not a /tmp/svpw.XXXXXX run directory" 2
  printf '%s' "$r"
}

# The environment every private command runs in.
exports() {
  local r=$1
  cat <<EOT
export SVPW_RUN=$r
export XDG_RUNTIME_DIR=$r
export PIPEWIRE_RUNTIME_DIR=$r
export PIPEWIRE_REMOTE=$CORE
export XDG_CONFIG_HOME=$r/config
export XDG_STATE_HOME=$r/state
export XDG_DATA_HOME=$r/data
export XDG_CACHE_HOME=$r/cache
export DBUS_SESSION_BUS_ADDRESS=unix:path=$r/no-bus
export DBUS_SYSTEM_BUS_ADDRESS=unix:path=$r/no-system-bus
EOT
}

# private_env <run> -> the `env` arguments of the private environment
private_env() {
  local r=$1
  PENV=(SVPW_RUN="$r" XDG_RUNTIME_DIR="$r" PIPEWIRE_RUNTIME_DIR="$r" PIPEWIRE_REMOTE=$CORE
        XDG_CONFIG_HOME="$r/config" XDG_STATE_HOME="$r/state" XDG_DATA_HOME="$r/data"
        XDG_CACHE_HOME="$r/cache" DBUS_SESSION_BUS_ADDRESS="unix:path=$r/no-bus"
        DBUS_SYSTEM_BUS_ADDRESS="unix:path=$r/no-system-bus")
}

in_env() { # in_env <run> cmd...
  local r=$1; shift
  private_env "$r"
  env "${PENV[@]}" "$@"
}

# Starts a daemon and records "pid start-time" in <run>/pids. The subshell execs `env`, which
# execs `setsid`, which (not being a group leader) execs the daemon: $! is the daemon's own pid.
# The pid is then checked against /proc before it is trusted.
launch() { # launch <run> <log> cmd...
  local r=$1 log=$2; shift 2
  private_env "$r"
  ( exec env "${PENV[@]}" setsid "$@" >"$log" 2>&1 </dev/null ) &
  local pid=$! t=0
  LAUNCHED+=("$pid")   # our own child until it is recorded: an interrupted start can still end it
  # wait until the subshell has exec'd the daemon
  while [ "$(basename "$(tr '\0' '\n' 2>/dev/null <"/proc/$pid/cmdline" | head -n1)")" != "$(basename "$1")" ]; do
    t=$((t+1)); [ $t -lt 100 ] || die "pid $pid is not $1 (the launch did not exec in place)"
    sleep 0.05
  done
  proc_owned "$r" "$pid" "$(proc_start "$pid")" || die "pid $pid does not carry the private environment"
  echo "$pid $(proc_start "$pid")" >> "$r/pids"
}

proc_start() { awk '{ sub(/^.*\) /, ""); print $20 }' "/proc/$1/stat" 2>/dev/null; }

# proc_owned <run> <pid> <start>: the pid is still the process recorded, and ours.
proc_owned() {
  local r=$1 pid=$2 start=$3
  [ -r "/proc/$pid/environ" ] || return 1
  [ "$(proc_start "$pid")" = "$start" ] || return 1
  tr '\0' '\n' <"/proc/$pid/environ" | grep -qxF "SVPW_RUN=$r"
}

STARTING=
LAUNCHED=()
cleanup_start() {
  if [ -n "$STARTING" ]; then
    echo "private-pipewire: start did not finish; stopping what was started" >&2
    local p
    for p in ${LAUNCHED[@]+"${LAUNCHED[@]}"}; do kill -TERM "$p" 2>/dev/null || true; done
    local t
    for t in $(seq 1 $((DEADLINE*20))); do
      local alive=0
      for p in ${LAUNCHED[@]+"${LAUNCHED[@]}"}; do kill -0 "$p" 2>/dev/null && alive=1; done
      [ $alive = 0 ] && break
      sleep 0.05
    done
    for p in ${LAUNCHED[@]+"${LAUNCHED[@]}"}; do kill -KILL "$p" 2>/dev/null || true; done
    for p in ${LAUNCHED[@]+"${LAUNCHED[@]}"}; do wait "$p" 2>/dev/null || true; done
    stop_dir "$STARTING" || true
  fi
}

cmd_start() {
  local r made=
  # a refusal after the directory was made removes it (only when it has the exact shape)
  refuse() { if [ -n "$made" ] && valid_run "$made"; then rmdir "$made" 2>/dev/null || true; fi; die "$1" 2; }
  if [ -n "${SVPW_RUN:-}" ]; then
    r=$SVPW_RUN
    valid_run "$r" || die "SVPW_RUN must be a /tmp/svpw.XXXXXX directory" 2
    [ "$r" != "${XDG_RUNTIME_DIR:-}" ] || die "SVPW_RUN is the caller's own XDG_RUNTIME_DIR" 2
    [ -d "$r" ] || die "SVPW_RUN is not a directory" 2
    [ ! -s "$r/pids" ] || die "an instance is already recorded in $r (stop it first)" 2
  else
    r=$(mktemp -d /tmp/svpw.XXXXXX) || die "mktemp failed"
    made=$r
    valid_run "$r" || refuse "unexpected run directory shape $r"
  fi
  [ "$r" != "${XDG_RUNTIME_DIR:-}" ] || refuse "the run directory is the caller's own XDG_RUNTIME_DIR"
  [ "$r" != "${PIPEWIRE_RUNTIME_DIR:-}" ] || refuse "the run directory is the caller's own PIPEWIRE_RUNTIME_DIR"
  local sock="$r/$CORE"
  [ "${#sock}" -lt 108 ] || refuse "socket path ${#sock} bytes, must stay under 108"
  STARTING=$r
  trap cleanup_start EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  chmod 700 "$r"
  mkdir -p "$r/config/wireplumber" "$r/state" "$r/data" "$r/cache"
  : > "$r/pids"
  # WirePlumber reads the fragment from the temp XDG_CONFIG_HOME (the default config is the system one).
  cp -r "$HERE/wireplumber.conf.d" "$r/config/wireplumber/"
  rm -f "$sock" "$sock.lock"

  launch "$r" "$r/pipewire.log" pipewire -c "$HERE/pipewire.conf"
  local pw; pw=$(head -n1 "$r/pids" | cut -d' ' -f1)
  local t=0
  while [ ! -S "$sock" ]; do
    kill -0 "$pw" 2>/dev/null || die "pipewire exited: $(tail -n 3 "$r/pipewire.log" | tr '\n' ' ')"
    t=$((t+1)); [ $t -lt $((DEADLINE*20)) ] || die "pipewire did not open its socket"
    sleep 0.05
  done

  launch "$r" "$r/wireplumber.log" wireplumber
  local wp; wp=$(tail -n1 "$r/pids" | cut -d' ' -f1)
  # Ready when WirePlumber's own client shows in the dump.
  t=0
  while ! in_env "$r" timeout 5 pw-dump 2>/dev/null | python3 -I -c '
import json, sys
try: d = json.load(sys.stdin)
except Exception: sys.exit(1)
sys.exit(0 if any(o.get("type", "").endswith("Client") and (o.get("info") or {}).get("props", {}).get("wireplumber.daemon") for o in d) else 1)'; do
    kill -0 "$wp" 2>/dev/null || die "wireplumber exited: $(tail -n 3 "$r/wireplumber.log" | tr '\n' ' ')"
    t=$((t+1)); [ $t -lt $((DEADLINE*5)) ] || die "wireplumber did not connect"
    sleep 0.2
  done
  STARTING=
  exports "$r"
}

cmd_proof() {
  local r; r=$(run_dir) || die "no private run directory (set SVPW_RUN or source the env lines)" 2
  [ "${PIPEWIRE_REMOTE:-}" = "$CORE" ] || die "PIPEWIRE_REMOTE is not $CORE" 2
  [ "${XDG_RUNTIME_DIR:-}" = "$r" ] && [ "${PIPEWIRE_RUNTIME_DIR:-}" = "$r" ] || die "XDG_RUNTIME_DIR/PIPEWIRE_RUNTIME_DIR are not the private run directory" 2
  [ -S "$r/$CORE" ] || die "no socket $r/$CORE" 2
  in_env "$r" timeout 10 pw-dump > "$r/proof-dump.json" 2>"$r/proof-dump.err" || die "pw-dump failed" 2
  python3 -I - "$r/proof-dump.json" "$CORE" "$SINK" <<'PY'
import json, sys
dump = json.load(open(sys.argv[1])); core = sys.argv[2]; sink = sys.argv[3]
bad = []
cores = [o for o in dump if o["type"] == "PipeWire:Interface:Core"]
if not cores or cores[0].get("info", {}).get("props", {}).get("core.name") != core:
    bad.append("core.name is not " + core)
clients = {o["id"] for o in dump if o["type"] == "PipeWire:Interface:Client"}
for o in dump:
    t = o["type"].split(":")[-1]
    props = (o.get("info") or {}).get("props") or {}
    text = json.dumps(props).lower()
    for word in ("alsa", "bluez", "v4l2", "libcamera"):
        if word in text:
            bad.append("%s %s mentions %s" % (t, o["id"], word))
    if t == "Device":
        bad.append("Device %s %s" % (o["id"], props.get("device.name")))
    if t == "Node":
        name = props.get("node.name")
        cid = props.get("client.id")
        if name in ("Dummy-Driver", sink): continue
        if "device.id" in props or "api.alsa.path" in props: bad.append("device node %s" % name)
        elif cid is None: bad.append("node %s %s without a client" % (o["id"], name))
        elif cid not in clients: bad.append("node %s of unknown client" % name)
if not any(o["type"].endswith("Node") and ((o.get("info") or {}).get("props") or {}).get("node.name") == sink for o in dump):
    bad.append("the null sink " + sink + " is missing")
if bad:
    print("proof FAILED:\n  " + "\n  ".join(bad), file=sys.stderr); sys.exit(1)
nodes = [((o.get("info") or {}).get("props") or {}).get("node.name") for o in dump if o["type"].endswith("Node")]
print("proof ok: nodes %s, %d clients, no device, no alsa/bluez/v4l2 object" % (nodes, len(clients)))
PY
}

# stop_dir <run>: ends every recorded process that is still the one started, fails loudly if
# one outlives its deadline (the run directory is then kept), else removes the run directory.
stop_dir() {
  local r=$1 pid start line alive=0 t
  valid_run "$r" || die "refusing to remove $r" 2
  local -a pids=() starts=()
  if [ -f "$r/pids" ]; then
    while read -r pid start; do
      [ -n "${pid:-}" ] || continue
      if proc_owned "$r" "$pid" "$start"; then pids+=("$pid"); starts+=("$start")
      elif [ -e "/proc/$pid" ]; then echo "private-pipewire: pid $pid is no longer the recorded process; not signalled" >&2; fi
    done < <(tac "$r/pids")
  fi
  for pid in ${pids[@]+"${pids[@]}"}; do kill -TERM "$pid" 2>/dev/null || true; done
  for t in $(seq 1 $((DEADLINE*20))); do
    alive=0
    for i in ${!pids[@]}; do proc_owned "$r" "${pids[$i]}" "${starts[$i]}" && alive=1; done
    [ $alive = 0 ] && break
    sleep 0.05
  done
  if [ $alive = 1 ]; then
    for i in ${!pids[@]}; do proc_owned "$r" "${pids[$i]}" "${starts[$i]}" && kill -KILL "${pids[$i]}" 2>/dev/null || true; done
    sleep 0.3
    for i in ${!pids[@]}; do
      if proc_owned "$r" "${pids[$i]}" "${starts[$i]}"; then
        die "pid ${pids[$i]} is still alive after SIGKILL; run directory $r kept"
      fi
    done
  fi
  for i in ${!pids[@]}; do
    if proc_owned "$r" "${pids[$i]}" "${starts[$i]}"; then die "pid ${pids[$i]} is still alive; run directory $r kept"; fi
  done
  rm -rf -- "$r"
  echo "stopped ${#pids[@]} process(es), removed $r"
}

cmd_stop() {
  local r; r=$(run_dir) || die "no private run directory (set SVPW_RUN)" 2
  stop_dir "$r"
}

cmd_env() {
  local r; r=$(run_dir) || die "no private run directory" 2
  exports "$r"
}

case "${1:-}" in
  start) cmd_start ;;
  stop) cmd_stop ;;
  env) cmd_env ;;
  proof) cmd_proof ;;
  *) die "usage: $0 start|stop|env|proof" 2 ;;
esac
