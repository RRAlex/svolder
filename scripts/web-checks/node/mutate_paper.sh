#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
# Mutates a copy of the theme Paper's sources (ruler.js, notes.js, changes.js, act.js, words.js, draw/computer.js);
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
P=$D/bins/svolderd/web/themes/paper
i=0; killed=0; names=()
while IFS='|' read -r name file from to; do
  case " ${names[*]} " in *" $name "*) ;; *) names+=("$name");; esac
  echo "$name|$file|$from|$to" >> "$D/list"
done <<'LIST'
the ruler is not linear in the steps|ruler.js|(raw - volume.lo) / span(volume)|raw / volume.hi
a place rounds down to its step|ruler.js|Math.round(volume.lo + fraction|Math.floor(volume.lo + fraction
a place is not held to the range|ruler.js|return Math.min(volume.hi, Math.max(volume.lo, step));|return step;
labels may touch|ruler.js|LABEL_GAP = 0.03|LABEL_GAP = 0
more than one note is red (the volume's and the choice)|notes.js|n.red && redVolume ? 'volume' : null|n.red ? 'volume' : null
more than one note is red (the volume's and the choice)|notes.js|red: n.id === redId,|red: n.redKey === redKey,
the peak is redder than the volume|notes.js|['card', 'volume', 'peak', 'path', 'passphrase']|['card', 'peak', 'volume', 'path', 'passphrase']
a run of changes is no run|changes.js|RUN_MS = 3000|RUN_MS = 0
a request is this page's for a minute|act.js|MINE_MS = 3000|MINE_MS = 60000
the first reading is a change|changes.js|if (!before) return null;|if (!before) return { field: 'volume', label: 'volume', from: '', to: '', value: null, at, mine: false };
two hops that change the sound are the only hop|notes.js|changing.length === 1|changing.length >= 1
the EQ's words go smallest first|notes.js|Math.abs(b.db) - Math.abs(a.db)|Math.abs(a.db) - Math.abs(b.db)
a rate loses its decimals|words.js|\d+(?:\.\d+)? kHz|\d+ kHz
a retry at nought seconds waits|words.js|secs > 0 ?|secs >= 0 ?
the percent ruler's labels move|ruler.js|[0, 25, 50, 75, 100]|[0, 20, 50, 75, 100]
the percent ruler's ticks are sparser|ruler.js|PERCENT_TICK = 2|PERCENT_TICK = 5
the table shows seven rows before it scrolls|draw/computer.js|ROWS_SHOWN = 6|ROWS_SHOWN = 7
a dot is not placed from the marker|draw/computer.js|data-y="${(m.y / VIEW_HEIGHT).toFixed(5)}"|data-y="0.5"
both sides are underlined|draw/computer.js|sw.side === name ? 'on' : ''|'on'
the unavailable sentence is not drawn|draw/computer.js|if (sw.unavailable) return|if (false) return
+ Add point stays at sixteen points|draw/computer.js|t.canAdd ? word('c-add'|true ? word('c-add'
the lone point loses its remove|draw/computer.js|  const remove = word('c-remove', '×'|  const remove = p.n > 1 && word('c-remove', '×'
an EQ not confirmed is drawn as running|draw/computer.js|if (!t.verified) lines.push|if (false) lines.push
the device's presets note stays on the computer side|draw/notes.js|['volume', 'tone', 'presets'].includes(n.about)|['volume', 'tone'].includes(n.about)
the cross has no label|words.js|removePoint: (n) => `Remove point ${n}`|removePoint: (n) => `Remove ${n}`
the switch has no stable id|draw/computer.js|id="p-side-${name}" |
LIST
total=${#names[@]}
# the copy itself must pass, or a failure below proves nothing
for h in "$D"/scripts/web-checks/node/paper-*.mjs; do
  node "$h" >/dev/null 2>&1 || { echo "the unmutated copy fails $(basename "$h")"; rm -rf "$D"; exit 2; }
done
for name in "${names[@]}"; do
  rm -rf "$D/orig"; cp -r "$P" "$D/orig"
  while IFS='|' read -r n file from to; do
    [ "$n" = "$name" ] || continue
    python3 - "$P/$file" "$from" "$to" <<'PY'
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
  cp -r "$D/orig/." "$P/"
  if [ "$failed" -eq 1 ]; then
    killed=$((killed+1)); echo "killed:   $name"
  else
    echo "SURVIVED: $name"
  fi
done
echo "$killed of $total Paper source mutations killed"
rm -rf "$D"
[ "$killed" -eq "$total" ]
