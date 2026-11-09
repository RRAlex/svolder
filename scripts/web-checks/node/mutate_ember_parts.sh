#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
# Mutates a copy of the theme Ember's drawing of the page's parts (draw/parts.js, draw/meter-arch.js);
# ember-parts.mjs (run from the copy's own tree, so it imports the mutated files, and held to the
# frozen fixtures/ember-parts.json) must fail each time. The harness is deterministic, with no
# browser and no timing. Run by hand: bash scripts/web-checks/node/mutate_ember_parts.sh
# A line is `name|file|from|to`, the file named from the web dir. MUTANT_ONLY=<regex> runs only the
# mutants whose name matches.
H=$(dirname "$(readlink -f "$0")")
ROOT=$(dirname "$(dirname "$(dirname "$H")")")
mkdir -p "$ROOT/target/web-checks"
D=$(mktemp -d "$ROOT/target/web-checks/mep-XXXX")
# the harness sits where it sits in the repo, so its relative imports reach the copy of the web dir
mkdir -p "$D/scripts/web-checks/node" "$D/bins/svolderd"
cp "$H"/ember-parts.mjs "$H"/html-same.mjs "$D/scripts/web-checks/node/"
ln -s "$H/fixtures" "$D/scripts/web-checks/node/fixtures"
cp -r "$ROOT/bins/svolderd/web" "$D/bins/svolderd/web"
W=$D/bins/svolderd/web
killed=0; names=()
while IFS='|' read -r name file from to; do
  if [ -n "${MUTANT_ONLY:-}" ] && ! [[ $name =~ $MUTANT_ONLY ]]; then continue; fi
  names+=("$name")
  echo "$name|$file|$from|$to" >> "$D/list"
done <<'LIST'
a heading is changed|themes/ember/draw/parts.js|<h2>Volume</h2>|<h2>Level</h2>
the space before Power's word is removed|themes/ember/draw/parts.js|> Power</button>|>Power</button>
an arch's centre is moved by 1|themes/ember/draw/meter-arch.js|fixedEven(100 + r * Math.sin(angle), 2)|fixedEven(101 + r * Math.sin(angle), 2)
the space before a preset's button closes is removed|themes/ember/draw/parts.js|<small>${r.built_in ? 'built in' : 'yours'}</small> </button>|<small>${r.built_in ? 'built in' : 'yours'}</small></button>
the strength's span is renamed|themes/ember/draw/parts.js|<span class="lv" id="loud-strength">|<span class="lvl" id="loud-strength">
the guest's round - and + are left enabled|themes/ember/draw/parts.js|aria-describedby="loud-strength"${off}>${word}</button>|aria-describedby="loud-strength">${word}</button>
the mute's empty fallback is dropped|themes/ember/draw/parts.js|${attr('data-mute', v.mute ?? '')}|${attr('data-mute', v.mute)}
the notes' room is always on|themes/ember/draw/parts.js|<div class="notes${v.room ? ' room' : ''}">|<div class="notes room">
LIST
total=${#names[@]}
# the copy itself must pass, or a failure below proves nothing
node "$D/scripts/web-checks/node/ember-parts.mjs" >/dev/null 2>&1 || { echo "the unmutated copy fails ember-parts.mjs"; rm -rf "$D"; exit 2; }
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
  if out=$(node "$D/scripts/web-checks/node/ember-parts.mjs" 2>&1); then
    echo "SURVIVED: $name"
  else
    case "$out" in *SyntaxError*) echo "BROKEN (a syntax error is no kill): $name" ;;
      *) killed=$((killed+1)); echo "killed:   $name ($(printf '%s\n' "$out" | grep -m1 '^FAIL' | cut -c1-80))" ;;
    esac
  fi
  rm -rf "$W"; cp -r "$D/orig" "$W"
done
echo "$killed of $total ember-parts mutations killed"
rm -rf "$D"
[ "$killed" -eq "$total" ]
