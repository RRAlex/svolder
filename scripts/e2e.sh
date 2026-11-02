#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# End-to-end test: run svolder-sim as a BLE peripheral on one controller and drive the
# svolder CLI — and then svolderd — against it over BlueZ on another. Requires two Bluetooth
# controllers that support LE extended advertising (MaxAdvLen > 31): set both
# SVOLDER_E2E_ADV_ADAPTER and SVOLDER_E2E_SCAN_ADAPTER to two different controllers to pick
# them, or neither to auto-detect two qualifying controllers from what BlueZ exposes:
# two of the same USB model when the host has a pair, else USB dongles before any other
# controller. Never requires root.
set -euo pipefail

UNIT="${SVOLDER_E2E_UNIT:-0102030405060708}"
# The unit id the signal section's own simulator advertises, distinct from $UNIT: BlueZ can keep
# a stale device object, and its ManufacturerData, from the Gungnir 2 simulator that section
# stops, so a fresh unit id rather than a reused one is what makes the watch that follows see the
# new advertisement rather than the cached properties of the old one.
SIGNAL_UNIT="${SVOLDER_E2E_SIGNAL_UNIT:-0102030405060709}"
# The unit id the Loki Max section's simulator advertises: a fresh id for the reason SIGNAL_UNIT
# gives, since BlueZ keeps the ManufacturerData of a simulator it has heard before.
LOKI_UNIT=010203040506070a
# The passphrase the peripheral is started with, which the live tests encode frames for.
PASSPHRASE=1234
# A directory holding prebuilt `svolder` and `svolderd` (target/dist/aarch64, say) to drive in place
# of the cargo build's own. Unset or empty: the run builds and uses target/debug as always. Checked
# here, before anything touches a controller, so a wrong directory fails at once.
BIN_DIR="${SVOLDER_E2E_BIN_DIR:-}"
if [ -n "$BIN_DIR" ]; then
  for bin in svolder svolderd; do
    [ -f "$BIN_DIR/$bin" ] && [ -x "$BIN_DIR/$bin" ] \
      || { echo "e2e: SVOLDER_E2E_BIN_DIR=$BIN_DIR has no executable $BIN_DIR/$bin" >&2; exit 1; }
  done
fi
# This run's own directory: the logs and the state below live here, so two runs never share a
# file. Removed when the run ends, unless it fails after writing to a log: then it is kept, for
# the logs.
RUN_DIR=$(mktemp -d -t svolder-e2e.XXXXXX)
SIM_LOG="$RUN_DIR/sim.log"
DAEMON_LOG="$RUN_DIR/daemon.log"
# Set once the last step has passed, so the EXIT trap knows whether the run failed.
PASSED=
# The BLE MIDI GATT service. A peer adapter on this host may expose it; see fail().
BLE_MIDI_UUID=03b80e5a-ede8-4b33-a751-6ce34ec4c700
# Set once the central's view of the peer has listed that service.
PEER_EXPOSES_MIDI=
# The daemon the last section drives: its pid, the directory holding its configuration and
# its socket, and the socket itself. All empty until that section starts it.
DAEMON_PID=
DAEMON_DIR=
DAEMON_SOCKET=
# The file the signal section's `watch` writes its readings to, removed however the run ends.
# Empty until that section starts.
SIGNAL_OUT=

# Every svolder/svolderd this script runs keeps its state (last_adapter and the rest of
# DeviceState) here rather than the host's own ~/.local/state/svolder, the same way DAEMON_DIR
# keeps the daemon section's configuration off the host's own file.
STATE_DIR="$RUN_DIR/state"
export XDG_STATE_HOME="$STATE_DIR"

# The capture steps look at systemd's units and at the daemon's socket, and stop what they find.
# This host may have the real svolderd installed, so each capture step runs through `isolated`:
# a runtime directory of its own (the default socket lives there) and a `systemctl` that is not
# there, so a capture can neither see, stop nor restart the host's own daemon or its units.
ISOLATED_RUNTIME="$RUN_DIR/runtime"
ISOLATED_SYSTEMCTL="$RUN_DIR/no-systemctl"
# An empty configuration directory for the direct capture steps, so the developer's own
# config.toml (a `socket`, configured devices) cannot steer them. The daemon step keeps the
# configuration daemon_cli gives it.
ISOLATED_CONFIG="$RUN_DIR/config"
isolated() {
  XDG_RUNTIME_DIR="$ISOLATED_RUNTIME" SVOLDER_SYSTEMCTL="$ISOLATED_SYSTEMCTL" "$@"
}

# Every sim this run starts appends to the same file, so a late failure still shows the first
# sim's lines — the kill test's disconnects among them.
: >"$DAEMON_LOG"
: >"$SIM_LOG"
# How many sims this run has started, which is how many "ready" lines the log holds once the
# newest one is up.
SIM_STARTS=0

fail() {
  echo "e2e: FAIL — $1" >&2
  echo '--- sim log'
  cat "$SIM_LOG" 2>/dev/null || true
  echo '--- daemon log'
  cat "$DAEMON_LOG" 2>/dev/null || true
  if [ -n "$PEER_EXPOSES_MIDI" ]; then
    echo "e2e: the peer adapter exposes a BLE MIDI service ($BLE_MIDI_UUID). Unless bluetoothd" \
      "runs with --noplugin=midi (a systemd drop-in) and no WirePlumber BLE MIDI monitor is" \
      "registered, something on this host answers that service with a pairing attempt and the" \
      "link drops."
  fi
  if grep -q 'ATT error: 0x01' "$DAEMON_LOG" 2>/dev/null; then
    echo "e2e: the daemon log holds an ATT error 0x01: the peripheral adapter's GATT cache is" \
      "stale. Run: sudo systemctl restart bluetooth"
  fi
  exit 1
}

