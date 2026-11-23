#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# make ha-shots: the Home Assistant pictures of the README (docs/images/ha-dashboard.webp and
# ha-phone.webp), drawn from the custom:svolder dashboard of a Home Assistant you name.
#
# A private svolderd on the fake transport (a fake Bifrost 3 called "Bifrost 3", a fake computer
# called "Desk": the daemon runs in a bubblewrap UTS namespace whose host name is Desk, so the
# computer's device carries that name and never the real host's) publishes to that Home Assistant's
# MQTT broker under a base topic and client id of its own. Headless Chromium opens the dashboard's
# Desk page logged in with the long-lived token, with the card files of THIS checkout
# (contrib/home-assistant) served in place of the ones Home Assistant loads, and shoots the page's
# content area (no sidebar, no top bar, no other page's name) at 984 css px wide (the desktop) and
# 412 css px wide (the phone), both at twice that, into target/ha-shots/ and, encoded to lossless
# WebP, docs/images/.
#
# Home Assistant is only read. The rig adds retained topics to the broker and takes them away
# again: a trap (EXIT, INT, TERM and HUP; the cleanup itself ignores further signals and an
# interrupt exits non-zero) stops the daemon (by pid), clears exactly the retained topics the
# rig created (and refuses any other), checks the broker holds again what it held before and Home
# Assistant lists none of the rig's devices or entities, and removes the temp directories and the
# Chromium profile (0700, driven over a pipe, so no debugging port is open). A run killed outright
# leaves its daemon, its retained topics and its temp directory: the next run stops that daemon
# (found through target/ha-shots/last-rig, and only by its own socket path), clears the topics under
# the rig's base topic, its fake unit's discovery node and its own computer node (the computer node
# only when its config names the base topic), waits for Home Assistant to drop the device, and goes
# on. The password is read from its file into the daemon's environment alone.
#
# Environment (none has a default that names anyone's Home Assistant):
#   HA_URL                the Home Assistant, e.g. http://homeassistant.local:8123
#   HA_TOKEN_FILE         a file with a long-lived access token (read only; never printed)
#   HA_MQTT_PASSWORD_FILE a file with the password of the broker login
#   HA_MQTT_HOST          the broker's host (default: HA_URL's)
#   HA_MQTT_USER          the broker login (default: svolder)
#   HA_SHOTS_DASHBOARD    the url path of a dashboard on `strategy: type: custom:svolder`
#                         (default: svolder-auto)
set -uo pipefail

root=$(dirname "$(dirname "$(readlink -f "$0")")")
out=$root/target/ha-shots
images=$root/docs/images
cargo=${CARGO:-cargo}
here=$root/scripts
. "$here/tmp-base.sh"

# The rig's names: also in scripts/ha-shots.py, which looks for the entities they make.
unit=0A1B2C3D4E5F6071
unit_lower=0a1b2c3d4e5f6071
base=svolder-shots
client=svolder-shots-rig
computer=Desk
device='Bifrost 3'

missing=()
for v in HA_URL HA_TOKEN_FILE HA_MQTT_PASSWORD_FILE; do
    [ -n "${!v:-}" ] || missing+=("the environment variable $v")
done
for f in HA_TOKEN_FILE HA_MQTT_PASSWORD_FILE; do
    [ -z "${!f:-}" ] || [ -r "${!f}" ] || missing+=("a readable file at $f")
done
for tool in ffprobe chromium mosquitto_sub mosquitto_pub bwrap ffmpeg python3; do
    command -v "$tool" >/dev/null || missing+=("$tool")
