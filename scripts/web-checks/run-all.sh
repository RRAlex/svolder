#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# make web-checks: every node harness and every browser check of the web UI's effects (the menu,
# the levels, the layer, the smoke, what stirs it, its notes), the node ones once and the
# browser ones in headless Chromium and headless Firefox, WEB_CHECKS_JOBS at a time (default from
# the cores and the CPU pressure, see auto_jobs; 1 runs them one after the other, the serial lane's
# timing checks after the others; a script with timing cases runs its logic part in the pool and its
# timing part in the serial lane, see split_scripts) against throwaway
# fake daemons (never the
# installed binaries, never ports 8737/8738; each check has its own daemon home, free ports,
# browser profile and output files). A check that fails beside others is run again alone: passing
# is `flaky` (counted as passed, listed in the total), failing again is FAIL; a timing check that fails in
# the serial lane is run once more alone at the end of the lane, the same way. One line per check
# as it finishes, a total, exit 1 on any FAIL. Each check's whole output is kept in target/web-checks/out/. Nothing
# here takes a picture of the smoke: it is read with readPixels and float readbacks, numbers only.
# The mutation runners (mutate_*.py, node/mutate_*.sh) are slow, by hand only, never a gate: `make tests-check` (FULL=1 for the browser ones).
# Every check's wall, CPU and peak RSS go to target/web-checks/timing.tsv; the README has the
# gate ladder (task, lane end, merge, milestone, release) and the lock rules.
# Every gate reuses a pool browser check's last pass when nothing it reads changed (the result
# cache, fingerprint.py: the check's script and helpers, the web files its load map entry names,
# the svolderd binary, the browsers, this file); a reused check is reported as `cached`. The
# exclusive lane (the serial checks) and the node harnesses always run cold. The full gate runs
# cold too under --cold (make web-checks-cold; required before a version is TAGGED), under
# WEB_CHECKS_NO_CACHE=1, and by itself when a smoke file, the check tooling or the load map
# changed since the last passing full gate.
#
# Which gate when (the coverage map, COVERAGE.md, says what each check proves and where):
#   run-all.sh --cold   the full gate with nothing reused (make web-checks-cold): before a version is tagged;
#   run-all.sh          the full gate (make web-checks, 47 minutes in 0.32.0 at 6 jobs): required before every merge, and whenever a smoke
#                       file (smoke.js, smoke-model.js, smoke-director.js, smoke-engine.js,
#                       smoke-notes.js, effects.js, under bins/svolderd/web/themes/ember/) or one
#                       of the smoke's own checks changed;
#   run-all.sh --task   the quick gate's set with the logic-only checks (logic_only below) in
#                       Chromium only (make web-checks-task): a task or a lane end; refuses like --quick;
#   run-all.sh --quick  the quick gate (make web-checks-quick, about 38 min at WEB_CHECKS_JOBS=6, much longer with WEB_CHECKS_JOBS=1): the node
#                       checks, the browser checks that do not exercise the smoke (the effects
#                       menu and layout, the Tone message, the ring click, the curve's mouse
#                       drag, Now playing, the visualizer, Arrange mode, the theme contract and
#                       the core's own rows, devices, completeness on every device kind, the
#                       Firefox preload) and one short smoke check per browser. For a change to the
#                       page, the styles or the templates that leaves the smoke alone. It
#                       refuses (exit 1) when a smoke file or this directory changed against
#                       WEB_CHECKS_BASE (default main) in the commits, the working tree or the
#                       untracked files; outside a git checkout it runs and says it could not
#                       tell.
set -uo pipefail

here=$(dirname "$(readlink -f "$0")")
root=$(dirname "$(dirname "$here")")
out=$root/target/web-checks/out
timing=$root/target/web-checks/timing.tsv
cargo=${CARGO:-cargo}
. "$here/lockwait.sh"

quick=0
task=0
cold=0
refusal_only=0
case "${1:-}" in
    "") ;;
    --quick) quick=1 ;;
    --task) quick=1; task=1 ;;
    --cold) cold=1 ;;
    --refusal-only) quick=1; refusal_only=1 ;;
    *) echo "usage: run-all.sh [--quick | --task | --cold]" >&2; exit 2 ;;
esac