# Start svolder-sim on the advertising controller and wait until it says it is ready.
# The pid is exported so a live test can tell the peripheral apart from other devices. Unit id
# is $UNIT unless the caller sets $SIM_UNIT first, the way it sets $SIM_EXTRA.
#
# The log is appended to, one section per sim, so readiness is counted rather than matched:
# this sim is up once the log holds as many "ready" lines as this run has started sims.
start_sim() {
  SIM_STARTS=$((SIM_STARTS + 1))
  echo "--- svolder-sim start $SIM_STARTS" >>"$SIM_LOG"
  "$SIM_BIN" --adapter "$ADV" --unit "${SIM_UNIT:-$UNIT}" --passphrase "$PASSPHRASE" ${SIM_EXTRA:-} >>"$SIM_LOG" 2>&1 &
  SIM_PID=$!
  export SVOLDER_E2E_SIM_PID="$SIM_PID"
  local ready
  for _ in $(seq 100); do
    ready=$(grep -c 'svolder-sim: ready' "$SIM_LOG" 2>/dev/null || true)
    [ "${ready:-0}" -ge "$SIM_STARTS" ] && return 0
    kill -0 "$SIM_PID" 2>/dev/null || fail "svolder-sim exited before it was ready"
    sleep 0.1
  done
  fail "svolder-sim was not ready within 10s"
}

# Stop the sim, tolerating a pid that is already gone.
stop_sim() {
  [ -n "${SIM_PID:-}" ] || return 0
  kill "$SIM_PID" 2>/dev/null || true
  wait "$SIM_PID" 2>/dev/null || true
  SIM_PID=
  unset SVOLDER_E2E_SIM_PID
}

# Stop the daemon and wait for its socket to go with it; non-zero when the socket outlives
# it by more than 5s.
stop_daemon() {
  [ -n "$DAEMON_PID" ] || return 0
  kill "$DAEMON_PID" 2>/dev/null || true
  wait "$DAEMON_PID" 2>/dev/null || true
  DAEMON_PID=
  # A capture stops this daemon and svolder starts another on the same socket, which is no child
  # of this script: whatever still answers there is asked to stop over the socket.
  if [ -S "$DAEMON_SOCKET" ]; then
    isolated timeout 15 "$SVOLDER" --socket "$DAEMON_SOCKET" daemon stop >/dev/null 2>&1 || true
  fi
  for _ in $(seq 50); do
    [ -e "$DAEMON_SOCKET" ] || return 0
    sleep 0.1
  done
  return 1
}

cleanup() {
  # The run's exit status: zero for a pass or a skip, non-zero for a failure.
  local status=$?
  # The daemon holds the link to the peripheral while it runs, so it is stopped however the run
  # ended: the hand probe that SVOLDER_E2E_KEEP_SIM leaves the sim for could not reach a
  # peripheral that a daemon is still connected to. Neither step may fail here, or the sim
  # below it would be left on the air.
  stop_daemon || true
  if [ -n "$DAEMON_DIR" ]; then rm -rf "$DAEMON_DIR" || true; fi
  if [ "$status" != 0 ] && { [ -s "$SIM_LOG" ] || [ -s "$DAEMON_LOG" ]; }; then
    echo "e2e: this run's logs and state are kept in $RUN_DIR" >&2
  else
    rm -rf "$RUN_DIR" || true
  fi
  if [ -z "$PASSED" ] && [ -n "${SVOLDER_E2E_KEEP_SIM:-}" ] && [ -n "${SIM_PID:-}" ]; then
    echo "e2e: leaving the sim running (pid $SIM_PID); kill $SIM_PID before the next run"
    return
  fi
  if [ -n "$SIGNAL_OUT" ]; then rm -f "$SIGNAL_OUT" || true; fi
  stop_sim
}
trap cleanup EXIT
mkdir "$STATE_DIR"
mkdir -m 700 "$ISOLATED_RUNTIME"
mkdir "$ISOLATED_CONFIG"

SKIP_MSG="e2e: SKIP — needs two Bluetooth controllers with LE extended advertising (MaxAdvLen > 31); the 27-byte Forkbeard advertisement does not fit legacy 31-byte advertising, so btvirt virtual controllers cannot run this test. Add a second BLE 5.0+ USB adapter, or set SVOLDER_E2E_ADV_ADAPTER/SVOLDER_E2E_SCAN_ADAPTER."

all_controllers() {
  # Controller names that BlueZ currently exposes, in index order.
  busctl --system tree org.bluez 2>/dev/null | grep -oE 'hci[0-9]+' | sort -uV
}

max_adv_len() {
  # A controller's extended-advertising data budget in bytes, or empty if unavailable.
  busctl --system get-property org.bluez "/org/bluez/$1" org.bluez.LEAdvertisingManager1 SupportedCapabilities 2>/dev/null \
    | grep -oE 'MaxAdvLen" y [0-9]+' | grep -oE '[0-9]+$'
}

supports_ext_adv() {
  local len
  len=$(max_adv_len "$1")
  [ -n "$len" ] && [ "$len" -gt 31 ]
}

on_the_usb_bus() {
  # Whether BlueZ reports this controller's Modalias on the USB bus, which a dongle is and an
  # on-board or UART controller is not.
  local modalias
  modalias=$(busctl --system get-property org.bluez "/org/bluez/$1" org.bluez.Adapter1 Modalias 2>/dev/null || true)
  [[ "$modalias" == *'"usb:'* ]]
}

qualifying_controllers() {
  # The controllers that can carry the 27-byte advertisement, USB dongles first and every
  # other controller last, one per line: a dongle can be chosen and replaced for the job,
  # where an on-board controller is whatever the board came with.
  local h dongles='' others=''
  for h in $(all_controllers); do
    supports_ext_adv "$h" || continue
    if on_the_usb_bus "$h"; then dongles+="$h"$'\n'; else others+="$h"$'\n'; fi
  done
  printf '%s%s' "$dongles" "$others"
}

