#!/bin/bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# `make gate`: runs the slow checks of scripts/gate/checks.conf, skipping each one whose inputs
# cannot have changed since it last passed, and ends with a table of result and seconds.
#
#   gate.sh run [NAME...]   run the named checks (default: GATE_CHECKS, else every one in the manifest)
#   gate.sh key NAME        print the key of NAME, or why it has none (exit 1)
#   gate.sh list            the default checks
#
# The key is a hash of: the check's command and parameters, the rustc version, and the git tree hash
# (HEAD:path) of each path it lists. A pass writes target/gate-stamps/NAME-<key>; a later gate with
# the same key prints "reused (passed at COMMIT, DATE)" and skips the check. When in doubt it
# runs: a wrong reuse is the one failure this tool must not have. Never reused or stamped:
#   - a dirty tree under the check's paths (modified, staged, deleted or untracked files there);
#     git status cannot see skip-worktree files or ignored files, so a sparse checkout or an
#     ignored file inside a listed path is not noticed;
#   - any git failure (no repository, no HEAD, dubious ownership): there is no key;
#   - a tree that changed during the run (the key and the dirty state are taken again after a
#     pass and must match the ones taken before, and no file under the paths may have a newer
#     mtime than the start of the run, so an edit that was reverted is caught too);
#   - a make command line that can change what runs (a variable other than SEED, RUNS and the GATE_*
#     ones, -i or -k), T set, CARGO other than cargo, or an environment variable that changes the
#     build or the tests (SVOLDER_*, RUSTFLAGS, CARGO_BUILD_* except JOBS, CARGO_PROFILE_*, ...);
#   - a check whose STAMP field is `no`;
#   - a failure (no stamp is written).
# GATE_FRESH=1 (exactly 1) runs every check; a pass is still stamped.
#
# Env: GATE_ROOT (the repository, default this file's), GATE_MANIFEST, GATE_STAMPS (default
# ROOT/target/gate-stamps), GATE_REPORT (append the table to this file), GATE_FRESH, GATE_CHECKS,
# GATE_RUSTC_VERSION (replaces `rustc -V`; honoured only with GATE_MANIFEST, for the self-test), MAKE (default make).
set -uo pipefail
# an inherited repository (a git hook) must not decide which tree is keyed
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR GIT_OBJECT_DIRECTORY GIT_NAMESPACE

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/../tmp-base.sh"
work=$(svtmp_dir gate) || exit 1
trap 'rm -rf "$work"' EXIT
root=${GATE_ROOT:-$(cd "$here/../.." && pwd)}
manifest=${GATE_MANIFEST:-$here/checks.conf}
stamps=${GATE_STAMPS:-$root/target/gate-stamps}
export MAKE=${MAKE:-make}
export ROOT=$root
# a checkout path with a space fails every check (fails closed)
GMAKE="$MAKE -C $root"   # not exported: the Makefile passes MAKE on to scripts, which run it alone

trim() { local s=$1; s=${s#"${s%%[![:space:]]*}"}; printf '%s' "${s%"${s##*[![:space:]]}"}"; }

# load NAME: sets stamp, params, cmd, paths from the manifest; returns 1 when there is no such check
load() {
    local line n s p c d
    while IFS= read -r line || [ -n "$line" ]; do
        case $line in '#'* | '') continue ;; esac
        IFS='|' read -r n s p c d <<<"$line"
        n=$(trim "$n")
        [ "$n" = "$1" ] || continue
        stamp=$(trim "$s"); params=$(trim "$p"); cmd=$(trim "$c"); paths=$(trim "$d")
        # the command's own `|` would split it: a command here never has one; the paths are the last field
        params=$(eval "printf '%s' \"$params\"") || params=$p
        return 0
    done <"$manifest"
    return 1
}

names() {
    local line
    while IFS= read -r line || [ -n "$line" ]; do
        case $line in '#'* | '') continue ;; esac
        trim "${line%%|*}"; echo
    done <"$manifest"
}

rustc_version() {
    if [ -n "${GATE_RUSTC_VERSION:-}" ] && [ -n "${GATE_MANIFEST:-}" ]; then printf '%s' "$GATE_RUSTC_VERSION"; return; fi
    printf '%s %s %s' "$(rustc -vV 2>/dev/null || echo no-rustc)" "$("${CARGO:-cargo}" -V 2>/dev/null || echo no-cargo)" "${RUSTUP_TOOLCHAIN:-}"
}

