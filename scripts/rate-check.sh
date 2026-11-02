#!/bin/bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later

# Check by hand that the host capture leaves the sound card's rate to the player. On a PipeWire
# whose `clock.allowed-rates` holds several rates, a capture of the sink's monitor can hold the
# graph at one rate, so a 44.1 kHz track is resampled while the visualizer watches. The script
# plays a silent 44.1 kHz stream to the sink, first with no capture and then with the capture the
# daemon and the TUI start, and reads the graph's rate from `pw-top` each time.
#
# It uses the real PipeWire session (the streams are silent, nothing is set), so it is never part
# of `make check`. It refuses to run while the sink has a running stream, so nothing you play is
# disturbed or measured.
#
# Usage: scripts/rate-check.sh [SINK-NODE-NAME]    (default: the default sink's node.name)
#        scripts/rate-check.sh --self-test         (the parsers against saved tool output; no
#                                                   PipeWire; part of `make check`)
# Needs: pw-cat, pw-record, pw-top, pw-dump, pw-metadata, wpctl, python3.
# Exit: 0 PASS, 1 FAIL, 2 inconclusive or refused.

set -u

# The same `-P` properties as `pw_record_args` in crates/svolder-host/src/capture.rs; a test
# there reads this file, so the two cannot drift apart.
CAPTURE_PROPS='{ stream.capture.sink = true node.passive = true node.rate = 0 }'
PLAY_RATE=44100
SETTLE=7

# The parsers, each reading what the tool printed, so `--self-test` can run them on saved text.

# The default sink's node.name, from `wpctl inspect @DEFAULT_AUDIO_SINK@` on stdin.
parse_sink_name() {
    sed -n 's/^[ *]*node\.name = "\(.*\)"$/\1/p' | head -n 1
}

# The allowed rates, space separated, from `pw-metadata -n settings 0 clock.allowed-rates` on stdin.
parse_allowed_rates() {
    sed -n "s/.*value:'\[\(.*\)\]'.*/\1/p"
}

# The graph's rate on the sink's row of `pw-top -b` on stdin (the RATE column, the last row of
# the sink in the output wins), 0 when the sink is suspended or absent.
parse_graph_rate() {
    awk -v s="$1" '$NF == s { rate = $4 } END { print rate + 0 }'
}

# Exit 2 and say why when the sink named $1 is running or a running node feeds it, from the
# `pw-dump` JSON in the file $2; exit 0 when it is free.
sink_is_free() {
    python3 - "$1" "$2" <<'PY'
import json, sys

sink, path = sys.argv[1], sys.argv[2]
objects = json.load(open(path))
nodes = {o["id"]: o for o in objects if o.get("type") == "PipeWire:Interface:Node"}
ids = [i for i, o in nodes.items() if o["info"]["props"].get("node.name") == sink]
if not ids:
    print(f"rate-check: no node named {sink}", file=sys.stderr)
    sys.exit(2)
sink_id = ids[0]
busy = []
if nodes[sink_id]["info"].get("state") == "running":
    busy.append(sink)
for o in objects:
    if o.get("type") != "PipeWire:Interface:Link":
        continue
    info = o["info"]
    if info.get("input-node-id") != sink_id:
        continue
    source = nodes.get(info.get("output-node-id"))
    if source and source["info"].get("state") == "running":
        props = source["info"]["props"]
        busy.append(props.get("node.name") or props.get("application.name") or "a stream")
if busy:
    names = ", ".join(busy)
    print(f"rate-check: refusing to run, {sink} is in use by {names}", file=sys.stderr)
    print("rate-check: stop what plays on it and run again", file=sys.stderr)
    sys.exit(2)
PY
}

