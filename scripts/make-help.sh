#!/bin/sh
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# `make help`: the Makefile's targets by section, one short line each, or one target's details.
#   make-help.sh MAKEFILE            the list
#   make-help.sh MAKEFILE TARGET     TARGET's details and settings
#   make-help.sh --check MAKEFILE    fail if a summary or a settings line is too long, or a detail
#                                    line has no target
#
# The Makefile's own words feed it:
#   ##@ Section                     the targets after it belong to Section (first seen, first shown)
#   #: text                         a detail line of the target right below
#   #: NAME=a|b|<what>   text       a setting of that target and the values it takes: `a|b` are
#                                   the values, `<what>` says what to write; text says what it does
#                                   and its default. The list shows NAME=a|b|<what> under the target.
#   target: deps ## Summary         the summary, at most $max characters
set -eu

max=60
cols=100
mode=list
if [ "${1:-}" = --check ]; then
  mode=check
  shift
fi
makefile=$1
target=${2:-}
[ "$mode" = list ] && [ -n "$target" ] && mode=one

# Styling only on a terminal, and never with NO_COLOR set or TERM=dumb: a pipe or a file gets plain text.
bold='' dim='' warn='' off=''
if [ "$mode" != check ] && [ -t 1 ] && [ -z "${NO_COLOR:-}" ] && [ "${TERM:-dumb}" != dumb ]; then
  bold=$(printf '\033[1m') dim=$(printf '\033[2m') warn=$(printf '\033[33m') off=$(printf '\033[0m')
fi

awk -v mode="$mode" -v want="$target" -v max="$max" -v cols="$cols" \
  -v B="$bold" -v D="$dim" -v W="$warn" -v O="$off" '
/^##@ / {
  sec = substr($0, 5)
  if (!(sec in seen)) { seen[sec] = 1; order[++nsec] = sec }
  nd = 0; next
}
/^#:/ {
  line = substr($0, 3); sub(/^ /, "", line)
  detail[++nd] = line; dline[nd] = NR; next
}
/^[a-zA-Z0-9][a-zA-Z0-9_-]*:.*## / {
  name = $0; sub(/:.*/, "", name)
  sum = $0; sub(/^[^#]*## /, "", sum)
  names[++nt] = name; summary[name] = sum; section[name] = sec; at[name] = NR
  text = ""; ns[name] = 0
  for (i = 1; i <= nd; i++) {
    if (detail[i] ~ /^[A-Z_]+=/) {
      k = ++ns[name]
      field = detail[i]; sub(/   .*/, "", field)
      what = detail[i]; sub(/^[^ ]*/, "", what); sub(/^ */, "", what)
      sfield[name, k] = field; swhat[name, k] = what
    } else text = text detail[i] "\n"
  }
  details[name] = text
  if (length(name) > width) width = length(name)
  if (mode == "check" && sec == "") {
    printf "Makefile:%d: %s: no ##@ section above it, so make help would not list it\n", NR, name
    bad = 1
  }
  if (mode == "check" && length(sum) > max) {
    printf "Makefile:%d: %s: the summary is %d characters, at most %d (put the rest on #: lines above)\n", NR, name, length(sum), max
    bad = 1
  }
  nd = 0; next
}
{
  if (mode == "check" && nd > 0) {
    printf "Makefile:%d: a #: line not right above a target with a ## summary\n", dline[1]
    bad = 1
  }
  nd = 0
}
# The settings line(s) under a target in the list: NAME=values, two spaces apart, wrapped at cols.
function settings_lines(n,    k, out, line, pad, f) {
  pad = sprintf("%" (width + 4) "s", "")
  out = ""; line = ""
  for (k = 1; k <= ns[n]; k++) {
    f = sfield[n, k]
    if (line != "" && length(pad line "  " f) > cols) { out = out pad line "\n"; line = "" }
    line = line (line != "" ? "  " : "") f
  }
  if (line != "") out = out pad line "\n"
  return out
}
# Text wrapped at cols, each line indented by ind spaces.
function wrap(text, ind,    n, w, i, out, line, pad) {
  pad = sprintf("%" ind "s", ""); n = split(text, w, " "); out = ""; line = ""
  for (i = 1; i <= n; i++) {
    if (line != "" && length(pad line " " w[i]) > cols) { out = out pad line "\n"; line = "" }
    line = line (line != "" ? " " : "") w[i]
  }
  if (line != "") out = out pad line "\n"
  return out
}
# The words that warn, in the warning colour (nothing changes without styling).
function warned(text) { if (W != "") gsub(/drives your real audio|uses a real adapter/, W "&" O, text); return text }
# One value to show in an example: the first of a|b|c that is not a <placeholder>.
function example(f,    name, vals, n, i, v) {
  name = f; sub(/=.*/, "", name)
  vals = f; sub(/^[^=]*=/, "", vals)
  n = split(vals, v, "|")
  for (i = 1; i <= n; i++) if (v[i] !~ /^</) return name "=" v[i]
  return ""
}
END {
  if (mode == "check") {
    for (t = 1; t <= nt; t++) {
      n = names[t]; s = settings_lines(n); m = split(s, l, "\n")
      for (i = 1; i <= m; i++) if (length(l[i]) > cols) {
        printf "Makefile:%d: %s: a setting is too long for one %d-column line: %s\n", at[n], n, cols, l[i]
        bad = 1
      }
    }
    exit bad
  }
  if (mode == "one") {
    if (!(want in summary)) { printf "make help: no target \"%s\" with a summary; make help lists them\n", want; exit 2 }
    printf "%smake %s%s: %s\n", B, want, O, warned(summary[want])
    if (details[want] != "") printf "\n%s", warned(details[want])
    if (ns[want] > 0) {
      printf "\n%sSettings%s, each one optional (a|b: one of these values; <...>: what to write):\n", B, O
      w = 0
      for (k = 1; k <= ns[want]; k++) if (length(sfield[want, k]) > w) w = length(sfield[want, k])
      eg = ""
      for (k = 1; k <= ns[want]; k++) {
        row = sprintf("  %-" w "s   %s", sfield[want, k], swhat[want, k])
        if (length(row) <= cols) printf "  %s%-" w "s%s   %s\n", B, sfield[want, k], O, swhat[want, k]
        else printf "  %s%s%s\n%s", B, sfield[want, k], O, wrap(swhat[want, k], 6)
        e = example(sfield[want, k]); if (e != "") eg = eg " " e
      }
      if (eg != "") printf "\nFor example: %smake %s%s%s\n", B, want, eg, O
    }
    exit 0
  }
  printf "Usage: make <target> [SETTING=value ...]   (the settings a target takes are listed under it)\n"
  for (s = 1; s <= nsec; s++) {
    printf "\n%s%s%s\n", B, order[s], O
    for (t = 1; t <= nt; t++) {
      n = names[t]
      if (section[n] != order[s]) continue
      sl = settings_lines(n)
      if (sl != "" && D != "") { sub(/\n$/, "", sl); gsub(/\n/, O "\n" D, sl); sl = D sl O "\n" }
      printf "  %s%-" width "s%s  %s\n%s", B, n, O, warned(summary[n]), sl
    }
  }
  printf "\nWhat a target does, its settings and their defaults: %smake help T=<target>%s\n", B, O
}
' "$makefile"