# blocked: why this whole gate run must not reuse or stamp anything ("" when nothing); set once
blocked=
compute_blocked() {
    local w name val acc='' past=0 words=() v
    if ! git -C "$root" rev-parse --verify -q HEAD >/dev/null 2>&1; then
        blocked="git cannot read HEAD of $root"; return
    fi
    if [ -n "${T:-}" ]; then blocked="T is set"; return; fi
    case ${CARGO:-cargo} in cargo) ;; *) blocked="CARGO is set to ${CARGO}"; return ;; esac
    for v in $(compgen -e); do
        case $v in
            CARGO_BUILD_JOBS) ;;
            SVOLDER_* | RUSTFLAGS | RUSTDOCFLAGS | CARGO_ENCODED_* | CARGO_PROFILE_* | CARGO_BUILD_* | RUSTC | RUSTC_WRAPPER | RUSTC_WORKSPACE_WRAPPER | CARGO_TARGET_*_LINKER | CARGO_TARGET_*_RUNNER | RUST_TEST_* | RUST_MIN_STACK)
                blocked="$v is set"; return ;;
        esac
    done
    # MAKEFLAGS only (it carries every flag and variable; MFLAGS repeats the flags, dashed):
    # short and long flags before ` -- `, command line variables after it
    for w in ${MAKEFLAGS:-}; do
        if [ $past = 1 ]; then
            if [ -n "$acc" ]; then w="$acc $w"; acc=''; fi
            case $w in *'\') acc=${w%\\}; continue ;; esac
            name=${w%%=*}
            case $w in *=*) ;; *) blocked="make variable word '$w'"; return ;; esac
            case $name in SEED | RUNS | GATE_CHECKS | GATE_REPORT | GATE_FRESH) ;; *) blocked="make variable $name on the command line"; return ;; esac
        else
            case $w in
                --) past=1 ;;
                --ignore-errors | --keep-going) blocked="make $w"; return ;;
                --*) ;;
                -[a-zA-Z]* | [a-zA-Z]*) case ${w#-} in *[0-9]*) val=${w#-}; val=${val%%[0-9]*} ;; *) val=${w#-} ;; esac
                    case $val in *[ik]*) blocked="make flag $w"; return ;; esac ;;
            esac
        fi
    done
}

# paths of the loaded check as an array
parr=()
load_paths() { set -f; read -r -a parr <<<"$paths"; set +f; }

# dirty: true when git cannot tell, or when paths have changes the tree hashes would not see
dirty() {
    local out
    out=$(git -C "$root" status --porcelain --untracked-files=all -- "${parr[@]}" 2>/dev/null) || return 0
    [ -n "$out" ]
}

