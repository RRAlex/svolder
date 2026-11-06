#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
# Mutates a copy of the theme Signal's sources (line.js, fold.js, draw/line.js, draw/computer.js, draw/volume.js, draw/playing.js);
# Signal's node harnesses (signal-*.mjs, run from the copy's own tree, so they import the mutated
# files) must fail each time (a harness that fails on the mutated copy while the unmutated copy passes
# is a kill: the harnesses are deterministic, with no browser and no timing). Run by hand, slow: bash scripts/web-checks/node/mutate_signal.sh
# A line is `name|file|from|to`; lines of one name are one mutation (all its edits at once).
H=$(dirname "$(readlink -f "$0")")
ROOT=$(dirname "$(dirname "$(dirname "$H")")")
mkdir -p "$ROOT/target/web-checks"
D=$(mktemp -d "$ROOT/target/web-checks/ms-XXXX")
# the harnesses sit where they sit in the repo, so their relative imports reach the copy of the web dir
mkdir -p "$D/scripts/web-checks/node" "$D/bins/svolderd"
cp "$H"/signal-*.mjs "$D/scripts/web-checks/node/"
ln -s "$H/fixtures" "$D/scripts/web-checks/node/fixtures"
cp -r "$ROOT/bins/svolderd/web" "$D/bins/svolderd/web"
P=$D/bins/svolderd/web/themes/signal
i=0; killed=0; names=()
while IFS='|' read -r name file from to; do
  case " ${names[*]} " in *" $name "*) ;; *) names+=("$name");; esac
  echo "$name|$file|$from|$to" >> "$D/list"
done <<'LIST'
the line drops the hops after the DAC|line.js|  hops.push(...after.map(afterHop));|  void after.map(afterHop);
a declared hop is drawn live|line.js|live: linked === true,|live: true,
a hop that disappears stays chosen|line.js|return chosen && hops.some((hop) => hop.id === chosen) ? chosen : shown;|return chosen ? chosen : shown;
the facts start open|fold.js|get('facts', 'closed') === 'open'|get('facts', 'open') === 'open'
the computer's hop gains a write|draw/computer.js|const head = `<h2>${esc(COMPUTER.title)}</h2>`;|const head = `<h2>${esc(COMPUTER.title)}</h2><button type="button" data-act="mute">x</button>`;
the strip does not centre the chosen hop|draw/line.js|translateX(${drag - at * HOP_PX}px)|translateX(${drag}px)
a linked amp shown alone loses its DAC|line.js|  else if (dac) hops.push(dacBefore(dac));|  else if (dac) void dacBefore;
an amp's faults are picked by their words|draw/amp.js|filter((r) => /^230\./.test(r.id));|filter((r) => /^over-/i.test(r.label));
a muted volume says the word|draw/volume.js|num: muted ? WORDS.silence : said.num,|num: muted ? WORDS.muted : said.num,
a transport icon is not hidden from the readers|draw/playing.js|fill="currentColor" aria-hidden="true"|fill="currentColor"
the mode stays on a desktop's first screen|signal.js|${view.phone() ? modeRow(device) : ''}|${modeRow(device)}
the loudness chip loses its round buttons|draw/choices.js|'sg-st sg-rb'|'sg-st'
the menu row is offered when the sheet cannot open|signal.js|available: () => Boolean(bound.add && bound.canAdd?.() && bound.core?.model.shown()),|available: () => Boolean(bound.add && bound.core?.model.shown()),
LIST
total=${#names[@]}
# the copy itself must pass, or a failure below proves nothing
for h in "$D"/scripts/web-checks/node/signal-*.mjs; do
  node "$h" >/dev/null 2>&1 || { echo "the unmutated copy fails $(basename "$h")"; rm -rf "$D"; exit 2; }
done
for name in "${names[@]}"; do
  rm -rf "$D/orig"; mkdir "$D/orig"; mkdir -p "$D/orig/draw"; cp "$P"/*.js "$D/orig/"; cp "$P"/draw/*.js "$D/orig/draw/"
  while IFS='|' read -r n file from to; do
    [ "$n" = "$name" ] || continue
    python3 - "$P/$file" "$from" "$to" <<'PY'
import sys
s = open(sys.argv[1]).read()
assert s.count(sys.argv[2]) == 1, sys.argv[2]
open(sys.argv[1], 'w').write(s.replace(sys.argv[2], sys.argv[3], 1))
PY
  done < "$D/list"
  failed=0; where=
  for h in "$D"/scripts/web-checks/node/signal-*.mjs; do
    if ! out=$(node "$h" 2>&1); then
      failed=1
      # the failing test: the harness's own line in the stack, with the last test that passed before it
      where=$(printf '%s\n' "$out" | grep -o 'signal-[a-z]*\.mjs:[0-9]*' | head -n 1)
      break
    fi
  done
  cp "$D"/orig/*.js "$P/"; cp "$D"/orig/draw/*.js "$P/draw/"
  if [ "$failed" -eq 1 ]; then
    killed=$((killed+1)); echo "killed:   $name (${where:-?})"
  else
    echo "SURVIVED: $name"
  fi
done
echo "$killed of $total Signal source mutations killed"
rm -rf "$D"
[ "$killed" -eq "$total" ]