usb_id() {
  # This controller's real USB vendor:product, from the device it hangs off. BlueZ's own
  # Adapter1.Modalias reports a generic id for every controller on some hosts, so the
  # hardware's own is read instead. Empty when the controller has no USB device under it.
  local modalias
  modalias=$(cat "/sys/class/bluetooth/$1/device/modalias" 2>/dev/null || true)
  # `usb:v2357p0604d0200dcE0…` — the vendor and product fields, and nothing after them.
  [[ "$modalias" =~ ^usb:v([0-9A-Fa-f]{4})p([0-9A-Fa-f]{4}) ]] || return 0
  printf '%s:%s' "${BASH_REMATCH[1]}" "${BASH_REMATCH[2]}"
}

matched_pair() {
  # The first two of the given controllers that are the same USB model, printed as
  # "<peripheral> <central> <vendor:product>"; nothing when no two of them are. Two identical
  # radios keep chipset differences out of the connect path, which is the reason a pair of
  # dongles is here at all.
  #
  # The controllers are walked in index order, whatever order they arrive in, so the lower
  # index of a pair is always the one that advertises.
  local -A first=()
  local h id
  for h in $(printf '%s\n' "$@" | sort -V); do
    id=$(usb_id "$h")
    [ -n "$id" ] || continue
    if [ -n "${first[$id]:-}" ]; then
      printf '%s %s %s' "${first[$id]}" "$h" "$id"
      return 0
    fi
    first[$id]="$h"
  done
}

ADV="${SVOLDER_E2E_ADV_ADAPTER:-}"
SCAN="${SVOLDER_E2E_SCAN_ADAPTER:-}"
# Which rule chose the pair, printed with it, so a run says why it is on these controllers.
WHY=

# Naming only one controller is a mistake, not a request to auto-detect the other.
if { [ -n "$ADV" ] && [ -z "$SCAN" ]; } || { [ -z "$ADV" ] && [ -n "$SCAN" ]; }; then
  echo "e2e: set both SVOLDER_E2E_ADV_ADAPTER and SVOLDER_E2E_SCAN_ADAPTER, or neither" >&2
  exit 1
fi

if [ -n "$ADV" ] && [ "$ADV" = "$SCAN" ]; then
  echo "e2e: SVOLDER_E2E_ADV_ADAPTER and SVOLDER_E2E_SCAN_ADAPTER name the same controller ($ADV); peripheral and central must be different controllers" >&2
  exit 1
fi

if [ -n "$ADV" ] && [ -n "$SCAN" ]; then
  if ! supports_ext_adv "$ADV" || ! supports_ext_adv "$SCAN"; then
    echo "$SKIP_MSG"
    exit 0
  fi
  WHY=" (named)"
else
  mapfile -t QUALIFYING < <(qualifying_controllers)
  if [ "${#QUALIFYING[@]}" -lt 2 ]; then
    echo "$SKIP_MSG"
    exit 0
  fi
  # Two of a kind if there are two of a kind; otherwise the order above, dongles first.
  read -r PAIR_ADV PAIR_SCAN PAIR_ID <<<"$(matched_pair "${QUALIFYING[@]}")" || true
  if [ -n "$PAIR_ADV" ]; then
    ADV="$PAIR_ADV"; SCAN="$PAIR_SCAN"; WHY=" (matched pair $PAIR_ID)"
  else
    ADV="${QUALIFYING[0]}"; SCAN="${QUALIFYING[1]}"; WHY=" (no matched pair; USB bus first)"
  fi
fi

echo "e2e: peripheral on $ADV, central on $SCAN$WHY, unit $UNIT, signal unit $SIGNAL_UNIT"
# With SVOLDER_E2E_BIN_DIR set only the simulator is built: svolder and svolderd come from that
# directory. The ignored live transport tests below run `cargo test`, a native test binary that
# is built here whatever the prebuilt binaries are, and the simulator stays native too.
if [ -z "$BIN_DIR" ]; then
  cargo build -q -p svolder -p svolderd -p svolder-sim
else
  cargo build -q -p svolder-sim
fi

# The sim's executable as cargo names it, wherever the target directory is. The build above has
# just made it, so a build that fails here, or names no executable, is a broken run.
SIM_BUILD=$(cargo build -q -p svolder-sim --message-format=json) \
  || fail "cargo build of svolder-sim exited non-zero"
SIM_BIN=$(sed -n 's/.*"executable":"\([^"]*svolder-sim\)".*/\1/p' <<<"$SIM_BUILD" | sed -n 1p)
[ -n "$SIM_BIN" ] || fail "cargo build of svolder-sim named no svolder-sim executable"
if [ -z "$BIN_DIR" ]; then
  SVOLDER="target/debug/svolder"
  SVOLDERD="target/debug/svolderd"
else
  # svolder starts a daemon again after a capture from the svolderd beside itself, so the two
  # must be used from the one directory.
  SVOLDER="$BIN_DIR/svolder"
  SVOLDERD="$BIN_DIR/svolderd"
  echo "e2e: svolder and svolderd from $BIN_DIR"
fi

# The address of the controller the peripheral advertises from, as BlueZ reports it.
peer_address() {
  busctl --system get-property org.bluez "/org/bluez/$ADV" org.bluez.Adapter1 Address 2>/dev/null \
    | grep -oE '([0-9A-F]{2}:){5}[0-9A-F]{2}' || true
}

# Remember whether the central's view of the peer lists the BLE MIDI service, which fail()
# then explains. The device object exists only once the central has heard the peer, so no
# object is nothing to report.
note_peer_midi() {
  local addr uuids
  addr=$(peer_address)
  [ -n "$addr" ] || return 0
  uuids=$(busctl --system get-property org.bluez "/org/bluez/$SCAN/dev_${addr//:/_}" \
    org.bluez.Device1 UUIDs 2>/dev/null || true)
  case "$uuids" in *"$BLE_MIDI_UUID"*) PEER_EXPOSES_MIDI=1 ;; esac
}

start_sim

# Each verb's output is captured before it is searched: piping straight into `grep -q`
# lets grep exit on its first match, and the EPIPE that follows would fail the pipeline
# under `set -o pipefail` even though the command did its job.

