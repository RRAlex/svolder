#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# The GNOME top-bar indicator's make targets.
#   gnome-extension.sh pack        zip contrib/gnome-shell/<uuid> into target/gnome-extension/
#   gnome-extension.sh install     pack, then install the zip for this user (never enables it)
#   gnome-extension.sh uninstall   remove it for this user
#   gnome-extension.sh check       the gjs tests, the zip, and a headless GNOME Shell load of it
#   gnome-extension.sh shots DIR   the headless load again for each fixture, with screenshots in DIR
#   gnome-extension.sh shots3 DIR  the same for two fixtures on a monitor at scale 3
# The check never touches the running session: the shell it loads is a headless one started under
# its own D-Bus session, with HOME and every XDG directory in a scratch directory.

set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
uuid=svolder-indicator@rralex.github.io
src=$root/contrib/gnome-shell
# The fixtures are the panel stream's blessed output; the tests read them in place.
SVOLDER_FIXTURES_DIR=${SVOLDER_FIXTURES_DIR:-$src/tests/fixtures}
export SVOLDER_FIXTURES_DIR
out=$root/target/gnome-extension
zip=$out/$uuid.shell-extension.zip

pack() {
    command -v gnome-extensions >/dev/null 2>&1 || {
        echo "gnome-extension: gnome-extensions is not installed (the gnome-shell package ships it)" >&2
        return 1
    }
    mkdir -p "$out"
    set --
    for f in "$src/$uuid"/*.js; do
        case $(basename "$f") in
            extension.js) ;;
            *) set -- "$@" "--extra-source=$(basename "$f")" ;;
        esac
    done
    gnome-extensions pack "$src/$uuid" --force --out-dir="$out" "$@" >/dev/null
    echo "gnome-extension: packed $zip"
}

gjs_tests() {
    command -v gjs >/dev/null 2>&1 || {
        echo "gnome-extension-check: gjs is not installed; skipping the indicator's checks"
        return 2
    }
    failed=0
    for t in "$src"/tests/*.test.js; do
        gjs -m "$t" || failed=1
    done
    return "$failed"
}

# headless NAME MODE EXTRA...: one load of the zip; MODE is ok or exit3. Needs $scratch.
headless() {
    name=$1
    mode=$2
    fixture=$3
    shots=${4:-}
    run=$scratch/$name
    mkdir -p "$run/home" "$run/bin" "$run/tmp" "$run/runtime"
    chmod 700 "$run/runtime"
    cp "$src/tests/fake-svolder" "$run/bin/svolder"
    : > "$run/result"
    : > "$run/log"
    # The test shell runs without Xwayland: on GNOME 51 its main thread waits forever on a
    # synchronous X request to its own Xwayland (the volume control's pulse client), so the
    # shell never finishes starting and the automation script never runs.
    # SVOLDER_HEADLESS_SCALE=N: a 3840x2160 virtual monitor that the script sets to scale N.
    extra=
    wrap=
    if [ -n "${SVOLDER_HEADLESS_SCALE:-}" ]; then
        extra=" --virtual-monitor 3840x2160"
        wrap="--hotplug"
    fi
    printf '#!/bin/sh\nexec "$@" --no-x11%s\n' "$extra" > "$run/bin/shell-wrap"
    chmod +x "$run/bin/shell-wrap"
    wrap="$wrap --wrap $run/bin/shell-wrap"
    # $wrap holds no spaces inside a word, so it splits into the tool's arguments on purpose.
    # shellcheck disable=SC2086
    if env HOME="$run/home" TMPDIR="$run/tmp" XDG_RUNTIME_DIR="$run/runtime" \
        XDG_CONFIG_HOME="$run/home/.config" XDG_DATA_HOME="$run/home/.local/share" \
        XDG_STATE_HOME="$run/home/.local/state" XDG_CACHE_HOME="$run/home/.cache" \
        SVOLDER_HEADLESS_TEXT_SCALE="${SVOLDER_HEADLESS_TEXT_SCALE:-}" \
        PATH="$run/bin:$PATH" SVOLDER_HEADLESS_MODE="$mode" SVOLDER_HEADLESS_RESULT="$run/result" \
        SVOLDER_FAKE_FIXTURE="$fixture" SVOLDER_FAKE_LOG="$run/log" SVOLDER_FAKE_PIDFILE="$run/pid" \
        SVOLDER_FAKE_EXIT="$([ "$mode" = exit3 ] && echo 3 || { [ "$mode" = exit0 ] && echo 0; } || true)" \
        SVOLDER_FAKE_STDERR="$([ "$mode" = exit3 ] && echo 'svolder: no daemon is running. Start one with `svolder daemon start`' || true)" \
        SVOLDER_HEADLESS_SHOTS="$shots" \
        timeout 120 dbus-run-session -- gnome-shell-test-tool --headless $wrap --extension "$zip" \
        "$src/tests/headless-load.js" > "$run/shell.log" 2>&1; then
        :
    else
        echo "gnome-extension-check: the headless load ($name) failed; its checks:" >&2
        cat "$run/result" >&2
        echo "gnome-extension-check: the shell's log ($run/shell.log), last lines:" >&2
        tail -n 30 "$run/shell.log" >&2
        return 1
    fi
    if grep -q '^FAIL' "$run/result"; then
        cat "$run/result" >&2
        return 1
    fi
    echo "gnome-extension-check: headless load $name: $(grep -c '^ok' "$run/result") checks passed"
}

# The session's document portal may still have its FUSE directory mounted under a runtime dir.
cleanup() {
    # A run that failed may have left its fake stream alive: stop the PIDs this run wrote.
    for f in "$scratch"/*/pid; do
        [ -f "$f" ] && kill "$(cat "$f")" 2>/dev/null || true
    done
    for d in "$scratch"/*/runtime/doc; do
        [ -d "$d" ] && { fusermount3 -u -z "$d" 2>/dev/null || fusermount -u -z "$d" 2>/dev/null || true; }
    done
    rm -rf "$scratch"
}

scratch_dir() {
    . "$here/tmp-base.sh"
    scratch=$(svtmp_dir gnome-ext)
    trap cleanup EXIT
}

headless_possible() {
    command -v gnome-shell-test-tool >/dev/null 2>&1 && command -v dbus-run-session >/dev/null 2>&1
}

case ${1:-} in
pack)
    pack
    ;;
install)
    pack
    gnome-extensions install --force "$zip"
    echo "gnome-extension: installed $uuid for this user, not enabled."
    echo "GNOME Shell on Wayland only finds a new extension when you log in: until you log out and"
    echo "back in, \`gnome-extensions enable\` says it does not exist. After logging back in:"
    echo "  gnome-extensions enable $uuid"
    ;;
uninstall)
    gnome-extensions uninstall "$uuid"
    ;;
check)
    status=0
    gjs_tests || status=$?
    [ "$status" -ne 2 ] || exit 0
    [ "$status" -eq 0 ] || exit "$status"
    pack
    if ! headless_possible; then
        echo "gnome-extension-check: gnome-shell-test-tool or dbus-run-session is missing; skipping the headless load"
        exit 0
    fi
    scratch_dir
    # away.jsonl ends back at the DAC: its first line alone is the away state the bar draws.
    head -n 1 "$SVOLDER_FIXTURES_DIR/away.jsonl" > "$scratch/away_first.jsonl"
    for f in "$SVOLDER_FIXTURES_DIR"/*.jsonl "$scratch/away_first.jsonl"; do
        headless "$(basename "$f" .jsonl)" ok "$f"
    done
    # Large Text: the line's height and place and the menu's width hold at a text scale of 1.25.
    for f in dac_changed player_unknown untouched config_io dac_io_changed no_mpris; do
        SVOLDER_HEADLESS_TEXT_SCALE=1.25 headless "large-$f" ok "$SVOLDER_FIXTURES_DIR/$f.jsonl"
    done
    headless exit3 exit3 "$SVOLDER_FIXTURES_DIR/untouched.jsonl"
    headless exit0 exit0 "$SVOLDER_FIXTURES_DIR/no_device.jsonl"
    ;;
only)
    # One fixture in one headless load: `only FIXTURE [TEXT_SCALE [SHOT_PREFIX]]`.
    name=${2:?usage: gnome-extension.sh only FIXTURE [TEXT_SCALE]}
    pack
    headless_possible || exit 0
    scratch_dir
    SVOLDER_HEADLESS_TEXT_SCALE=${3:-} headless "only-$name" ok "$SVOLDER_FIXTURES_DIR/$name.jsonl" "${4:-}"
    ;;
shots)
    dir=${2:?usage: gnome-extension.sh shots DIR}
    pack
    scratch_dir
    mkdir -p "$dir"
    head -n 1 "$SVOLDER_FIXTURES_DIR/away.jsonl" > "$scratch/away_first.jsonl"
    for f in untouched dac_changed muted weak_signal away_first old_signal no_device several_scenes resampled not_this_dac other_input player_unknown config_fail config_io dac_io_changed no_mpris; do
        fixture=$SVOLDER_FIXTURES_DIR/$f.jsonl
        [ "$f" != away_first ] || fixture=$scratch/away_first.jsonl
        [ -f "$fixture" ] || {
            echo "gnome-extension: no fixture $f.jsonl yet; skipped" >&2
            continue
        }
        headless "shot-$f" ok "$fixture" "$dir/$(echo "$f" | sed s/_first//)"
    done
    ;;
shots3)
    dir=${2:?usage: gnome-extension.sh shots3 DIR}
    pack
    scratch_dir
    mkdir -p "$dir"
    for f in untouched dac_changed; do
        SVOLDER_HEADLESS_SCALE=3 headless "shot3-$f" ok "$SVOLDER_FIXTURES_DIR/$f.jsonl" "$dir/$f-3x"
    done
    ;;
*)
    echo "usage: gnome-extension.sh pack|install|uninstall|check|only FIXTURE [SCALE [PREFIX]]|shots DIR|shots3 DIR" >&2
    exit 1
    ;;
esac
