#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# make web-shots: the web UI's pictures. Throwaway daemons on the fake transport, each with its
# own homes, socket and loopback port, serve a DAC, an amp, the sign-in page and a lost link;
# headless Firefox and headless Chromium shoot each at a phone's width (390x844) and a
# desktop's (1280x900; the DAC's is 1280x1700, its phone's 390x1000) into target/web-shots/.
# The DAC's daemon runs the fake host (SVOLDER_HOST=fake: a player with a cover and a route), so
# its page shows Now playing (the band on a desktop, the folded line and the visualizer card under
# the volume on a phone); Chromium's DAC shots are taken by scripts/web-shot.py, which waits until
# the visualizer has drawn frames. The others run with no host. A run fails on any
# uncaught error or Content Security Policy report Chromium logs, and on any uncaught error
# Firefox logs for a page (its console on stdout, from a scratch profile's user.js). Firefox
# cannot be gated on CSP reports: in headless screenshot mode it enforces the policy but writes
# no violation to stdout, stderr or the console (checked with devtools.console.stdout.content
# and .chrome, browser.dom.window.dump.enabled and MOZ_LOG=CSP,CSMLog); only a report-uri
# endpoint hears of it. The policy is the daemon's header, the same for both browsers, so
# Chromium's check stands for it. Firefox's grep still lists the CSP words, a harmless backstop
# that would catch a future Firefox which logs them. The README's two pictures are Chromium's
# phone and desktop DAC shots (at 2x: Firefox's headless screenshot ignores a device pixel
# ratio), encoded to lossless WebP into docs/images/ as make screenshots encodes its stills (a
# picture over its 300 KB budget is encoded at a near-lossless quality instead). The pages that
# carry the effects layer are shot with ?effects=glow: the level for that view only (nothing is
# stored), so the pictures show the glow whatever the default level is.
set -euo pipefail

root=$(dirname "$(dirname "$(readlink -f "$0")")")
out=$root/target/web-shots
images=$root/docs/images
cargo=${CARGO:-cargo}
correction="$root/target/screenshots/autoeq/Sennheiser HD 800 S ParametricEQ.txt"