# How long one scan listens: the sim advertises every 100 ms, so a window this long holds about
# fifty of its advertisements. crates/svolder-ble/tests/bluez_e2e.rs keeps a copy of this
# constant and of the two below for its own scans; a change to one copy must be made to the other.
SCAN_WINDOW=5s
# How many of those windows a step gives the radio before it calls the peripheral absent.
# SCAN_TRIES in bluez_e2e.rs must follow it.
SCAN_TRIES=3
# How long a step waits between two of those windows, in seconds. SCAN_BREATH in bluez_e2e.rs
# must follow it.
SCAN_BREATH=1

# True when a scan lists the unit, trying up to SCAN_TRIES times, leaving the last listing in
# SCAN_LISTING for a step that has to quote it; exits the run when the
# scan itself fails, naming the step $1 describes.
scan_lists_unit() {
  local what="$1" i
  for i in $(seq "$SCAN_TRIES"); do
    [ "$i" = 1 ] || sleep "$SCAN_BREATH"
    SCAN_LISTING=$("$SVOLDER" --transport ble --adapter "$SCAN" scan --timeout "$SCAN_WINDOW") \
      || fail "scan exited non-zero ($what)"
    # This scan is what gives the central a device object for the peer to read.
    note_peer_midi
    grep -qi "$UNIT" <<<"$SCAN_LISTING" && return 0
  done
  return 1
}

echo "e2e: scan"
scan_lists_unit "peripheral advertising" || fail "scan did not list the peripheral"

echo "e2e: inspect"
out=$("$SVOLDER" --transport ble --adapter "$SCAN" -d "$UNIT" inspect) || fail "inspect exited non-zero"
grep -qi "Volume" <<<"$out" || fail "inspect did not report controls"

echo "e2e: the adapter is recorded"
grep -q "last_adapter = \"$SCAN\"" "$STATE_DIR/svolder/state.toml" \
  || fail "the state file does not record $SCAN as the adapter that connected: $(cat "$STATE_DIR/svolder/state.toml" 2>&1)"

echo "e2e: set volume"
out=$("$SVOLDER" --transport ble --adapter "$SCAN" -d "$UNIT" set volume 100) || fail "set exited non-zero"
grep -q "100" <<<"$out" || fail "set volume not confirmed"

echo "e2e: capture"
CAPTURE_FILE="$RUN_DIR/capture.txt"
XDG_CONFIG_HOME="$ISOLATED_CONFIG" isolated "$SVOLDER" --transport ble --adapter "$SCAN" -d "$UNIT" capture --seconds 3 -o "$CAPTURE_FILE" \
  >/dev/null 2>"$RUN_DIR/capture.err" \
  || fail "capture exited non-zero: $(cat "$RUN_DIR/capture.err" 2>&1)"
[ -s "$CAPTURE_FILE" ] || fail "capture wrote no file at $CAPTURE_FILE"
for header in '== doctor ==' '== adverts ==' '== inspect ==' '== frames =='; do
  grep -qF "$header" "$CAPTURE_FILE" || fail "the capture file has no '$header' section"
done
grep -q '  sent  ' "$CAPTURE_FILE" || fail "the capture file records no sent frame"
grep -q '  received  ' "$CAPTURE_FILE" || fail "the capture file records no received frame"
if grep -qi "$UNIT" "$CAPTURE_FILE"; then
  fail "the capture file still holds the unit id $UNIT, which the scrub must take out"
fi
grep -q '0000000000000001' "$CAPTURE_FILE" \
  || fail "the capture file does not show the unit as the placeholder 0000000000000001"

echo "e2e: capture keeps ids"
KEEP_FILE="$RUN_DIR/capture-keep.txt"
XDG_CONFIG_HOME="$ISOLATED_CONFIG" isolated "$SVOLDER" --transport ble --adapter "$SCAN" -d "$UNIT" capture --keep-ids --seconds 1 -o "$KEEP_FILE" \
  >/dev/null 2>"$RUN_DIR/capture.err" \
  || fail "capture --keep-ids exited non-zero: $(cat "$RUN_DIR/capture.err" 2>&1)"
grep -qi "$UNIT" "$KEEP_FILE" || fail "capture --keep-ids did not keep the unit id $UNIT in the file"

echo "e2e: ignored live transport tests"
# One at a time: each test drives the same two controllers. Named one by one so that the
# test which stops the peripheral stays out of this group and runs in its own step below.
SVOLDER_E2E_SCAN_ADAPTER="$SCAN" SVOLDER_E2E_UNIT="$UNIT" \
  cargo test -q -p svolder-ble --test bluez_e2e -- --ignored --test-threads 1 --exact \
    scan_finds_the_peripheral \
    connect_writes_a_command_and_receives_a_status \
    a_cut_connect_leaves_the_peripheral_disconnected \
    the_transport_reports_the_adapter_it_opened \
  || fail "bluez_e2e live tests failed"

echo "e2e: link ends when the peripheral goes away"
SVOLDER_E2E_SCAN_ADAPTER="$SCAN" SVOLDER_E2E_UNIT="$UNIT" SVOLDER_E2E_SIM_PID="$SIM_PID" \
  cargo test -q -p svolder-ble --test bluez_e2e -- --ignored --test-threads 1 --exact \
    the_link_ends_when_the_peripheral_goes_away \
  || fail "bluez_e2e link-ends test failed"
# The test stopped the sim; reap it so the trap does not wait on a pid that is already gone.
stop_sim

# A scan lists what is on the air: with nothing advertising the unit it must not appear in
# any of the windows, and once the peripheral is back it must appear again.
# Wait for bluetoothd to drop the advertisement the killed sim had registered, so the scans
# below start with nothing on the air.
sleep 1
echo "e2e: scan with the peripheral gone"
if scan_lists_unit "peripheral down"; then
  fail "scan listed the peripheral while nothing was advertising it: $SCAN_LISTING"
fi

# Without --trace the sim logs no frames, so the steps below that connect to it leave nothing for
# the daemon section's frame counts to read; that section restarts the sim with --trace.
SIM_EXTRA="--profile gungnir2-v2" start_sim

echo "e2e: scan with the peripheral back"
scan_lists_unit "peripheral back" || fail "scan did not list the peripheral after it started advertising again"