# keyof: prints the key of the loaded check; fails when git does
keyof() {
    local p t k
    local st cfg d
    {
        printf 'cmd %s\nparams %s\nrustc %s\n' "$cmd" "$params" "$(rustc_version)"
        # cargo settings outside the tree: the user's, CARGO_HOME's and any parent directory's
        d=$root
        while [ "$d" != / ] && [ -n "$d" ]; do
            d=$(dirname "$d")
            for cfg in "$d/.cargo/config" "$d/.cargo/config.toml"; do
                [ -f "$cfg" ] && printf 'cfg %s %s\n' "$cfg" "$(sha256sum <"$cfg" | cut -c1-24)"
            done
        done
        for cfg in "$HOME/.cargo/config" "$HOME/.cargo/config.toml" "${CARGO_HOME:-$HOME/.cargo}/config" "${CARGO_HOME:-$HOME/.cargo}/config.toml" "$root/.cargo/config" "$root/.cargo/config.toml"; do
            [ -f "$cfg" ] && printf 'cfg %s %s\n' "$cfg" "$(sha256sum <"$cfg" | cut -c1-24)"
        done
        for p in "${parr[@]}"; do
            if [ "$p" = . ]; then t=$(git -C "$root" rev-parse --verify -q 'HEAD^{tree}' 2>/dev/null) || return 1
            elif t=$(git -C "$root" rev-parse --verify -q "HEAD:$p" 2>/dev/null); then :
            else
                # absent only when HEAD itself is readable and has no such path
                [ -z "$(git -C "$root" ls-tree HEAD -- "$p" 2>/dev/null)" ] && git -C "$root" rev-parse --verify -q HEAD >/dev/null 2>&1 || return 1
                t=absent
            fi
            printf '%s %s\n' "$p" "$t"
        done
    } | sha256sum | cut -c1-24 >"$work/key"
    st=("${PIPESTATUS[@]}")
    [ "${st[0]}" = 0 ] && [ "${st[1]}" = 0 ] || return 1
    k=$(cat "$work/key")
    case $k in *[!0-9a-f]* | '') return 1 ;; esac
    [ ${#k} = 24 ] || return 1
    printf '%s' "$k"
}

# touched_since MARKER: true when a file under the check's paths was written after MARKER (an edit
# made and reverted during the run leaves its mtime); target, .git and __pycache__ are left out
touched_since() {
    local p out start
    for p in "${parr[@]}"; do
        [ -e "$root/$p" ] || continue
        start=$root; [ "$p" = . ] || start=$root/$p
        # files only: a directory's mtime moves when an entry is added or removed inside it
        out=$(find "$start" \( -path "$root/target" -o -path "$root/.git" -o -name __pycache__ \) -prune -o ! -type d -newer "$1" -print -quit 2>/dev/null) || return 0
        [ -z "$out" ] || return 0
    done
    return 1
}

cmd_key() {
    load "$1" || { echo "gate: no check named $1" >&2; return 2; }
    load_paths
    compute_blocked
    [ -z "$blocked" ] || { echo "gate: $1 has no key: $blocked" >&2; return 1; }
    [ "$stamp" = yes ] || { echo "gate: $1 is never stamped" >&2; return 1; }
    dirty && { echo "gate: $1 has a dirty tree under its paths" >&2; return 1; }
    keyof || { echo "gate: $1: git failed, no key" >&2; return 1; }
}

declare -a rows
fails=0

record() { rows+=("$(printf '%-18s %-8s %6s  %s' "$1" "$2" "$3" "$4")"); }

run_one() {
    local name=$1 key='' key2 t0 t1 note='' at=''
    load "$name" || { echo "gate: no check named $name" >&2; record "$name" fail 0 "not in the manifest"; fails=$((fails + 1)); return; }
    load_paths
    local can=0 commit=''
    if [ "$stamp" != yes ]; then note="not stamped"
    elif [ -n "$blocked" ]; then note="not reused, not stamped: $blocked"
    elif dirty; then note="dirty tree under its paths (or git failed): not reused, not stamped"
    elif ! key=$(keyof); then note="git failed, no key: not reused, not stamped"
    else can=1; commit=$(git -C "$root" rev-parse --short HEAD 2>/dev/null) || can=0; fi
    if [ $can = 1 ] && [ "${GATE_FRESH:-0}" != 1 ] && [ -f "$stamps/$name-$key" ]; then
        at=$(cat "$stamps/$name-$key")
        echo "gate: $name: reused ($at)"
        record "$name" reused 0 "$at"
        return
    fi
    [ "${GATE_FRESH:-0}" = 1 ] && [ -z "$note" ] && note="GATE_FRESH=1"
    echo "gate: $name: running: $cmd"
    : >"$work/marker"
    t0=$(date +%s)
    # a clean make environment: the outer command line's flags and variables are judged above
    ( unset MAKEFLAGS MFLAGS MAKELEVEL; eval "$cmd" )
    local rc=$?
    t1=$(date +%s)
    if [ $rc -eq 0 ]; then
        if [ $can = 1 ]; then
            if dirty || ! key2=$(keyof) || [ "$key2" != "$key" ]; then
                note="tree changed during the run: not stamped"
            elif touched_since "$work/marker"; then
                note="a file under its paths was written during the run: not stamped"
            else
                mkdir -p "$stamps"
                printf 'passed at %s, %s\n' "$commit" "$(date '+%Y-%m-%d %H:%M')" >"$stamps/$name-$key"
            fi
        fi
        if [ "$name" = web-checks ] && [ -f "$root/target/web-checks/timing.tsv" ]; then
            note="$(awk -F'\t' '$4=="cached"{c++} END{print c+0}' "$root/target/web-checks/timing.tsv") cached by the web gate's own cache"
        fi
        record "$name" pass $((t1 - t0)) "$note"
    else
        record "$name" fail $((t1 - t0)) "exit $rc"
        fails=$((fails + 1))
    fi
}

cmd_run() {
    local list=("$@") n
    compute_blocked
    [ -z "$blocked" ] || echo "gate: nothing is reused or stamped: $blocked"
    if [ ${#list[@]} -eq 0 ] && [ -n "${GATE_CHECKS:-}" ]; then read -r -a list <<<"$GATE_CHECKS"; fi
    if [ ${#list[@]} -eq 0 ]; then mapfile -t list < <(names | grep -v -- '-fresh$'); fi
    for n in "${list[@]}"; do run_one "$n"; done
    local table
    table=$(printf '%-18s %-8s %6s  %s\n' check result secs note; for r in "${rows[@]}"; do printf '%s\n' "$r"; done)
    printf '\n%s\n' "$table"
    if [ -n "${GATE_REPORT:-}" ]; then
        { printf '\n## make gate, %s, %s\n\n' "$(date '+%Y-%m-%d %H:%M')" "$(git -C "$root" rev-parse --short HEAD)"; printf '%s\n' "$table"; } >>"$GATE_REPORT"
    fi
    [ $fails -eq 0 ]
}

case ${1:-} in
    run) shift; cmd_run "$@" ;;
    key) shift; cmd_key "$@" ;;
    list) names | grep -v -- '-fresh$' ;;
    *) echo "usage: gate.sh run [NAME...] | key NAME | list" >&2; exit 2 ;;
esac