# The files whose change needs the full gate: the smoke's, effects.js (it starts and stops the
# smoke; the quick gate does not cycle effects) and the smoke's own checks and harness. The checks
# of the menu, the layout, the Tone message, the ring click, devices and the preload are not
# listed: they belong to the quick gate.
w=bins/svolderd/web/themes/ember
c=scripts/web-checks
n=$c/node
smoke_files=(
    $w/smoke.js $w/smoke-model.js $w/smoke-director.js $w/smoke-engine.js $w/smoke-notes.js
    $w/effects.js
    $c/smoke_core.py $c/smoke_page.py $c/smoke_quick.py $c/smoke_probe.py $c/notes.py
    $c/notes_tune.py $c/haze.py $c/scroll.py $c/sources.py $c/sweep.py $c/touch.py
    $c/effects_cycle.py $c/tune.py $c/common.py $c/parts.py $c/fingerprint.py $c/gate_cache.sh $c/lockwait.sh $c/gate_wait.sh $c/run-all.sh $c/COVERAGE.md $c/soak.py
    $c/mutate_smoke.py $c/mutate_notes.py $c/mutate_haze.py $c/mutate_scroll.py
    $c/mutate_sources.py $c/mutate_effects.py $c/mutate_registry.py $c/mutpool.py
    $n/director.mjs $n/effects.mjs $n/look-values.mjs $n/model.mjs $n/notes.mjs $n/sources.mjs
    $n/mutate_director.sh $n/mutate_model.sh $n/mutate_notes.sh $n/mutate_sources.sh
)