# A unit resumes connectable advertising once a link to it closes, which is what lets a central
# find it again from a fresh advertisement: connect, close, scan, find the unit, connect again.
# `inspect` opens a link and closes it when it ends, so the second inspect is a second connect
# that needed the unit back on the air.
connects_again() {
  local unit="$1" what="$2" i found=
  local closed_before
  closed_before=$(grep -c 'svolder-sim: a link closed; advertising again' "$SIM_LOG" || true)
  "$SVOLDER" --transport ble --adapter "$SCAN" -d "$unit" inspect >/dev/null \
    || fail "the first connect to $what failed"
  for i in $(seq "$SCAN_TRIES"); do
    [ "$i" = 1 ] || sleep "$SCAN_BREATH"
    SCAN_LISTING=$("$SVOLDER" --transport ble --adapter "$SCAN" scan --timeout "$SCAN_WINDOW") \
      || fail "scan exited non-zero (after a link to $what closed)"
    if grep -qi "$unit" <<<"$SCAN_LISTING"; then found=1; break; fi
  done
  [ -n "$found" ] || fail "scan did not list $what after a link to it closed: $SCAN_LISTING"
  # The simulator itself saw the link close and put the advertisement back on the air.
  [ "$(grep -c 'svolder-sim: a link closed; advertising again' "$SIM_LOG" || true)" -gt "$closed_before" ] \
    || fail "the sim of $what did not log advertising again after its link closed"
  "$SVOLDER" --transport ble --adapter "$SCAN" -d "$unit" inspect >/dev/null \
    || fail "the second connect to $what, after its first link closed, failed"
}

echo "e2e: advertising resumes after a link closes"
connects_again "$UNIT" "the peripheral"

# The simulator's legacy layout is what a phone scans for, and its advertisement is a second
# registration of its own. Off by default: its scan response reaches the central only when
# bluetoothd runs with --experimental.
if [ "${SVOLDER_E2E_LEGACY:-}" = 1 ]; then
  echo "e2e: advertising resumes after a link closes (legacy layout)"
  stop_sim
  sleep 1
  SIM_UNIT="${SVOLDER_E2E_LEGACY_UNIT:-010203040506070b}" SIM_EXTRA="--layout legacy" start_sim
  connects_again "${SVOLDER_E2E_LEGACY_UNIT:-010203040506070b}" "the legacy peripheral"
fi

# --trace makes the sim log every frame it receives: the daemon section's firmware and identify
# steps read the frames they count from there, so it starts here, after the steps above that
# connect to a sim of their own. The profile is named, not left to the sim's default, because the
# firmware step counts on it listing subsystems 1 and 2.
stop_sim
sleep 1
SIM_EXTRA="--trace --profile gungnir2-v2" start_sim

# --- the daemon, over the same two controllers --------------------------------------------
# A running daemon holds the link to the peripheral, so no step that opens a direct session
# can follow this section: it is the last one before the run passes.

# Start svolderd on the central controller with a configuration of its own — a temporary
# XDG_CONFIG_HOME naming the sim's unit ($DAEMON_UNIT when set, else $UNIT) — and wait for the
# socket it serves.
start_daemon() {
  # A daemon started before has left its configuration directory behind.
  if [ -n "$DAEMON_DIR" ]; then rm -rf "$DAEMON_DIR" || true; fi
  DAEMON_DIR=$(mktemp -d)
  DAEMON_SOCKET="$DAEMON_DIR/svolder.sock"
  mkdir -p "$DAEMON_DIR/svolder"
  cat >"$DAEMON_DIR/svolder/config.toml" <<EOF
schema = 1
adapter = "$SCAN"

[[device]]
name = "e2e"
udid = "${DAEMON_UNIT:-$UNIT}"

[web]
enabled = true
listen = "127.0.0.1:0"
auth = "none"
EOF
  SVOLDER_HOST=off XDG_CONFIG_HOME="$DAEMON_DIR" "$SVOLDERD" --transport ble --socket "$DAEMON_SOCKET" \
    >"$DAEMON_LOG" 2>&1 &
  DAEMON_PID=$!
  for _ in $(seq 100); do
    [ -S "$DAEMON_SOCKET" ] && return 0
    kill -0 "$DAEMON_PID" 2>/dev/null || fail "svolderd exited before it opened its socket"
    sleep 0.1
  done
  fail "svolderd did not open its socket within 10s"
}

# Run the CLI against that daemon, with the daemon's own configuration directory so a
# developer's own configuration cannot steer it. The transport is left at its default,
# `auto`, so this is the choice a user gets, and `-v` is always passed so the run names the
# path it took for ran_over_the_daemon to check.
daemon_cli() {
  XDG_CONFIG_HOME="$DAEMON_DIR" "$SVOLDER" -v --socket "$DAEMON_SOCKET" "$@"
}

# Fail unless the run whose output is $1, the step $2 names, went to this daemon.
#
# Nothing else in the output would tell: the configuration this section wrote names the central
# and the sim's unit, so a CLI that found no daemon on the socket falls back to a direct session
# over the same radio and prints much the same readings. Only the `-v` line separates a daemon's
# answer from the CLI's own, which is what makes a dead daemon a failure here.
ran_over_the_daemon() {
  grep -q "daemon at $DAEMON_SOCKET" <<<"$1" || fail "$2 did not run over the daemon"
}

# True once the daemon reports its device connected. The daemon scans and connects on its
# own after it opens the socket, so the readings a verb asks for exist only from here on.
daemon_holds_the_device() {
  local out
  for _ in $(seq 30); do
    out=$(daemon_cli devices 2>&1) || out=''
    grep -q 'connected' <<<"$out" && ! grep -q 'disconnected' <<<"$out" && return 0
    sleep 1
  done
  return 1
}

echo "e2e: daemon starts"
start_daemon

echo "e2e: daemon connects to the peripheral"
daemon_holds_the_device || fail "the daemon did not report the peripheral connected within 30s"