# The parsers against saved tool output (the real shape of each, see the README beside them).
self_test() {
    local here fx dumps failed=0
    here=$(dirname "$(readlink -f "$0")")
    fx="$here/../crates/svolder-host/tests/fixtures/rate-check"
    dumps="$here/../crates/svolder-host/tests/fixtures/pw-dump"
    command -v python3 >/dev/null 2>&1 || {
        echo "rate-check: python3 is not installed" >&2
        exit 2
    }
    expect() { # name want got
        if [ "$2" = "$3" ]; then
            echo "ok   $1"
        else
            echo "FAIL $1: wanted '$2', got '$3'"
            failed=$((failed + 1))
        fi
    }
    local hdmi=alsa_output.pci-0000_11_00.1.hdmi-stereo
    local usb=alsa_output.usb-Generic_USB_Audio-00.HiFi__Speaker__sink

    expect "the default sink's name" "$hdmi" "$(parse_sink_name <"$fx/wpctl-inspect.txt")"
    expect "no sink name from text that has none" "" "$(echo 'Node not found' | parse_sink_name)"
    expect "the allowed rates" "44100 48000 88200 96000 176400 192000" \
        "$(parse_allowed_rates <"$fx/pw-metadata.txt" | xargs)"
    expect "no allowed rates when the key is unset" "" \
        "$(echo "Found \"settings\" metadata 32" | parse_allowed_rates)"
    expect "an idle sink's graph rate is 0" 0 "$(parse_graph_rate "$hdmi" <"$fx/pw-top-idle.txt")"
    expect "a running sink's graph rate is read from its row" 44100 \
        "$(parse_graph_rate "$hdmi" <"$fx/pw-top-running.txt")"
    expect "another node's rate is not the sink's" 0 \
        "$(parse_graph_rate "$usb" <"$fx/pw-top-running.txt")"
    expect "a sink absent from pw-top reads 0" 0 \
        "$(parse_graph_rate nothing <"$fx/pw-top-running.txt")"

    local out rc
    out=$(sink_is_free "$hdmi" "$dumps/one-stream.json" 2>&1); rc=$?
    expect "a running sink is refused" 2 "$rc"
    expect "  and the stream feeding it is named" 1 \
        "$(echo "$out" | grep -c 'in use by .*pw-cat')"
    out=$(sink_is_free "$usb" "$dumps/one-stream.json" 2>&1); rc=$?
    expect "a suspended sink nothing feeds is free" "0:" "$rc:$out"
    out=$(sink_is_free "$usb" "$dumps/stream-elsewhere.json" 2>&1); rc=$?
    expect "a running sink is refused (the other one)" 2 "$rc"
    out=$(sink_is_free "$hdmi" "$dumps/schiit-sink.json" 2>&1); rc=$?
    expect "a suspended sink is free while a stream runs on another" "0:" "$rc:$out"
    out=$(sink_is_free nothing "$dumps/one-stream.json" 2>&1); rc=$?
    expect "an unknown sink is refused" 2 "$rc"
    expect "  and says so" 1 "$(echo "$out" | grep -c 'no node named nothing')"

    if [ "$failed" -gt 0 ]; then
        echo "rate-check self-test: $failed failed"
        exit 1
    fi
    echo "rate-check self-test: all parsers read the saved output"
    exit 0
}

[ "${1:-}" = "--self-test" ] && self_test

for tool in pw-cat pw-record pw-top pw-dump pw-metadata wpctl python3; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "rate-check: $tool is not installed" >&2
        exit 2
    }
done

sink=${1:-}
if [ -z "$sink" ]; then
    sink=$(wpctl inspect @DEFAULT_AUDIO_SINK@ 2>/dev/null | parse_sink_name)
fi
[ -n "$sink" ] || {
    echo "rate-check: no default sink; name one: scripts/rate-check.sh NODE-NAME" >&2
    exit 2
}

workdir=$(mktemp -d)
trap 'jobs -p | xargs -r kill 2>/dev/null; rm -rf "$workdir"' EXIT

# Refuse while the sink or a stream feeding it is running.
pw-dump >"$workdir/dump.json" 2>/dev/null
sink_is_free "$sink" "$workdir/dump.json" || exit 2

allowed=$(pw-metadata -n settings 0 clock.allowed-rates 2>/dev/null | parse_allowed_rates)
count=$(echo "$allowed" | wc -w)
echo "sink: $sink"
echo "allowed rates: ${allowed:-unknown}"

graph_rate() {
    pw-top -b -n 3 2>/dev/null | parse_graph_rate "$sink"
}

play() {
    timeout 4 pw-cat --playback --raw --rate "$PLAY_RATE" --channels 2 --format s16 \
        --target "$sink" - </dev/zero >/dev/null 2>&1 &
}

settle() {
    sleep "$SETTLE"
}

echo "A: no capture, a silent $PLAY_RATE Hz stream"
play
without=$(graph_rate)
wait
echo "   graph rate: $without"
settle

echo "B: the capture's properties running, then the same stream"
timeout 12 pw-record --raw --format f32 --rate 48000 --channels 2 -P "$CAPTURE_PROPS" \
    --target "$sink" - >"$workdir/samples" 2>/dev/null &
before=$(graph_rate)
echo "   graph rate before the stream: $before"
play
with=$(graph_rate)
wait
bytes=$(wc -c <"$workdir/samples")
echo "   graph rate: $with ($bytes bytes captured)"

if [ "$count" -lt 2 ]; then
    echo "inconclusive: only one rate is allowed, so the graph cannot move"
    echo "  (set clock.allowed-rates in pipewire.conf.d to several rates and run again)"
    exit 2
fi
if [ "$without" != "$PLAY_RATE" ]; then
    echo "inconclusive: with no capture the graph did not follow the stream ($without Hz);"
    echo "  the sink may not support $PLAY_RATE Hz or the rate is forced"
    exit 2
fi
if [ "$with" = "$PLAY_RATE" ]; then
    echo "PASS: the graph followed the $PLAY_RATE Hz stream with the capture running"
    exit 0
fi
echo "FAIL: the capture held the graph at $with Hz; a $PLAY_RATE Hz stream would be resampled"
exit 1
