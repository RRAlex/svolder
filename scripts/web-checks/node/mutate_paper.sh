#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
# Mutates a copy of the theme Paper's sources (ruler.js, notes.js, changes.js, words.js, draw/computer.js)
# and of the core files whose constants it reads (core/computer.js, core/act.js); a file is named
# from the web dir;
# Paper's node harnesses (paper-*.mjs, run from the copy's own tree, so they import the mutated
# files) must fail each time. Run by hand, slow: bash scripts/web-checks/node/mutate_paper.sh
# A line is `name|file|from|to`; lines of one name are one mutation (all its edits at once).
H=$(dirname "$(readlink -f "$0")")
ROOT=$(dirname "$(dirname "$(dirname "$H")")")
mkdir -p "$ROOT/target/web-checks"
D=$(mktemp -d "$ROOT/target/web-checks/mp-XXXX")
# the harnesses sit where they sit in the repo, so their relative imports reach the copy of the web dir
mkdir -p "$D/scripts/web-checks/node" "$D/bins/svolderd"
cp "$H"/paper-*.mjs "$D/scripts/web-checks/node/"
ln -s "$H/fixtures" "$D/scripts/web-checks/node/fixtures"
cp -r "$ROOT/bins/svolderd/web" "$D/bins/svolderd/web"
W=$D/bins/svolderd/web
i=0; killed=0; names=()
while IFS='|' read -r name file from to; do
  case " ${names[*]} " in *" $name "*) ;; *) names+=("$name");; esac
  echo "$name|$file|$from|$to" >> "$D/list"
done <<'LIST'
the ruler is not linear in the steps|themes/paper/ruler.js|(raw - volume.lo) / span(volume)|raw / volume.hi
a place rounds down to its step|themes/paper/ruler.js|Math.round(volume.lo + fraction|Math.floor(volume.lo + fraction
a place is not held to the range|themes/paper/ruler.js|return Math.min(volume.hi, Math.max(volume.lo, step));|return step;
labels may touch|themes/paper/ruler.js|LABEL_GAP = 0.03|LABEL_GAP = 0
more than one note is red (the volume's and the choice)|themes/paper/notes.js|n.red && redVolume ? 'volume' : null|n.red ? 'volume' : null
more than one note is red (the volume's and the choice)|themes/paper/notes.js|red: n.id === redId,|red: n.redKey === redKey,
the peak is redder than the volume|themes/paper/notes.js|['card', 'volume', 'peak', 'path', 'passphrase']|['card', 'peak', 'volume', 'path', 'passphrase']
a run of changes is no run|themes/paper/changes.js|RUN_MS = 3000|RUN_MS = 0
a request is this page's for a minute|core/act.js|MINE_MS = 3000|MINE_MS = 60000
the first reading is a change|themes/paper/changes.js|if (!before) return null;|if (!before) return { field: 'volume', label: 'volume', from: '', to: '', value: null, at, mine: false };
two hops that change the sound are the only hop|themes/paper/notes.js|changing.length === 1|changing.length >= 1
the EQ's words go smallest first|themes/paper/notes.js|Math.abs(b.db) - Math.abs(a.db)|Math.abs(a.db) - Math.abs(b.db)
a rate loses its decimals|themes/paper/words.js|\d+(?:\.\d+)? kHz|\d+ kHz
a retry at nought seconds waits|themes/paper/words.js|secs > 0 ?|secs >= 0 ?
the percent ruler's labels move|themes/paper/ruler.js|[0, 25, 50, 75, 100]|[0, 20, 50, 75, 100]
the percent ruler's ticks are sparser|themes/paper/ruler.js|PERCENT_TICK = 2|PERCENT_TICK = 5
the table shows seven rows before it scrolls|core/computer.js|ROWS_SHOWN = 6|ROWS_SHOWN = 7
a dot is not placed from the marker|themes/paper/draw/computer.js|data-y="${(m.y / VIEW_HEIGHT).toFixed(5)}"|data-y="0.5"
both sides are underlined|themes/paper/draw/computer.js|sw.side === name ? 'on' : ''|'on'
the unavailable sentence is not drawn|themes/paper/draw/computer.js|if (sw.unavailable) return|if (false) return
+ Add point stays at sixteen points|themes/paper/draw/computer.js|t.canAdd ? word('c-add'|true ? word('c-add'
the lone point loses its remove|themes/paper/draw/computer.js|  const remove = word('c-remove', '×'|  const remove = p.n > 1 && word('c-remove', '×'
an EQ not confirmed is drawn as running|themes/paper/draw/computer.js|if (!t.verified) lines.push|if (false) lines.push
the device's presets note stays on the computer side|themes/paper/draw/notes.js|['volume', 'tone', 'presets'].includes(n.about)|['volume', 'tone'].includes(n.about)
the cross has no label|themes/paper/words.js|removePoint: (n) => `Remove point ${n}`|removePoint: (n) => `Remove ${n}`
the switch has no stable id|themes/paper/draw/computer.js|id="p-side-${name}" |
the point in hand is not drawn lit|themes/paper/draw/computer.js|const selected = view.computer.picked(page);|const selected = null;
a Q is written with one decimal|themes/paper/draw/computer.js|q: qText }|q: (q) => q.toFixed(1) }
a gesture's drop counts against an open form|core/freeze.js|gestures = Math.max(0, gestures - 1);|gestures = Math.max(0, gestures - 1); asks = Math.max(0, asks - 1);
LIST
total=${#names[@]}
# the copy itself must pass, or a failure below proves nothing
for h in "$D"/scripts/web-checks/node/paper-*.mjs; do
  node "$h" >/dev/null 2>&1 || { echo "the unmutated copy fails $(basename "$h")"; rm -rf "$D"; exit 2; }
done
for name in "${names[@]}"; do
  rm -rf "$D/orig"; cp -r "$W" "$D/orig"
  while IFS='|' read -r n file from to; do
    [ "$n" = "$name" ] || continue
    python3 - "$W/$file" "$from" "$to" <<'PY'
import sys
s = open(sys.argv[1]).read()
assert s.count(sys.argv[2]) == 1, sys.argv[2]
open(sys.argv[1], 'w').write(s.replace(sys.argv[2], sys.argv[3], 1))
PY
  done < "$D/list"
  failed=0
  for h in "$D"/scripts/web-checks/node/paper-*.mjs; do
    if ! node "$h" >/dev/null 2>&1; then failed=1; break; fi
  done
  cp -r "$D/orig/." "$W/"
  if [ "$failed" -eq 1 ]; then
    killed=$((killed+1)); echo "killed:   $name"
  else
    echo "SURVIVED: $name"
  fi
done
echo "$killed of $total Paper source mutations killed"
rm -rf "$D"
[ "$killed" -eq "$total" ]