done
python3 -c 'import websockets' 2>/dev/null || missing+=("python3-websockets")
encoders=$(ffmpeg -hide_banner -encoders 2>/dev/null || true)
grep -q ' libwebp ' <<<"$encoders" || missing+=("ffmpeg with libwebp (package ffmpeg)")
if [ ${#missing[@]} -gt 0 ]; then
    printf 'ha-shots: missing %s\n' "${missing[@]}" >&2
    exit 1
fi

HA_URL=${HA_URL%/}
ha_host=${HA_URL#*://}
ha_host=${ha_host%%/*}
ha_host=${ha_host%%:*}
mqtt_host=${HA_MQTT_HOST:-$ha_host}
mqtt_user=${HA_MQTT_USER:-svolder}
export HA_URL HA_TOKEN_FILE HA_SHOTS_DASHBOARD=${HA_SHOTS_DASHBOARD:-svolder-auto}
export HA_SHOTS_CHECKOUT=$root
export LC_ALL=C

# State the cleanup reads; the trap goes on before anything is created.
rig=
pid=0
rc=0
cleaning=0
snapped=0
out_profile=$out/profile
export HA_SHOTS_PROFILE=$out_profile

# Every retained topic of the rig's own: under its base topic, or in the discovery node of its fake
# unit. The computer's node (svolder_computer_desk) is the rig's only when its config says so:
# `owned_computer FILE` lists the topics of that node whose retained config names the base topic,
# and `foreign_computer FILE` those that do not (a real computer called desk).
own_re="^($base/|homeassistant/[a-z_]+/svolder_$unit/)"
cfg_filter() { grep -i -E "$own_re"; }

cleanup() {
    local code=$?
    [ "$cleaning" -eq 0 ] || return 0
    cleaning=1
    # A second Ctrl-C or TERM must not stop the cleanup half way.
    trap '' INT TERM HUP PIPE
    set +e
    [ "$code" -eq 0 ] || rc=$code
    [ "$rc" -eq 0 ] || echo "ha-shots: stopping (status $rc); cleaning up, please wait"
    if [ "$pid" -ne 0 ]; then
        # Take the rig's device out of the config so the daemon clears its discovery configs, then
        # stop the daemon, by pid. Nothing else in Home Assistant is touched.
        sv remove "$device" >/dev/null 2>&1
        cfg "$svolder" --socket "$sock" daemon stop >/dev/null 2>&1
        if kill -0 "$pid" 2>/dev/null; then kill "$pid" 2>/dev/null; fi
        wait "$pid" 2>/dev/null
        echo "ha-shots: the fake svolderd (pid $pid) is stopped"
    fi
    if [ "$snapped" -eq 1 ]; then
        clear_rig_topics "$rig/topics.after" "$rig/topics.created" || rc=1
        # Home Assistant drops a device and its entities when their discovery topics are cleared;
        # the check waits for that, a few seconds at most.
        local left=1 try
        for try in 1 2 3 4 5 6 7 8 9 10; do
            if python3 -I "$here/ha-shots.py" after >"$rig/after.txt" 2>&1; then left=0; break; fi
            python3 -c 'import time; time.sleep(2)'
        done
        cat "$rig/after.txt"
        [ "$left" -eq 0 ] || rc=1
    fi
    [ -z "$rig" ] || { rm -rf "$rig"; rm -f "$out/last-rig"; }
    rm -rf "$out_profile"
    echo "ha-shots: temp directory and Chromium profile removed"
    exit "$rc"
}
on_signal() { rc=$1; exit "$1"; }
trap cleanup EXIT
trap 'on_signal 130' INT
trap 'on_signal 143' TERM
trap 'on_signal 129' HUP

"$cargo" build -q --manifest-path "$root/Cargo.toml" -p svolder -p svolderd || exit 1
svolder=$root/target/debug/svolder
svolderd=$root/target/debug/svolderd

rig=$(svtmp_dir ha-shots) || exit 1
chmod 700 "$rig"
mkdir -p "$rig/config/svolder" "$rig/state" "$rig/data" "$rig/cache" "$rig/runtime" "$rig/mq"
# The fake host keeps its synthetic speakers sets in a private runtime directory.
chmod 700 "$rig/runtime"
# Under target/, not /tmp: the browser is a snap, which sees the home directory only.
mkdir -p "$out"
# The pointer to this run's directory, written first so that a killed run can be found; the
# previous run's is read before it is replaced.
last=$out/last-rig
old=
[ ! -r "$last" ] || old=$(head -n 1 "$last")
printf '%s\n' "$rig" >"$last"
sock=$rig/runtime/svolder.sock
before=$rig/topics.before

# mosquitto_sub/pub read their login from $XDG_CONFIG_HOME/mosquitto_{sub,pub} (mode 600, in the
# rig's temp dir, removed with it): the password is never on a command line.
(
    umask 077
    printf -- '-h %s\n-p 1883\n-u %s\n-P %s\n' "$mqtt_host" "$mqtt_user" \
        "$(tr -d '\r\n' <"$HA_MQTT_PASSWORD_FILE")" >"$rig/mq/mosquitto_sub"
    cp "$rig/mq/mosquitto_sub" "$rig/mq/mosquitto_pub"
)

# Every retained topic on the broker, sorted. A snapshot ends when -W's time is up (code 27); any
# other code is a failed connection. A broker may hold no retained topic, so an empty one is fine.
snap() {
    local code=0
    XDG_CONFIG_HOME="$rig/mq" mosquitto_sub -t '#' --retained-only -W 8 -F '%t' \
        >"$1.raw" 2>"$1.err" || code=$?
    sort -u "$1.raw" | grep -v '^Timed out$' >"$1" || true
    if [ "$code" != 27 ] && [ "$code" != 0 ]; then
        echo "ha-shots: mosquitto_sub failed ($code): $(head -1 "$1.err")"
        return 1
    fi
}

# The retained configs of the computer's discovery node with their payloads: the node is the
# rig's when a config names the base topic (the rig's availability and state topics).
computer_nodes() { # OWNED-FILE FOREIGN-FILE
    local code=0
    XDG_CONFIG_HOME="$rig/mq" mosquitto_sub -t 'homeassistant/+/svolder_computer_desk/+/config' \
        --retained-only -W 6 -F '%t	%p' >"$rig/nodes.raw" 2>"$rig/nodes.err" || code=$?
    if [ "$code" != 27 ] && [ "$code" != 0 ]; then
        echo "ha-shots: mosquitto_sub failed ($code)"
        return 1
    fi
    grep -v '^Timed out$' "$rig/nodes.raw" | grep -F "\"$base/" | cut -f1 | sort -u >"$1" || true
    grep -v '^Timed out$' "$rig/nodes.raw" | grep -v -F "\"$base/" | cut -f1 | sort -u >"$2" || true
}

# Clear (an empty retained message) exactly the topics of the rig that are new since `before`, and
# check the broker holds again what it held before. Only the rig's own names are ever cleared.
# clear_rig_topics AFTER-FILE CREATED-FILE
clear_rig_topics() {
    local after=$1 created=$2 ok=0 topic
    if ! snap "$after" || ! computer_nodes "$rig/nodes.owned" "$rig/nodes.foreign"; then
        echo "ha-shots: FAIL: could not read the broker; topics of the rig may be left on it"
        echo "    ($base/*, homeassistant/*/svolder_$unit/*,"
        echo "    homeassistant/*/svolder_computer_desk/*);"
        echo "    run make ha-shots again, it clears a crashed run's leftovers first"
        return 1
    fi
    comm -13 "$before" "$after" >"$rig/topics.new"
    { cfg_filter <"$rig/topics.new"
      grep -x -F -f "$rig/nodes.owned" "$rig/topics.new"; } | sort -u >"$created"
    grep -v -x -F -f "$created" "$rig/topics.new" >"$rig/topics.foreign" || true
    echo "ha-shots: retained topics the rig created: $(wc -l <"$created")"
    while IFS= read -r topic; do
        [ -n "$topic" ] || continue
        case $topic in
            "$base"/* | homeassistant/*/svolder_"$unit"/* \
                | homeassistant/*/svolder_"$unit_lower"/* \
                | homeassistant/*/svolder_computer_desk/*) ;;
            *)
                echo "ha-shots: refusing to clear $topic (outside the rig's names)"
                ok=1
                continue
                ;;
        esac
        if grep -q -x -F -- "$topic" "$before"; then
            echo "ha-shots: refusing to clear $topic (it existed before the rig)"
            ok=1
            continue
        fi
        XDG_CONFIG_HOME="$rig/mq" mosquitto_pub -r -n -t "$topic" || ok=1
    done <"$created"
    snap "$rig/topics.final" || ok=1
    if [ -n "$(comm -12 "$created" "$rig/topics.final")" ]; then
        echo "ha-shots: FAIL: these retained topics are still on the broker:"
        comm -12 "$created" "$rig/topics.final" | sed 's/^/    /'
        ok=1
    fi
    if [ -n "$(comm -23 "$before" "$rig/topics.final")" ]; then
        echo "ha-shots: FAIL: topics that existed before the rig are gone:"
        comm -23 "$before" "$rig/topics.final" | sed 's/^/    /'
        ok=1
    fi
    if [ -s "$rig/topics.foreign" ]; then
        # A new retained topic outside the rig's names is left alone; one that names the rig is a
        # failure (a daemon change published a topic the patterns do not know).
        echo "ha-shots: WARN: new retained topics outside the rig's names, left alone:"
        sed 's/^/    /' "$rig/topics.foreign"
        rigish="$base|$unit|svolder_computer_desk|computer_desk"
        if grep -q -i -E "$rigish" "$rig/topics.foreign"; then
            echo "ha-shots: FAIL: one of them may be the rig's; check it, clear it by hand if it is"
            ok=1
        fi
    fi
    [ "$ok" -ne 0 ] || echo "ha-shots: ok: every topic the rig created is cleared;" \
        "the broker holds what it held before"
    return "$ok"
}

cfgenv=(XDG_CONFIG_HOME="$rig/config" XDG_STATE_HOME="$rig/state" XDG_DATA_HOME="$rig/data"
    XDG_CACHE_HOME="$rig/cache" XDG_RUNTIME_DIR="$rig/runtime" HOME="$rig"
    SVOLDER_SYSTEMCTL="$rig/no-systemctl" SVOLDER_ADAPTER=hci-none SVOLDER_TRANSPORT=fake
    SVOLDER_FAKE_PROFILE=bifrost3 SVOLDER_FAKE_UNIT=$unit SVOLDER_HOST=fake
    SVOLDER_FAKE_READINGS=usb_connected=1,loudness_strength=60
    SVOLDER_AUTOEQ_URL=http://127.0.0.1:9/)
cfg() { env "${cfgenv[@]}" "$@"; }
sv() { cfg "$svolder" --socket "$sock" --transport daemon "$@"; }

# A run killed outright (SIGKILL, power) leaves its daemon and its temp directory: the pointer
# file names the directory; the daemon is the process whose command line is this checkout's
# svolderd with that directory's socket. Nothing else is ever stopped.
if [ -n "$old" ]; then
    case $old in
        /tmp/svolder/ha-shots.*|"${SVOLDER_TMP_BASE:-/tmp/svolder}"/ha-shots.*)
            for f in /proc/[0-9]*/cmdline; do
                opid=${f#/proc/}
                opid=${opid%/cmdline}
                [ "$opid" != "$$" ] || continue
                mine="$svolderd --socket $old/runtime/svolder.sock "
                if tr '\0' ' ' <"$f" 2>/dev/null | grep -q -F -- "$mine"; then
                    echo "ha-shots: stopping the daemon (pid $opid) of a killed run"
                    kill "$opid" 2>/dev/null
                fi
            done
            rm -rf "$old"
            ;;
    esac
    python3 -c 'import time; time.sleep(4)'
fi
python3 -I "$here/ha-shots.py" absent || exit 1
snap "$rig/topics.start" || exit 1
computer_nodes "$rig/nodes.owned" "$rig/nodes.foreign" || exit 1
if [ -s "$rig/nodes.foreign" ]; then
    echo "ha-shots: the broker holds a computer named desk that is not the rig's;"
    echo "    refusing to share its ids:"
    sed 's/^/    /' "$rig/nodes.foreign"
    exit 1
fi
# Leftovers of a crashed run of this tool (matched by its base topic, its fake unit id and its
# own computer node, never by a name alone): cleared first, then Home Assistant must have dropped
# the device and entities they made.
{ cfg_filter <"$rig/topics.start"; cat "$rig/nodes.owned"; } | sort -u >"$rig/topics.left"
if [ -s "$rig/topics.left" ]; then
    echo "ha-shots: clearing $(wc -l <"$rig/topics.left") retained topics a crashed run left"
    while IFS= read -r topic; do
        XDG_CONFIG_HOME="$rig/mq" mosquitto_pub -r -n -t "$topic" || exit 1
    done <"$rig/topics.left"
    gone=1
    for try in $(seq 1 30); do
        if python3 -I "$here/ha-shots.py" after >"$rig/after.txt" 2>&1; then gone=0; break; fi
        python3 -c 'import time; time.sleep(2)'
    done
    if [ "$gone" -ne 0 ]; then
        cat "$rig/after.txt"
        echo "ha-shots: Home Assistant still lists the crashed run's device"
        exit 1
    fi
fi
snap "$before" || exit 1
snapped=1
echo "ha-shots: retained topics on the broker before the rig: $(wc -l <"$before")"

cat >"$rig/config/svolder/config.toml" <<EOF
schema = 1

[[device]]
name = "$device"
udid = "$unit"

[mqtt]
enabled = true
host = "$mqtt_host"
port = 1883
username = "$mqtt_user"
commands = true
client_id = "$client"
base_topic = "$base"
EOF

# The daemon's host name is Desk inside a UTS namespace of its own (bwrap, no root needed); the
# password goes into its environment alone, from a subshell, never onto a command line.
(
    SVOLDER_MQTT_PASSWORD=$(tr -d '\r\n' <"$HA_MQTT_PASSWORD_FILE")
    export SVOLDER_MQTT_PASSWORD
    exec env "${cfgenv[@]}" bwrap --bind / / --dev-bind /dev /dev --unshare-uts \
        --hostname "$computer" \
        --die-with-parent "$svolderd" --socket "$sock" --config "$rig/config/svolder/config.toml"
) >"$rig/daemon.log" 2>&1 &
pid=$!
echo "ha-shots: fake svolderd started (pid $pid)"

shoot() {
    python3 -I "$here/ha-shots.py" appear || return 1
    # Pleasant state (the USB plugged in and the DAC's loudness strength at 60 come from
    # SVOLDER_FAKE_READINGS): the DAC on USB at -24 dB with the Warm tone preset, the computer's EQ
    # on with a preset, the crossfeed on, a speakers set chosen and the computer's loudness strength
    # at 60, and two scenes, "Late night" the one that is on.
    sv eq load Warm >/dev/null && sv eq mode eq >/dev/null && sv set input USB >/dev/null &&
        sv host eq preset Bass >/dev/null && sv host eq on >/dev/null &&
        sv host crossfeed on >/dev/null &&
        sv host speakers use fake-with-room >/dev/null && sv host loudness strength 60 >/dev/null &&
        sv scene save "Late night" >/dev/null &&
        sv host crossfeed off >/dev/null && sv host eq preset Presence >/dev/null &&
        sv scene save "Focus" >/dev/null && sv scene load "Late night" >/dev/null &&
        sv set volume -24dB >/dev/null || return 1
    python3 -I "$here/ha-shots.py" wait volume -24 || return 1
    python3 -I "$here/ha-shots.py" wait crossfeed on || return 1
    python3 -I "$here/ha-shots.py" shoot "$out" || return 1
}
if ! shoot; then
    echo "ha-shots: daemon log tail:"
    tail -n 15 "$rig/daemon.log" | sed 's/^/    /'
    exit 1
fi

# The pictures: lossless WebP as make web-shots encodes its stills, near-lossless when over budget.
mkdir -p "$images"
for pair in desktop:ha-dashboard phone:ha-phone; do
    src=$out/${pair%%:*}.png
    dst=$images/${pair#*:}.webp
    ffmpeg -v error -y -fflags +bitexact -i "$src" -map_metadata -1 -flags:v +bitexact \
        -c:v libwebp -lossless 1 -compression_level 6 "$dst" || exit 1
    size=$(stat -c %s "$dst")
    if [ "$size" -gt 300000 ]; then
        ffmpeg -v error -y -fflags +bitexact -i "$src" -map_metadata -1 -flags:v +bitexact \
            -c:v libwebp -quality 92 -compression_level 6 "$dst" || exit 1
        size=$(stat -c %s "$dst")
    fi
    if [ "$size" -gt 300000 ]; then
        echo "ha-shots: ${pair#*:}.webp is $size bytes, over its 300000 budget" >&2
        exit 1
    fi
    printf 'ha-shots: %s is %d bytes (%s), the budget is 300000\n' "${pair#*:}.webp" "$size" \
        "$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height \
            -of csv=p=0:s=x "$dst" 2>/dev/null || echo ?)"
done
echo "ha-shots: done: $images/ha-dashboard.webp and $images/ha-phone.webp"