echo "e2e: daemon status"
out=$(daemon_cli status 2>&1) || fail "daemon status exited non-zero (expected 0)"
ran_over_the_daemon "$out" "status"
grep -qi "Volume" <<<"$out" || fail "daemon status did not report the volume"

echo "e2e: firmware"
# The session asks `210.2.<sub>` once per link for each subsystem the profile lists, and the
# answers come back as ordinary readings. The sim runs as gungnir2-v2, which lists subsystems 1
# and 2, and answers each with its firmware build; the daemon asked as soon as it connected, so
# the readings are there within moments.
FIRMWARE_OK=
for _ in $(seq 20); do
  out=$(daemon_cli status 2>&1) || fail "daemon status for the firmware readings exited non-zero (expected 0)"
  if grep -qE 'Status 210\.2\.1 u16 3( |$)' <<<"$out" && grep -qE 'Status 210\.2\.2 u16 3( |$)' <<<"$out"; then
    FIRMWARE_OK=1
    break
  fi
  sleep 0.5
done
[ -n "$FIRMWARE_OK" ] || fail "the daemon's status has no firmware readings 210.2.1 and 210.2.2 (u16 3) within 10s: $out"
# One question per subsystem went out over the radio: at least two, since a link that dropped and
# came back is asked again, and the sim's log holds nothing but this daemon's frames. No frame of
# any other function of the firmware-update family did: the only `210.x` the daemon may send is
# the version query, `210.2.<sub>`: a bare `210` and every `210.x` but `.2` is turned away.
FIRMWARE_ASKED=$(grep -c 'Command 210\.2\.[0-9]* ' "$SIM_LOG" || true)
[ "$FIRMWARE_ASKED" -ge 2 ] || fail "the sim saw $FIRMWARE_ASKED firmware-version questions (expected at least 2)"
FIRMWARE_UPDATE=$(grep -cE 'Command 210( |\.([013-9]|2[0-9]))' "$SIM_LOG" || true)
[ "$FIRMWARE_UPDATE" = 0 ] || fail "the sim saw $FIRMWARE_UPDATE frames of a firmware-update function (expected none)"

echo "e2e: identify"
# Frames the sim has seen for function 200 before this step, so the counts below are this step's
# own. The function prints as `200` followed by a space, which no 200.x function would.
identify_frames() {
  grep -c 'Command 200 ' "$SIM_LOG" || true
}
# The two calls are meant to land inside the daemon's 2 s gap; a host that stalls between them
# (a slow `svolder` start, a busy machine) can let the gap pass, so the pair is timed and, when
# more than 1.5 s passed between the calls and the second sent a frame of its own, run once more
# from a quiet start (the late frames have reached the sim and the gap is long over).
now_ms() {
  echo $(( $(date +%s%N) / 1000000 ))
}
for attempt in 1 2; do
  IDENTIFY_BEFORE=$(identify_frames)
  out=$(daemon_cli identify 2>&1) || fail "daemon identify exited non-zero (expected 0): $out"
  FIRST_AT=$(now_ms)
  ran_over_the_daemon "$out" "identify"
  grep -q 'identify sent to ' <<<"$out" || fail "identify did not say it sent the frame: $out"
  out=$(daemon_cli identify 2>&1) || fail "the second daemon identify exited non-zero (expected 0): $out"
  PAIR_GAP_MS=$(( $(now_ms) - FIRST_AT ))
  if [ "$attempt" = 1 ] && [ "$PAIR_GAP_MS" -gt 1500 ] \
    && ! grep -q 'identify just sent to ' <<<"$out"; then
    echo "e2e: identify: ${PAIR_GAP_MS} ms between the two calls, so the gap passed; once more"
    sleep 3
    continue
  fi
  # The second call lands inside the daemon's 2 s gap and sends nothing.
  grep -q 'identify just sent to ' <<<"$out" || fail "a second identify inside 2 s did not say 'just sent': $out"
  break
done
# The first frame reaches the sim within moments; wait for it, then give a second frame the same
# chance to show up before counting.
for _ in $(seq 50); do
  [ "$(identify_frames)" -gt "$IDENTIFY_BEFORE" ] && break
  sleep 0.1
done
sleep 1
IDENTIFY_SEEN=$(( $(identify_frames) - IDENTIFY_BEFORE ))
[ "$IDENTIFY_SEEN" = 1 ] \
  || fail "the sim saw $IDENTIFY_SEEN identify frames (function 200) for two calls inside 2 s (expected exactly 1)"

# Not the `signal` step further down, which is the sim's meter signal: this one is how strongly
# the daemon hears the peripheral.
echo "e2e: device signal strength"
out=$(daemon_cli devices --json 2>/dev/null) || fail "daemon devices --json exited non-zero (expected 0)"
# `devices --json` prints one object per device: the signal read is the one in the object of the
# sim's unit.
DEVICE_OBJECT=$(grep -iF "\"unit\":\"$UNIT\"" <<<"$out" | sed -n 1p || true)
[ -n "$DEVICE_OBJECT" ] || fail "devices --json lists no device with the unit $UNIT: $out"
SIGNAL_OBJECT=$(grep -oE '"signal":\{[^}]*\}' <<<"$DEVICE_OBJECT" | sed -n 1p || true)
[ -n "$SIGNAL_OBJECT" ] || fail "devices --json carries no signal for the device: $out"
grep -q '"source":"advert"' <<<"$SIGNAL_OBJECT" \
  || fail "the signal's source is not \"advert\": $SIGNAL_OBJECT"
grep -qE '"dbm":-[0-9]+' <<<"$SIGNAL_OBJECT" \
  || fail "the signal's dbm is not a negative number: $SIGNAL_OBJECT"

echo "e2e: daemon set volume"
out=$(daemon_cli set volume 100 2>&1) || fail "daemon set volume exited non-zero (expected 0)"
ran_over_the_daemon "$out" "set volume"
grep -q "100" <<<"$out" || fail "daemon set volume not confirmed"