# refuse_if_smoke_changed: the quick gate says the full gate is needed and exits 1 when a smoke
# file changed since WEB_CHECKS_BASE; without a git checkout (or the base) it says it cannot tell.
refuse_if_smoke_changed() {
    local base=${WEB_CHECKS_BASE:-main} changed line f
    if ! git -C "$root" rev-parse --git-dir >/dev/null 2>&1 ||
        ! git -C "$root" rev-parse --verify -q "$base" >/dev/null 2>&1; then
        echo "web-checks: could not tell whether a smoke file changed (no git checkout or no" \
            "'$base'); running the quick gate anyway" >&2
        return 0
    fi
    if ! changed=$({
        git -C "$root" diff --name-only "$base"...HEAD || exit 1
        git -C "$root" diff --name-only HEAD || exit 1
        git -C "$root" ls-files --others --exclude-standard || exit 1
    } | sort -u; [ "${PIPESTATUS[0]}" -eq 0 ]); then
        echo "web-checks: could not tell whether a smoke file changed (git failed against" \
            "'$base'); running the quick gate anyway" >&2
        return 0
    fi
    while IFS= read -r line; do
        for f in "${smoke_files[@]}"; do
            case $line in
                "$f" | "$f"/*)
                    case $line in
                        $c/common.py | $c/parts.py | $c/run-all.sh | $c/COVERAGE.md | $c/fingerprint.py | $c/gate_cache.sh | $c/lockwait.sh | $c/gate_wait.sh)
                            what="the gate's own harness changed ($line); every check reads it"
                            ;;
                        *) what="a smoke file or one of the smoke's checks changed ($line)" ;;
                    esac
                    echo "web-checks: $what, so the quick and task gates cannot vouch for it." \
                        "Run the merge rung: make web-checks (the full gate, both engines)." \
                        "A lane end with only this change may run the full gate once instead of" \
                        "the task gate. See 'The gate ladder' in scripts/web-checks/README.md." >&2
                    exit 1
                    ;;
            esac
        done
    done <<<"$changed"
}
if [ "$quick" -eq 1 ]; then
    refuse_if_smoke_changed
fi
# --refusal-only (used by gate_refusal.sh): only the quick gate's refusal, then stop.
if [ "$refusal_only" -eq 1 ]; then
    exit 0
fi

missing=()
command -v node >/dev/null || missing+=("node")
# Without jsdom the harnesses that need a page skip (node/dom.mjs), and a skip fails the gate:
# WEB_CHECK_ALLOW_NO_JSDOM=1 lets it start and lets them pass, each saying SKIPPED in its row.
if ! node -e 'require("jsdom")' 2>/dev/null; then
    if [ "${WEB_CHECK_ALLOW_NO_JSDOM:-}" = 1 ]; then
        echo "web-checks: jsdom is missing and WEB_CHECK_ALLOW_NO_JSDOM=1: the node harnesses that" \
            "need a page are SKIPPED, not run" >&2
    else
        missing+=("jsdom for node (apt package node-jsdom; WEB_CHECK_ALLOW_NO_JSDOM=1 skips its harnesses)")
    fi
fi
[ -x /usr/bin/time ] || missing+=("/usr/bin/time (install it: sudo apt install time)")
command -v python3 >/dev/null || missing+=("python3")
python3 -I -c 'import websockets' 2>/dev/null || missing+=("python3-websockets (apt package)")
python3 -I -c 'import numpy, PIL' 2>/dev/null ||
    missing+=("python3-numpy and python3-pil (apt packages)")
command -v chromium >/dev/null || [ -x /snap/bin/chromium ] || missing+=("chromium")
command -v firefox >/dev/null || [ -x /snap/bin/firefox ] || missing+=("firefox")
command -v geckodriver >/dev/null || [ -x /snap/bin/geckodriver ] || missing+=("geckodriver")
if [ ${#missing[@]} -gt 0 ]; then
    printf 'web-checks: missing %s\n' "${missing[@]}" >&2
    exit 1
fi

# read before the build: the gate's own compile would otherwise raise the pressure it reads
# The default job count: a quarter of the cores (6 on 24), less when the CPU is already contended.
# The contention is read from the kernel's pressure stall information (/proc/pressure/cpu, the
# `some` line's avg10: the share of the last 10 s in which some task waited for a core), not
# from the load average, which counts tasks blocked on I/O as well and so reads high while the
# CPUs are idle. avg10 under 10: the quarter; under 30: two thirds of it; under 60: half; more:
# 2. Never fewer than 2 nor more than 12. Without the pressure file the quarter. WEB_CHECKS_JOBS
# overrides it.
auto_jobs() {
    local cores base p=0 n
    cores=$(nproc 2>/dev/null || echo 4)
    base=$((cores / 4))
    [ "$base" -lt 2 ] && base=2
    [ "$base" -gt 12 ] && base=12
    if [ -r /proc/pressure/cpu ]; then
        p=$(awk '$1 == "some" { sub("avg10=", "", $2); print int($2) }' /proc/pressure/cpu 2>/dev/null)
        case $p in '' | *[!0-9]*) p=0 ;; esac
    fi
    if [ "$p" -lt 10 ]; then n=$base
    elif [ "$p" -lt 30 ]; then n=$((base * 2 / 3))
    elif [ "$p" -lt 60 ]; then n=$((base / 2))
    else n=2
    fi
    [ "$n" -lt 2 ] && n=2
    echo "web-checks: $n jobs ($cores cores, CPU pressure avg10 ${p}%; WEB_CHECKS_JOBS overrides)" >&2
    echo "$n"
}
if [ -n "${WEB_CHECKS_JOBS:-}" ]; then jobs_n=$WEB_CHECKS_JOBS; else jobs_n=$(auto_jobs); fi
case $jobs_n in
    '' | *[!0-9]* | 0) echo "web-checks: WEB_CHECKS_JOBS must be a number of 1 or more" >&2; exit 2 ;;
esac

# The result cache (fingerprint.py): read on every gate's pool browser checks, written after every
# pass. The full gate reads it only when it may vouch for the reuse: --cold and WEB_CHECKS_NO_CACHE=1
# run every check, and so does a full gate whose state (the smoke files, the check tooling, the load
# map) differs from the last passing full gate's. The shared part of the fingerprint (the Rust
# sources from cargo's dep-info, the tooling, the browsers) is taken after the build below.
fpy=("python3" -I "$here/fingerprint.py")
cache_read=1
{ [ "${WEB_CHECKS_NO_CACHE:-}" = 1 ] || [ "$cold" -eq 1 ]; } && cache_read=0
# state_hash PART: the hash of the smoke's files and the check tooling (tools) or of the map (map)
state_hash() {
    if [ "$1" = map ]; then
        "${fpy[@]}" state "$root/$c/load-map.json"
    else
        local f paths=()
        for f in "${smoke_files[@]}" "$c/touched.py"; do paths+=("$root/$f"); done
        "${fpy[@]}" state "${paths[@]}"
    fi
}
tools_state= map_state=
if [ "$quick" -eq 0 ] && [ -z "${WEB_CHECKS_ONLY:-}" ]; then
    tools_state=$(state_hash tools) map_state=$(state_hash map)
    if [ "$cache_read" -eq 1 ] && [ "$("${fpy[@]}" marker get)" != "$tools_state $map_state" ]; then
        cache_read=0
        echo "web-checks: running cold: a smoke file, the check tooling or the load map changed since" \
            "the last passing full gate (or there is none)" >&2
    fi
fi
cache_ok=0
cached=()
declare -A fps=()
CFP= CCFP= CFP_SCRIPT= CFP_ENGINE=

if ! "$cargo" build -q --manifest-path "$root/Cargo.toml" -p svolderd -p svolder; then
    echo "web-checks: the svolderd/svolder build failed; no check ran against a stale binary" >&2
    exit 1
fi

fp_base=$("${fpy[@]}" base) || fp_base=
[ -z "$fp_base" ] && cache_read=0
if [ "$cache_read" -eq 1 ]; then
    echo "web-checks: result cache on (a check unchanged since its last pass is reported cached;" \
        "--cold or WEB_CHECKS_NO_CACHE=1 runs every check)" >&2
fi

# leftovers of earlier runs (profiles, daemon homes, web copies) go, then the old outputs; this
# runs before any check starts, never while one runs
mkdir -p "$out"
find "$root/target/web-checks" -mindepth 1 -maxdepth 1 ! -name out ! -name cache ! -name deps -exec rm -rf {} +
printf 'check\tengine\tlane\tverdict\twall_s\tcpu_s\trss_mb\n' >"$timing"
find "$out" -mindepth 1 -maxdepth 1 -exec rm -rf {} +
# A full gate rewrites the load records (touched.py) from nothing, so a file a check stopped loading
# drops out of the map it writes at the end.
full_gate=0
[ "$quick" -eq 0 ] && [ -z "${WEB_CHECKS_ONLY:-}" ] && full_gate=1
[ "$full_gate" -eq 1 ] && rm -rf "$root/target/web-checks/deps"

# A TERM or INT (or an early exit) stops every check that is running. Each check runs in a process
# group of its own (setsid) whose id is kept in $out/LABEL.pgid, so the kill reaches the check's
# children too, even when its wrapper was itself killed; the Python checks close their browsers and
# daemons on a TERM.
stop_children() {
    local f pg pid
    trap - EXIT TERM INT
    for pid in $(jobs -rp); do kill -TERM "$pid" 2>/dev/null; done
    for f in "$out"/*.pgid; do
        [ -e "$f" ] || continue
        read -r pg <"$f"
        kill -0 -- "-$pg" 2>/dev/null && kill -TERM -- "-$pg" 2>/dev/null
    done
    wait 2>/dev/null
    for f in "$out"/*.pgid; do
        [ -e "$f" ] || continue
        read -r pg <"$f"
        kill -0 -- "-$pg" 2>/dev/null && kill -KILL -- "-$pg" 2>/dev/null
    done
    return 0
}
trap stop_children EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# The per-check measurements: one tab-separated row per run of a check (label, engine, lane, verdict,
# wall seconds, CPU seconds = user+sys of its waited-for descendants, peak RSS in MB of its largest
# process) into $timing, from GNU time; the top 15 by wall are printed at the end.
# The gate lock, shared with the other lanes and gates on this machine: browser checks hold it
# shared (flock -s), so any number run side by side; a check that reads timing (a draw cost, a
# frame rate) and the re-run of a failed check hold it exclusive (flock -x) and so run with no other
# browser check of any gate on the machine. A check that waits over 10 s says so. The node harnesses do not take it.
if [ -z "${WEB_CHECKS_LOCK:-}" ]; then
    # The default lock lives in /tmp/svolder, made and checked by tmp-base.sh's rules (0700, a real
    # directory this user owns); mutpool.py holds the same file.
    (SVOLDER_TMP_BASE=/tmp/svolder; . "$(dirname "$(readlink -f "$0")")/../tmp-base.sh"; svtmp_base) || exit 1
fi
gate_lock=${WEB_CHECKS_LOCK:-/tmp/svolder/web-gate.lock}
lock_mode=
tsv_row() {
    local label=$1 lane=$2 verdict=$3 tfile=$4 engine=- wall u s rss
    case $label in
        *-chromium) engine=chromium ;;
        *-firefox) engine=firefox ;;
    esac
    read -r wall u s rss < <(tail -n 1 "$tfile" 2>/dev/null) || true
    if [ -z "${rss:-}" ]; then wall=0; u=0; s=0; rss=0; fi
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$label" "$engine" "$lane" "$verdict" "$wall" \
        "$(awk -v u="$u" -v s="$s" 'BEGIN { printf "%.1f", u + s }')" \
        "$(awk -v k="$rss" 'BEGIN { printf "%.0f", k / 1024 }')" >>"$timing"
}
top_table() {
    [ -s "$timing" ] || return 0
    echo "slowest 15 runs (wall s, CPU s, peak RSS MB of the largest process; all rows in $timing):"
    tail -n +2 "$timing" | sort -t$'\t' -k5,5nr | head -n 15 |
        awk -F'\t' '{ printf "  %-30s %-9s %-7s %6.1f %7.1f %6d\n", $1, $2, $3, $5, $6, $7 }'
}

# excerpt FILE: what failed in a check's output, indented.
excerpt() {
    grep -a '^FAIL\|Error\|Traceback' "$1" | head -n 5 | sed 's/^/        /'
}

# one_check LABEL FILE FAILWORD COMMAND...: one check, run in the background (the caller waits for
# it): its whole output goes to FILE, its verdict (`ok` or FAILWORD: `FAIL`, or `retry` for a
# failure that will be run again) and seconds to $out/LABEL.verdict, and its line (label, seconds,
# last line) is printed as it finishes. A check with no verdict file at the end was lost (killed,
# crashed) and counts as FAIL.
one_check() {
    local label=$1 file=$2 fail_word=$3 start=$SECONDS verdict=ok c= last msg term= tfile
    local launched waited=0 stamp
    launched=$(now_ms)
    shift 3
    tfile=$out/$label.time
    stamp=$out/$label.lock
    rm -f "$stamp"
    lockcmd=()
    if [ -n "${lock_mode:-}" ]; then
        lockcmd=(flock "$lock_mode" "$gate_lock")
        if ! flock -w 10 "$lock_mode" "$gate_lock" true 2>/dev/null; then
            echo "waiting  $label: waiting for the gate lock (held by another gate)"
        fi
    fi
    rm -f "$tfile"
    # a TERM while the check is being launched is held until its pid is known, then acted on
    trap 'term=1' TERM
    rm -f "$out/$label.verdict"
    stampcmd=()
    [ ${#lockcmd[@]} -gt 0 ] && stampcmd=(bash "$here/lockwait.sh" stamp "$stamp")
    setsid -w "${lockcmd[@]}" "${stampcmd[@]}" \
        /usr/bin/time -f '%e %U %S %M' -o "$tfile" "$@" >"$file" 2>&1 &
    c=$!
    echo "$c" >"$out/$label.pgid"
    trap 'rm -f "$out/$label.pgid"; kill -TERM -- "-$c" 2>/dev/null; wait "$c" 2>/dev/null; exit 143' TERM
    if [ -n "$term" ]; then kill -TERM "$BASHPID"; fi
    wait "$c" || verdict=$fail_word
    rm -f "$out/$label.pgid"
    last=$(tail -n 1 "$file")
    # the seconds in the verdict and the row are the check's own: the wait for the gate lock is
    # told apart (`waited N s`)
    waited=$(waited_secs "$launched" "$stamp")
    printf '%s %s\n' "$verdict" $((SECONDS - start - waited)) >"$out/$label.verdict"
    if [ -n "${CFP:-}" ]; then
        # a pass is stored at the end of the gate (store_pending), when nothing it reads has changed
        # since the check started; any other outcome drops the record
        if [ "$verdict" = ok ]; then
            printf '%s\t%s\t%s\t%s\n' "$label" "$CFP_SCRIPT" "${CFP_ENGINE:--}" "$CCFP" >>"$out/pending.tsv"
        else
            "${fpy[@]}" forget "$label"
        fi
    fi
    tsv_row "$label" "${lane:-pool}" "$verdict" "$tfile"
    msg=$(printf '%-5s %-30s %4ds  %s' "$verdict" "$label" $((SECONDS - start - waited)) "$last")
    if [ "$waited" -gt 0 ]; then
        msg+=" (waited $waited s for the gate lock)"
    fi
    if [ "$verdict" != ok ]; then
        msg+=$'\n'$(excerpt "$file")
    fi
    printf '%s\n' "$msg"
}

# A check is "label|script|argument|part": python3 script.py argument [--part part] (node ones:
# label|node/x.mjs|). The part is empty for a check that is not split (a script's `--part logic` and
# `--part timing` are its two halves, see split_scripts below).
# entry_fields ENTRY sets e_label, e_script, e_arg and e_part; check_cmd ENTRY sets the array CMD.
entry_fields() {
    IFS='|' read -r e_label e_script e_arg e_part <<<"$1"
}
check_cmd() {
    entry_fields "$1"
    CMD=()
    case $e_script in
        *.mjs) CMD=(env LOOK_RETUNED_SKIPS=1 node "$here/$e_script") ;;
        *.sh) CMD=(bash "$here/$e_script") ;;
        *)
            CMD=(python3 "$here/$e_script")
            [ -n "$e_arg" ] && CMD+=("$e_arg")
            [ -n "$e_part" ] && CMD+=(--part "$e_part")
            ;;
    esac
}

# cached_check LABEL: a check unchanged since its last pass is not run; its verdict is ok and its
# row says `cached`.
cached_check() {
    local label=$1 engine=-
    case $label in
        *-chromium) engine=chromium ;;
        *-firefox) engine=firefox ;;
    esac
    printf 'ok 0\n' >"$out/$label.verdict"
    printf '%s\t%s\t%s\t%s\t0\t0.0\t0\n' "$label" "$engine" "${lane:-pool}" cached >>"$timing"
    printf '%-5s %-30s %4ds  %s\n' cached "$label" 0 "unchanged since its last pass (--cold or WEB_CHECKS_NO_CACHE=1 runs it)"
    cached+=("$label")
}

# run_pool JOBS ENTRY...: the entries, JOBS at a time, in the order given.
run_pool() {
    local n=$1 entry label script
    shift
    for entry in "$@"; do
        entry_fields "$entry"
        label=$e_label
        CFP=
        if [ "$cache_ok" -eq 1 ]; then
            CFP=$("${fpy[@]}" check "$e_script" "$e_arg" "$fp_base" "$label") || CFP=
            fps[$label]=$CFP
            if [ "$cache_read" -eq 1 ] && [ -n "$CFP" ] && "${fpy[@]}" hit "$label" "$CFP"; then
                cached_check "$label"
                continue
            fi
        fi
        while [ "$(jobs -rp | wc -l)" -ge "$n" ]; do wait -n 2>/dev/null; done
        check_cmd "$entry"
        if [ -n "$CFP" ]; then
            # what the check starts from, taken now that the slot is free: the fingerprint without the
            # check's own web areas (the whole web dir counts), against which the end of the gate decides
            # whether anything moved while it ran. The base is the one taken after the build.
            CFP_SCRIPT=$e_script CFP_ENGINE=$e_arg
            CCFP=$("${fpy[@]}" check "$CFP_SCRIPT" "$CFP_ENGINE" "$fp_base") || CCFP=
            [ -z "$CCFP" ] && CFP=
        fi
        one_check "$label" "$out/$label.out" "$pool_fail" "${CMD[@]}" &
    done
    wait
}

# Mean seconds a check took in the 0.39.4 and 0.39.5 release gates: the longest start first so
# the long browser checks are not left for the end. A check not listed counts as 60.
declare -A weight=(
    [nightdrive_theme]=535 [signal_theme]=440 [paper]=265 [visualizer]=255 [effects_menu]=200
    [nightdrive_phone]=115 [nightdrive_states]=90 [space_key]=45 [ember_link]=15 [modal]=90 [popups]=395 [devices_popup]=570 [eq_library]=180 [loudness_popup]=160 [setup_popup]=180 [crossfeed_popup]=130 [speakers_popup]=105 [scenes_popup]=160 [menu_theme_row]=30 [tone_overlay]=35 [contrast]=495 [phone_sweep]=220
    [completeness]=230 [playing]=315 [tone_note]=120 [contract]=200 [frame_button]=180 [keys]=170
    [nightdrive_sheet]=165 [arrange]=115 [tone_drag]=60 [core_rows]=60 [computer_core]=40 [computer_ember]=280 [computer_paper]=175 [computer_signal]=285 [signal_vu]=175 [sweep]=60
    [effects_cycle]=50 [scroll]=40 [sources]=115 [smoke_page]=95 [smoke_quick]=6
    [smoke_core]=145 [haze]=200 [touch]=35 [tone_touch]=50
    [computer_nightdrive]=515 [ha_cards]=25
)
by_weight() {
    local e script
    for e in "$@"; do
        script=${e#*|}
        script=${script%%|*}
        printf '%s %s\n' "${weight[${script%.py}]:-60}" "$e"
    done | sort -s -k1,1nr | cut -d' ' -f2-
}

# A retuned look number (node/look-values.mjs) fails node-model alone: the tests written for the
# approved look skip themselves in the other harnesses while it does.
node_entries=()
for name in effects facts model sources notes director keep tone-curve plot tone-drag playing viz vu \
    core-store core-api core-stream core-actions core-device-config core-device-settings core-device-readings core-device-facts core-device-wire core-device-walk core-computer core-words core-table core-point-drag core-prefs core-registry core-climb core-loop core-title core-old-readings core-repeat core-freeze core-names core-html core-inline-ask core-list core-driver core-clock core-volume-ask core-mounted core-menu-rows core-menu core-popups core-stay core-devices core-eq-library core-loudness core-crossfeed core-speakers core-scenes core-help core-setup dom-skip core-keys theme-keys ember-parts ember-keys \
    level-write toast frame-keys core-modal core-router switch ember-order contract \
    paper-tokens paper-words paper-live paper-ruler paper-draw paper-notes \
    signal-line signal-draw signal-vu nightdrive-scene nightdrive-sky nightdrive-hud nightdrive-tune nightdrive-draw nightdrive-ridge nightdrive-floor nightdrive-budget nightdrive-debug nightdrive-glow nightdrive-mood nightdrive-states nightdrive-controls nightdrive-eqkey nightdrive-dash-clock nightdrive-keys signal-controls signal-palette; do
    node_entries+=("node-$name|node/$name.mjs|")
done
node_entries+=("gate-refusal|gate_refusal.sh|" "gate-cache|gate_cache.sh|" "gate-wait|gate_wait.sh|")
node_entries+=("ha-strategy|ha/strategy.mjs|" "ha-cards|ha/cards.mjs|")

# The checks that assert a frame rate, a draw cost or a wall-clock pace (frames a second against a
# band, a flick's speed, a velocity after wheel steps a set time apart) need the machine to
# themselves: they hold the gate lock exclusive and run one at a time after the parallel part, on both
# gates, and a re-run of any check takes the lock exclusive too.
# split_scripts: the scripts whose cases are mostly logic (state, geometry, dye sums, notes formed) with
# a few that read the clock. Each runs twice: `--part logic` in the pool like any browser check
# (label NAME-ENGINE, cached, alone retry) and `--part timing` in the serial lane (label
# NAME_timing-ENGINE, cold, exclusive). Which case is which is in each script's `--part` selection
# (parts.py: part_has, counts) and in the README's "The serial lane's timing parts".
split_scripts=(smoke_core smoke_page haze effects_cycle scroll smoke_quick sources)
# serial_scripts: whole scripts that are timing: nightdrive_timing is Night drive's draw cost, its
# phone budget and its tap under a slowed CPU; notes forms its notes from real input pace and smoke
# timing (its logic cases fail in a loaded pool), so it is not split. The pace asserts that moved to a fake clock
# (fakeclock.py: nightdrive_theme's Still and readout rates) stay in the pool.
serial_scripts=(nightdrive_timing notes)

# contrast and phone_sweep (the sweeps, minutes of wall time each) are in the full gate only
if [ "$quick" -eq 1 ]; then
    browser_checks=(effects_menu effects_layout tone_note knob_click tone_drag playing \
        device_presets reduced_motion visualizer arrange contract frame_button keys space_key modal popups devices_popup eq_library loudness_popup setup_popup crossfeed_popup speakers_popup scenes_popup menu_theme_row tone_overlay ember_link core_rows computer_core computer_ember computer_paper computer_signal signal_vu paper signal_theme nightdrive_theme nightdrive_sheet nightdrive_phone nightdrive_states nightdrive_timing smoke_quick \
        computer_nightdrive ha_cards)
else
    browser_checks=(smoke_core smoke_page effects_menu effects_layout sources notes sweep haze \
        scroll tone_note knob_click tone_drag playing device_presets reduced_motion visualizer \
        arrange contract frame_button keys space_key modal popups devices_popup eq_library loudness_popup setup_popup crossfeed_popup speakers_popup scenes_popup menu_theme_row tone_overlay contrast phone_sweep ember_link core_rows computer_core computer_ember computer_paper computer_signal signal_vu paper signal_theme nightdrive_theme nightdrive_sheet nightdrive_phone nightdrive_states nightdrive_timing effects_cycle \
        computer_nightdrive ha_cards)
fi
entries=()
serial_entries=()
# The checks whose asserts are pure logic driven through the page (the values, the keys, the link,
# the presets, the rows): another engine adds little to them, so the task gate runs them in Chromium
# only; the quick and the full gate run them in both engines.
logic_only=(space_key device_presets core_rows)
for browser in chromium firefox; do
    for name in "${browser_checks[@]}"; do
        if [ "$task" -eq 1 ] && [ "$browser" = firefox ] &&
            [[ " ${logic_only[*]} " == *" $name "* ]]; then
            continue
        fi
        entry="$name-$browser|$name.py|$browser"
        if [[ " ${split_scripts[*]} " == *" $name "* ]]; then
            entries+=("$entry|logic")
            serial_entries+=("${name}_timing-$browser|$name.py|$browser|timing")
        elif [[ " ${serial_scripts[*]} " == *" $name "* ]]; then
            serial_entries+=("$entry")
        else
            entries+=("$entry")
        fi
    done
done
entries+=("effects_devices-chromium|effects_devices.py|" "completeness-chromium|completeness.py|chromium")
if [ "$quick" -eq 0 ]; then
    entries+=("touch-chromium|touch.py|" "tone_touch-chromium|tone_touch.py|")
fi
entries+=("firefox_preload|firefox_preload.py|")

# WEB_CHECKS_ONLY="label label ...": run only those entries (a debugging aid for the gate itself).
if [ -n "${WEB_CHECKS_ONLY:-}" ]; then
    only() {
        local e kept=()
        for e in "$@"; do [[ " $WEB_CHECKS_ONLY " == *" ${e%%|*} "* ]] && kept+=("$e"); done
        printf '%s\n' "${kept[@]}"
    }
    mapfile -t node_entries < <(only "${node_entries[@]}")
    mapfile -t entries < <(only "${entries[@]}")
    mapfile -t serial_entries < <(only "${serial_entries[@]}")
    [ -z "${node_entries[*]}" ] && node_entries=()
    [ -z "${entries[*]}" ] && entries=()
    [ -z "${serial_entries[*]}" ] && serial_entries=()
fi

total=$(( ${#node_entries[@]} + ${#entries[@]} + ${#serial_entries[@]} ))

# The node harnesses and the gate's own refusal check: a second or two, one at a time.
pool_fail=FAIL
lane=node lock_mode=
run_pool 1 "${node_entries[@]}"
lane=pool lock_mode=-s
# only the pool's browser checks use the cache; the serial lane's pace and draw-cost checks run cold
cache_ok=1

# The browser checks, longest first, JOBS at a time. With more than one job a failure is not
# final: it is run again alone below. With one job the order is the listed one and a failure stands.
if [ "$jobs_n" -gt 1 ]; then
    pool_fail=retry
    mapfile -t ordered < <(by_weight "${entries[@]}")
else
    pool_fail=FAIL
    ordered=("${entries[@]}")
fi
run_pool "$jobs_n" "${ordered[@]}"

# retry_alone ENTRY...: each check of the list whose verdict is `retry` (it failed beside others) is
# run once more alone: passing is `flaky` (counted as passed; both outputs stay, the first in
# LABEL.out, the second in LABEL.again.out), failing again is FAIL.
flaky=()
retry_alone() {
    local entry label word
    for entry in "$@"; do
        label=${entry%%|*}
        word=
        [ -r "$out/$label.verdict" ] && read -r word _ <"$out/$label.verdict"
        if [ "$word" = retry ]; then
            echo "again $label (alone)"
            lane=again lock_mode=-x
            # a pass that needed the alone retry (flaky) is never stored, and a stored one is
            # dropped
            CFP=
            "${fpy[@]}" forget "$label"
            check_cmd "$entry"
            one_check "$label" "$out/$label.again.out" FAIL "${CMD[@]}" &
            wait $!
            read -r word _ <"$out/$label.verdict"
            if [ "$word" = ok ]; then
                flaky+=("$label")
                echo "flaky $label"
            fi
        fi
    done
}
retry_alone "${ordered[@]}"

# The serial lane, one at a time on a quiet machine. A timing check that fails is run once more alone
# at the end of the lane (no waiting for a quiet machine): passing is `flaky`, failing again is FAIL.
pool_fail=retry
cache_ok=0 CFP=
lane=serial lock_mode=-x
serial_start=$SECONDS
if [ ${#serial_entries[@]} -gt 0 ]; then
    run_pool 1 "${serial_entries[@]}"
    retry_alone "${serial_entries[@]}"
fi

serial_wall=$((SECONDS - serial_start))

# The verdict: a check with no verdict file was lost (killed, crashed) and is a FAIL.
failed=0
for entry in "${node_entries[@]}" "${ordered[@]}" "${serial_entries[@]}"; do
    label=${entry%%|*}
    word=
    [ -r "$out/$label.verdict" ] && read -r word _ <"$out/$label.verdict"
    if [ "$word" != ok ]; then
        failed=$((failed + 1))
        if [ -z "$word" ]; then
            echo "FAIL  $label: lost, no verdict (the check's process was killed or crashed)"
        fi
    fi
done

note=
if [ ${#flaky[@]} -gt 0 ]; then
    note=" (${#flaky[@]} flaky: ${flaky[*]})"
fi
if [ ${#cached[@]} -gt 0 ]; then
    note+=" (${#cached[@]} cached, not run: ${cached[*]})"
fi
# the full gate that passed refreshes the committed map of which checks load which web files
if [ "$full_gate" -eq 1 ] && [ "$failed" -eq 0 ]; then
    python3 -I "$here/touched.py" --refresh --replace >&2 || echo "web-checks: could not refresh load-map.json" >&2
    # the next full gate may reuse this one's passes while the smoke, the tooling and the map stay as
    # they are now (the map as refreshed); not when a smoke or tooling file moved during the run
    if [ -n "$tools_state" ] && [ "$(state_hash tools)" = "$tools_state" ]; then
        "${fpy[@]}" marker set "$tools_state $(state_hash map)"
    fi
fi
# store_pending: the passes of this gate, each under the fingerprint the next gate will compute (the
# map as refreshed above), and only when nothing the check reads has moved since the check started:
# the base now (the Rust sources, the tooling and the browsers as they are now, read once) must equal
# the one taken after the build, and the check's own fingerprint (its script files and the whole web
# dir) the one taken at its start.
store_pending() {
    local label script engine ccfp base_now
    [ -r "$out/pending.tsv" ] || return 0
    base_now=$("${fpy[@]}" base) || base_now=
    while IFS=$'\t' read -r label script engine ccfp; do
        [ "$engine" = - ] && engine=
        if [ -n "$ccfp" ] && [ "$base_now" = "$fp_base" ] &&
            [ "$("${fpy[@]}" check "$script" "$engine" "$base_now" 2>/dev/null)" = "$ccfp" ]; then
            "${fpy[@]}" store "$label" "$("${fpy[@]}" check "$script" "$engine" "$fp_base" "$label")"
        else
            "${fpy[@]}" forget "$label"
        fi
    done <"$out/pending.tsv"
}
store_pending
echo "serial lane: $serial_wall s"
top_table
echo "web-checks: $((total - failed)) of $total checks passed in $SECONDS s$note" \
    "(output in target/web-checks/out)"
[ "$failed" -eq 0 ]