missing=()
command -v firefox >/dev/null || missing+=("firefox")
command -v chromium >/dev/null || missing+=("chromium")
command -v curl >/dev/null || missing+=("curl")
command -v python3 >/dev/null || missing+=("python3")
python3 -c 'import websockets' 2>/dev/null || missing+=("python3-websockets")
# Captured first: grep -q ending a pipe early would fail it under pipefail.
encoders=$(ffmpeg -hide_banner -encoders 2>/dev/null || true)
grep -q ' libwebp ' <<<"$encoders" || missing+=("ffmpeg with libwebp (package ffmpeg)")
[ -r "$correction" ] || missing+=("the cached AutoEq file: run make screenshots once")
if [ ${#missing[@]} -gt 0 ]; then
    printf 'web-shots: missing %s\n' "${missing[@]}" >&2
    exit 1
fi

"$cargo" build -q --manifest-path "$root/Cargo.toml" -p svolder -p svolderd
svolder=$root/target/debug/svolder
svolderd=$root/target/debug/svolderd

mkdir -p "$out"
find "$out" -mindepth 1 -maxdepth 1 -exec rm -rf {} +
# Under target/, not /tmp: both browsers are snaps, which see the home directory only.
scratch=$(mktemp -d -p "$out")
pids=()
cleanup() {
    for pid in "${pids[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    rm -rf "$scratch"
}
trap cleanup EXIT

# A free loopback port, for a daemon's web UI.
free_port() {
    python3 -I -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); \
print(s.getsockname()[1])'
}

# The environment every run of a built binary here gets: the homes under $1, nothing of the
# user's.
homes() {
    local home=$1
    echo XDG_CONFIG_HOME="$home/config" XDG_STATE_HOME="$home/state" \
        XDG_DATA_HOME="$home/data" XDG_CACHE_HOME="$home/cache" XDG_RUNTIME_DIR="$home/run"
}

# start NAME DEVICE PROFILE PORT WEB-LINES [VAR=VALUE...]: a daemon serving DEVICE (a fake
# PROFILE) and its web UI on PORT with WEB-LINES in its [web] table, waited for until the page
# answers.
start() {
    local name=$1 device=$2 profile=$3 port=$4 web=$5
    shift 5
    local home=$scratch/$name
    mkdir -p -m 700 "$home" "$home/config/svolder" "$home/state" "$home/data" \
        "$home/cache" "$home/run"
    # This heredoc expands on purpose: the device's name, the port and the [web] lines.
    cat >"$home/config/svolder/config.toml" <<EOT
schema = 1

[[device]]
name = "$device"
udid = "0102030405060708"
passphrase = "1234"

[web]
enabled = true
listen = "127.0.0.1:$port"
$web
EOT
    # shellcheck disable=SC2046
    env -i PATH="$PATH" HOME="$HOME" $(homes "$home") SVOLDER_TRANSPORT=fake \
        SVOLDER_FAKE_PROFILE="$profile" SVOLDER_HOST=off "$@" \
        "$svolderd" --socket "$home/run/svolder.sock" \
        --config "$home/config/svolder/config.toml" >"$out/$name-daemon.log" 2>&1 &
    pids+=($!)
    for _ in $(seq 100); do
        curl -s -o /dev/null "http://127.0.0.1:$port/assets/themes/ember/style.css" && return 0
        sleep 0.1
    done
    echo "web-shots: the $name daemon's web UI did not answer; see $out/$name-daemon.log" >&2
    exit 1
}

# api PORT PATH JSON: a write through a daemon's web UI, from its own origin.
api() {
    curl -sf -o /dev/null -X POST -H 'Content-Type: application/json' \
        -H "Origin: http://127.0.0.1:$1" --data "$3" \
        "http://127.0.0.1:$1/api/v1/devices/$2" \
        || { echo "web-shots: the write to $2 was refused" >&2; exit 1; }
}

# connected PORT DEVICE: wait until the device's model holds a volume reading.
connected() {
    for _ in $(seq 100); do
        curl -s "http://127.0.0.1:$1/api/v1/devices/$2/model" \
            | grep -q '"raw":[0-9]' && return 0
        sleep 0.1
    done
    echo "web-shots: $2 never reported a volume" >&2
    exit 1
}

dac_port=$(free_port)
start dac "Gungnir 2" gungnir2-v2 "$dac_port" 'auth = "none"' SVOLDER_HOST=fake
amp_port=$(free_port)
start amp "Amp" fake-amp "$amp_port" 'auth = "none"' SVOLDER_FAKE_SIGNAL=1
signin_port=$(free_port)
start signin "Gungnir 2" gungnir2-v2 "$signin_port" '' \
    SVOLDER_WEB_TOKEN=web-shots-token-0123456789abcdefghijkl
lost_port=$(free_port)
start lost "Gungnir 2" gungnir2-v2 "$lost_port" 'auth = "none"' SVOLDER_FAKE_GONE_AFTER_MS=3000

# The DAC as the TUI's pictures stage it: the HD 800 S correction imported (config only) and
# recalled, the volume at -24.0 dB; then an input change and a recall during a 75 s wait, so the
# strip has a minute of bars and its marks.
dac=Gungnir%202
connected "$dac_port" "$dac"
# shellcheck disable=SC2046
env -i PATH="$PATH" HOME="$HOME" $(homes "$scratch/dac") SVOLDER_TRANSPORT=fake \
    "$svolder" eq import "$correction" >/dev/null
api "$dac_port" "$dac/presets/recall" '{"name":"Sennheiser HD 800 S"}'
api "$dac_port" "$dac/controls/volume" '{"value":-24}'
sleep 25
api "$dac_port" "$dac/controls/input" '{"value":"Coaxial"}'
sleep 20
api "$dac_port" "$dac/presets/recall" '{"name":"Warm"}'
sleep 10
api "$dac_port" "$dac/presets/recall" '{"name":"Sennheiser HD 800 S"}'
sleep 20

# shoot BROWSER SIZE NAME URL: one picture.
shoot() {
    local browser=$1 size=$2 name=$3 url=$4 profile log
    local file=$out/$browser-$size-$name.png
    log=$out/$browser-$size-$name.log
    local window=390,844
    [ "$size" = desk ] && window=1280,900
    # The DAC's desktop picture is taller than a screen so the strip at the page's foot is in it.
    if [ "$name" = dac ]; then
        window=390,${WEB_SHOTS_PHONE_H:-1000}
        [ "$size" = desk ] && window=1280,${WEB_SHOTS_DESK_H:-1700}
    fi
    profile=$(mktemp -d -p "$scratch")
    case $browser in
        firefox)
            cat >"$profile/user.js" <<'EOT'
user_pref("layout.css.devPixelsPerPx", "2");
user_pref("devtools.console.stdout.content", true);
user_pref("browser.dom.window.dump.enabled", true);
EOT
            timeout 120 firefox --headless --no-remote --profile "$profile" \
                --window-size "$window" --screenshot "$file" "$url" >"$log" 2>&1 \
                || { echo "web-shots: firefox failed on the $name page" >&2; exit 1; }
            # Only the page's own errors: the browser's internals log "resource://" ones.
            if grep -E '^JavaScript error: http|Content.Security.Policy|Refused to' "$log"; then
                echo "web-shots: firefox logged an error on the $name page ($size)" >&2
                exit 1
            fi
            ;;
        chromium)
            if [ "$name" = dac ]; then
                # The DAC's page has the visualizer card: the shot waits for its frames.
                timeout 180 python3 "$root/scripts/web-shot.py" --expect-card "$url" "$file" \
                    "${window%,*}" "${window#*,}" >"$log" 2>&1 \
                    || { cat "$log" >&2; echo "web-shots: chromium failed on the dac page" >&2
                         exit 1; }
                [ -s "$file" ] || { echo "web-shots: chromium drew no $file" >&2; exit 1; }
                return 0
            fi
            timeout 120 chromium --headless --disable-gpu --hide-scrollbars \
                --force-device-scale-factor=2 --user-data-dir="$profile" \
                --window-size="$window" --timeout=5000 --enable-logging=stderr \
                --v="${WEB_SHOTS_VERBOSITY:-0}" --screenshot="$file" "$url" >"$log" 2>&1 \
                || { echo "web-shots: chromium failed on the $name page" >&2; exit 1; }
            if grep -E 'Uncaught|Content Security Policy|Refused to' "$log"; then
                echo "web-shots: chromium logged an error on the $name page ($size)" >&2
                exit 1
            fi
            ;;
    esac
    [ -s "$file" ] || { echo "web-shots: $browser drew no $file" >&2; exit 1; }
}