echo "e2e: a volume write through the web UI"
# The web UI listens on a port the kernel chose; `daemon status` names it (`svolder status` is the
# device's readings). A lowering from 100 lands at once, and the page's own origin is what a
# browser sends with a POST.
WEB_ADDR=$(daemon_cli daemon status 2>&1 | sed -n 's/^web: listening on \([0-9.]*:[0-9]*\) .*/\1/p')
[ -n "$WEB_ADDR" ] || fail "daemon status named no web UI address"
code=$(curl -s -o /dev/null -w '%{http_code}' -X POST \
  -H 'Content-Type: application/json' -H "Origin: http://$WEB_ADDR" \
  --data '{"raw":90}' "http://$WEB_ADDR/api/v1/devices/e2e/volume") \
  || fail "curl to the web UI exited non-zero (expected 0)"
[ "$code" = 200 ] || fail "the web UI answered the volume write with $code (expected 200)"
# `get volume` prints the converted value (dB); `--json` carries the raw step the page wrote.
out=$(daemon_cli get volume --json 2>&1) || fail "daemon get volume exited non-zero (expected 0)"
ran_over_the_daemon "$out" "get volume after the web write"
grep -q '"raw":90,' <<<"$out" || fail "the volume the web UI wrote did not reach the device: $out"

echo "e2e: daemon watch"
# `watch` runs until something stops it, so `timeout` is what ends it: 124 is the timeout
# doing that and 0 is the CLI having ended first; any other code is the CLI failing. Spelled
# out rather than run through daemon_cli, because `timeout` takes a program and not a function.
WATCH_CODE=0
out=$(XDG_CONFIG_HOME="$DAEMON_DIR" timeout 3 "$SVOLDER" -v --socket "$DAEMON_SOCKET" watch 2>&1) \
  || WATCH_CODE=$?
case "$WATCH_CODE" in
  0 | 124) ;;
  *) fail "daemon watch exited $WATCH_CODE (expected 0 or 124)" ;;
esac
ran_over_the_daemon "$out" "watch"
grep -qi "Volume" <<<"$out" || fail "daemon watch printed no status line"

echo "e2e: daemon reloads on SIGHUP"
kill -HUP "$DAEMON_PID" || fail "kill -HUP of the daemon exited non-zero (expected 0)"
# The daemon logs the reload as it starts one, so that line is what says the signal reached the
# reload loop; surviving the signal alone would not, and neither would a reading afterwards.
# The line after it is what says the reload finished: the daemon reconciles its devices with
# the file it just re-read and names the ones it serves now.
for _ in $(seq 50); do
  grep -q 'reloading the configuration and the profiles' "$DAEMON_LOG" && break
  sleep 0.1
done
grep -q 'reloading the configuration and the profiles' "$DAEMON_LOG" \
  || fail "the daemon logged no reload within 5s of the SIGHUP"
for _ in $(seq 50); do
  grep -q 'reloaded, serving ' "$DAEMON_LOG" && break
  sleep 0.1
done
grep -q 'reloaded, serving ' "$DAEMON_LOG" \
  || fail "the daemon started a reload but never finished one within 5s"
out=$(daemon_cli status 2>&1) || fail "daemon status after SIGHUP exited non-zero (expected 0)"
ran_over_the_daemon "$out" "status after SIGHUP"
grep -qi "Volume" <<<"$out" || fail "daemon status after SIGHUP did not report the volume"

# svolder stops the daemon that holds the device, records, and starts a daemon again on the same
# socket. That daemon is not this script's child (it has a session of its own), so stop_daemon
# asks it to stop over the socket.
# Through daemon_cli the capture has its own --socket and the configuration the daemon was given;
# `isolated` keeps it off the host's own systemd units.
echo "e2e: capture hands the daemon back"
DAEMON_CAPTURE_FILE="$RUN_DIR/capture-daemon.txt"
isolated daemon_cli capture --transport ble --yes --seconds 1 -o "$DAEMON_CAPTURE_FILE" \
  >/dev/null 2>"$RUN_DIR/capture.err" \
  || fail "capture over a running daemon exited non-zero: $(cat "$RUN_DIR/capture.err" 2>&1)"
[ -s "$DAEMON_CAPTURE_FILE" ] || fail "capture over a running daemon wrote no file"
daemon_holds_the_device \
  || fail "the daemon was not back and connected within 30s of the capture that stopped it"

echo "e2e: daemon stops"
stop_daemon || fail "the daemon's socket was still there 5s after the daemon was killed"

echo "e2e: signal"
stop_sim
SIM_UNIT="$SIGNAL_UNIT" SIM_EXTRA="--profile fake-amp --signal" start_sim
# `watch` on a device the configuration does not name spends one scan window learning the
# device before it connects, so its first meter line is about ten seconds away. The watch runs
# in the background until it has printed three readings of the left meter or SIGNAL_BUDGET
# seconds have passed, and Ctrl-C ends it the way a user would, so the link is released rather
# than left with BlueZ; a peripheral still held would not advertise for the next scan. It scans
# for $SIGNAL_UNIT rather than $UNIT — see that variable's own comment above.
SIGNAL_BUDGET=25
SIGNAL_OUT=$(mktemp)
"$SVOLDER" --transport ble --adapter "$SCAN" -d "$SIGNAL_UNIT" watch >"$SIGNAL_OUT" 2>&1 &
WATCH_PID=$!
for _ in $(seq $((SIGNAL_BUDGET * 2))); do
  [ "$(grep -c 'Meter L' "$SIGNAL_OUT")" -ge 3 ] && break
  kill -0 "$WATCH_PID" 2>/dev/null || break
  sleep 0.5
done
kill -INT "$WATCH_PID" 2>/dev/null || true
for _ in $(seq 10); do
  kill -0 "$WATCH_PID" 2>/dev/null || break
  sleep 0.5
done
if kill -0 "$WATCH_PID" 2>/dev/null; then
  kill "$WATCH_PID" 2>/dev/null || true
  fail "signal watch did not end within 5s of Ctrl-C: $(cat "$SIGNAL_OUT")"
fi
SIGNAL_CODE=0
wait "$WATCH_PID" || SIGNAL_CODE=$?
[ "$SIGNAL_CODE" = 0 ] || fail "signal watch exited $SIGNAL_CODE (expected 0 on Ctrl-C): $(cat "$SIGNAL_OUT")"
[ "$(grep -c 'Meter L' "$SIGNAL_OUT")" -ge 3 ] \
  || fail "the simulator's signal did not reach watch within ${SIGNAL_BUDGET}s: $(cat "$SIGNAL_OUT")"
stop_sim

# --- a Loki Max: its presets over the radio -------------------------------------------------
# The simulator is a Loki Max that reports a band on its two-id form, so each reading below goes
# through the alias read, and traces every frame it receives. The bands start flat and the three
# slots hold Flat. `get` prints the converted reading: raw 20 on band 1 is 4.0 dB and raw 28 is
# 10.4 dB (the band spans -12 to +12 dB over raw 0 to 30).
echo "e2e: a Loki Max reports a band on its short id"
SIM_UNIT="$LOKI_UNIT" SIM_EXTRA="--trace --profile lokimax --loki-reports short" start_sim
loki_cli() {
  "$SVOLDER" --transport ble --adapter "$SCAN" -d "$LOKI_UNIT" "$@"
}
out=$(loki_cli set band1 20 2>&1) || fail "set band1 on the Loki Max exited non-zero (expected 0): $out"
out=$(loki_cli get band1 2>&1) || fail "get band1 on the Loki Max exited non-zero (expected 0): $out"
grep -qE '(^| )4\.0 dB' <<<"$out" \
  || fail "the band written on 6.1.3 did not read back as 4.0 dB when the unit reports on 6.1: $out"

echo "e2e: a Loki Max applies a built-in setting"
out=$(loki_cli preset apply "Bass Boost" 2>&1) || fail "preset apply exited non-zero (expected 0): $out"
grep -q 'Bass Boost on' <<<"$out" || fail "preset apply did not say the setting is on: $out"
out=$(loki_cli get band1 2>&1) || fail "get band1 after the setting exited non-zero (expected 0): $out"
grep -qE '(^| )10\.4 dB' <<<"$out" || fail "band 1 did not read 10.4 dB (raw 28) after Bass Boost: $out"

echo "e2e: a Loki Max saves the bands and recalls them"
out=$(loki_cli set save 2 --yes 2>&1) || fail "set save 2 exited non-zero (expected 0): $out"
out=$(loki_cli preset apply Flat 2>&1) || fail "preset apply Flat exited non-zero (expected 0): $out"
out=$(loki_cli get band1 2>&1) || fail "get band1 after Flat exited non-zero (expected 0): $out"
grep -qE '(^| )0\.0 dB' <<<"$out" || fail "band 1 did not read 0.0 dB after Flat: $out"
out=$(loki_cli set recall 2 2>&1) || fail "set recall 2 exited non-zero (expected 0): $out"
out=$(loki_cli get band1 2>&1) || fail "get band1 after the recall exited non-zero (expected 0): $out"
grep -qE '(^| )10\.4 dB' <<<"$out" || fail "band 1 did not read 10.4 dB after recalling the slot that held Bass Boost: $out"
# A recall is one command on 7.2.<slot> with no data; the trace prints the empty data as nothing.
LOKI_RECALLS=$(grep -cE 'Command 7\.2\.2 +from' "$SIM_LOG" || true)
[ "$LOKI_RECALLS" = 1 ] || fail "the sim saw $LOKI_RECALLS recall frames on 7.2.2 (expected exactly 1)"
LOKI_SAVES=$(grep -cE 'Command 7\.1\.2 +from' "$SIM_LOG" || true)
[ "$LOKI_SAVES" = 1 ] || fail "the sim saw $LOKI_SAVES save frames on 7.1.2 (expected exactly 1)"

# A unit that sets its bands on a recall and says nothing is asked for everything once, a second
# later: the daemon's reading of band 1 catches up, and the sim sees one refresh (function 1) more.
echo "e2e: a Loki Max that stays silent after a recall is asked once"
stop_sim
SIM_UNIT="$LOKI_UNIT" SIM_EXTRA="--trace --profile lokimax --loki-reports short --silent-recall" start_sim
DAEMON_UNIT="$LOKI_UNIT" start_daemon
daemon_holds_the_device || fail "the daemon did not report the Loki Max connected within 30s"
out=$(daemon_cli set band1 20 2>&1) || fail "daemon set band1 exited non-zero (expected 0): $out"
out=$(daemon_cli set save 1 --yes 2>&1) || fail "daemon set save 1 exited non-zero (expected 0): $out"
ran_over_the_daemon "$out" "set save"
out=$(daemon_cli set band1 10 2>&1) || fail "the second daemon set band1 exited non-zero (expected 0): $out"
# The connect's own refresh is over by now; this step's refreshes are counted from here.
sleep 1
refresh_frames() {
  grep -cE 'Command 1 +from' "$SIM_LOG" || true
}
REFRESH_BEFORE=$(refresh_frames)
out=$(daemon_cli set recall 1 2>&1) || fail "daemon set recall 1 exited non-zero (expected 0): $out"
ran_over_the_daemon "$out" "set recall"
CAUGHT_UP=
for _ in $(seq 40); do
  out=$(daemon_cli get band1 --json 2>&1) || fail "daemon get band1 exited non-zero (expected 0): $out"
  if grep -q '"raw":20,' <<<"$out"; then CAUGHT_UP=1; break; fi
  sleep 0.25
done
[ -n "$CAUGHT_UP" ] || fail "the daemon's band 1 never caught up to the recalled 20 within 10s: $out"
# Give a second refresh the same chance to show up before counting.
sleep 3
REFRESH_SEEN=$(( $(refresh_frames) - REFRESH_BEFORE ))
[ "$REFRESH_SEEN" = 1 ] \
  || fail "the sim saw $REFRESH_SEEN refresh frames after a silent recall (expected exactly 1)"
stop_daemon || fail "the daemon's socket was still there 5s after the daemon was killed"
stop_sim

PASSED=1
echo "e2e: PASS"