# The lost-link daemon's device went off the air a minute ago; it is between attempts.
for browser in firefox chromium; do
    for size in phone desk; do
        shoot "$browser" "$size" dac "http://127.0.0.1:$dac_port/?effects=glow"
        shoot "$browser" "$size" amp "http://127.0.0.1:$amp_port/?effects=glow"
        shoot "$browser" "$size" signin "http://127.0.0.1:$signin_port/"
        shoot "$browser" "$size" lost "http://127.0.0.1:$lost_port/?effects=glow"
    done
done

# The README's two pictures, lossless WebP as make screenshots encodes its stills.
mkdir -p "$images"
for pair in phone:web-phone desk:web-desktop; do
    ffmpeg -v error -y -fflags +bitexact -i "$out/chromium-${pair%%:*}-dac.png" \
        -map_metadata -1 -flags:v +bitexact -c:v libwebp -lossless 1 -compression_level 6 \
        "$images/${pair#*:}.webp"
    size=$(stat -c %s "$images/${pair#*:}.webp")
    if [ "$size" -gt 300000 ]; then
        # Too big lossless at 2x: the same picture at a near-lossless quality.
        ffmpeg -v error -y -fflags +bitexact -i "$out/chromium-${pair%%:*}-dac.png" \
            -map_metadata -1 -flags:v +bitexact -c:v libwebp -quality 92 -compression_level 6 \
            "$images/${pair#*:}.webp"
        size=$(stat -c %s "$images/${pair#*:}.webp")
    fi
    if [ "$size" -gt 300000 ]; then
        echo "web-shots: ${pair#*:}.webp is $size bytes, over its 300000 budget" >&2
        exit 1
    fi
    printf 'web-shots: %s is %d bytes, the budget is 300000\n' "${pair#*:}.webp" "$size"
done
printf 'web-shots: done: %s\n' "$out"
