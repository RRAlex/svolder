#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 the svolder authors
# SPDX-License-Identifier: GPL-3.0-or-later
# Mutates a copy of the core's computer writes (core/computer-actions.js and the write.js and
# actions.js beside it) and the helpers they use (words, steps, the hand ranges, the question);
# core-computer.mjs and core-words.mjs (run from the copy's own tree, so they import the mutated
# file) must fail each time. The harness is deterministic, with no browser and no timing. Run by
# hand: bash scripts/web-checks/node/mutate_computer.sh
# A line is `name|file|from|to`. MUTANT_ONLY=<regex> runs only the mutants whose name matches.
H=$(dirname "$(readlink -f "$0")")
ROOT=$(dirname "$(dirname "$(dirname "$H")")")
mkdir -p "$ROOT/target/web-checks"
D=$(mktemp -d "$ROOT/target/web-checks/mc-XXXX")
mkdir -p "$D/scripts/web-checks/node" "$D/bins/svolderd"
cp "$H"/core-computer.mjs "$H"/core-words.mjs "$H"/core-table.mjs "$H"/core-point-drag.mjs "$D/scripts/web-checks/node/"
mkdir -p "$D/crates/svolder-host/src/eq"
cp "$ROOT/crates/svolder-host/src/eq/points.rs" "$ROOT/crates/svolder-host/src/eq/control.rs" "$D/crates/svolder-host/src/eq/"
ln -s "$H/fixtures" "$D/scripts/web-checks/node/fixtures"
cp -r "$ROOT/bins/svolderd/web" "$D/bins/svolderd/web"
P=$D/bins/svolderd/web/core
i=0; killed=0; names=()
while IFS='|' read -r name file from to; do
  if [ -n "${MUTANT_ONLY:-}" ] && ! [[ $name =~ $MUTANT_ONLY ]]; then continue; fi
  names+=("$name")
  echo "$name|$file|$from|$to" >> "$D/list"
done <<'LIST'
a point set drops its slot from the body|computer-actions.js|slot === undefined ? { ...body, done } : { ...body, done, slot }|{ ...body, done }
a point remove drops its slot from the body|computer-actions.js|slot === undefined ? {} : { slot }|{}
a point set sends its place as the slot|computer-actions.js|{ ...body, done, slot }|{ ...body, done, slot: at }
a 409 for a moved point does not end the drag|computer-actions.js|ended: Boolean(result.moved) && !done,|ended: false,
a 409 for a moved point is toasted in the daemon's words|computer-actions.js|const said = moved ? POINT_MOVED : why;|const said = why;
a remove of a point that moved is toasted in the daemon's words|computer-actions.js|movedPoint(answer) ? { ...answer, error: POINT_MOVED, moved: true } : answer|answer
a cell equal to the point's own value is written|computer-actions.js|typedValue === current) {|false) {
a cell equal to the cell's own text is written|computer-actions.js|typedValue === parse(shown ?? '')|false
a cell typed past the hand range is not held to it|computer-actions.js|Math.min(high, Math.max(low, typedValue))|typedValue
a cell that is not a number writes on|computer-actions.js|const value = readTyped(typed, low, high);|const value = readTyped(typed, low, high) ?? 0;
a step goes against its key|tone-drag.js|return Math.sign(table[at] - point[param]) === delta ? table[at] : null;|return table[at];
a guest can write|write.js|  if (viewer?.guest) return false;|  void viewer;
a page whose writes are off can write|write.js|  return viewer?.writes !== false;|  return true;
a refusal does not turn a guest away|actions.js|refuse: () => writes.turnedAway(),|refuse: () => null,
a question forgets where the focus returns|ask.js|const back = () => (open.returnTo ? document.getElementById(open.returnTo) : null);|const back = () => null;
a point removed that moved loses its flag|computer-actions.js|error: POINT_MOVED, moved: true }|error: POINT_MOVED }
a typed k with no number reads as zero|words.js|body !== '' && Number.isFinite(number)|Number.isFinite(number)
a comma that may be thousands is only a decimal mark|computer-actions.js|const thousands = thousandsReading(typed);|const thousands = null;
a comma is thousands when the decimal reading is in range too|computer-actions.js|if (inside(decimal) !== inside(thousands)) return inside(decimal) ? decimal : thousands;|if (!inside(thousands)) return decimal; return thousands;
a comma the range cannot settle is guessed|computer-actions.js|  return { error: `Write ${decimal} or ${kilo}.` };|  return decimal;
a comma the range cannot settle is refused without the way to write it|computer-actions.js|`Write ${decimal} or ${kilo}.`|'Not a number.'
a thousands reading keeps its commas|words.js|bare.replaceAll(',', '')|bare
a thousands reading takes a group of two digits|words.js|(,\d{3})+|(,\d{2,3})+
a thousands reading is every comma number|words.js|/^[+-]?[1-9]\d{0,2}(,\d{3})+$/.test(bare) ? |/^[+-]?\d+,\d+$/.test(bare) ? 
a thousands reading takes a group that starts with 0|words.js|/^[+-]?[1-9]\d{0,2}(,\d{3})+$/.test(bare) ? |/^[+-]?\d{1,3}(,\d{3})+$/.test(bare) ? 
a comma out of range both ways is asked again|computer-actions.js|if (!inside(decimal)) return decimal;|
a typed comma is not a point|words.js|.replace(',', '.')|
a gain's minus is a hyphen|words.js|`−${text}`|`-${text}`
the hand's gain range is wider than the daemon's|computer.js|g: [-12, 12, 1]|g: [-24, 24, 1]
the steps take their top frequency from a literal|tone-drag.js|if (f.at(-1) < fHigh) f.push(fHigh);|
a question's focus is not given again after the answer|ask.js|queueMicrotask(again);|void again;
a cell is not rounded to its field's places|computer-actions.js|const typedValue = round(value, places);|const typedValue = value;
a cell of a field with no hand range is not refused|computer-actions.js|!HAND[param]) return { ok: false, status: 0, error: REFUSED };|false) return { ok: false, status: 0, error: REFUSED };
the most points ignores the model's|computer.js|export const maxPoints = (eq) => eq?.max ?? POINTS_MAX;|export const maxPoints = () => POINTS_MAX;
a full table is added to anyway|computer-actions.js|if (count >= max) return|if (count > max) return
an added point is placed by the model's frequencies, not the held ones|computer-actions.js|(_, i) => currentOf(i + 1));|(_, i) => ({ ...(modelPoints()[i] ?? FLAT_POINT) }));
a preset change does not read the list again|computer-actions.js|if (answer.ok) held.resync();|if (false) held.resync();
a large nudge steps by one|computer-actions.js|return computerActions.setVolume(at + delta);|return computerActions.setVolume(at + 1);
a nudge before the first read writes|computer-actions.js|      if (at === null) return|      if (false) return
mute writes what it is already|computer-actions.js|return muteLane(!muted);|return muteLane(muted);
the point in hand does not follow a re-sort|computer.js|picked.n === extra.from) picked.n = extra.to;|picked.n === extra.from) picked.n = picked.n;
the point in hand stays when another device is shown|computer.js|if (picked.device !== shown|if (false
the device's side has a tone|computer.js|const tone = lazy(() => (on ? computer.toneOf(page) : null));|const tone = lazy(() => computer.toneOf(page));
a flip keeps the point in hand|computer.js|      computer.pick(null);|      void 0;
the table counts a row it shows|table.js|&& hidden > 0 ? moreWords(hidden)|? moreWords(hidden)
a row above the view is not scrolled to|table.js|if (top < area.scrollTop) area.scrollTop = top;|if (false) area.scrollTop = top;
the table forgets its scroll across a draw|table.js|kept = box()?.scrollTop ?? 0;|kept = 0;
a drawn table does not show the point in hand|table.js|if (n !== null) reveal(n);|void n;
a typed cell is read after the text is put back|table.js|await cell(n, param, typed, shown);|await cell(n, param, input.value, shown);
a typed cell writes what it showed|table.js|await cell(n, param, typed, shown);|await cell(n, param, shown, shown);
leaving a typed cell does not put the text back|table.js|input.addEventListener('blur', close);|void close;
Escape in a typed cell reaches the page|table.js|event.stopPropagation?.();|void event;
a drag's last value is never written|point-drag.js|if (s.wrote) await s.write.release();|void s.write;
a drag does not follow a re-sort|point-drag.js|if (session?.n === from) session.n = to;|void from;
a pinch starts from the number the drag began with|point-drag.js|const point = pointOf(s.n);|const point = pointOf(s.write.n ?? 1);
a burst's last value is never written|point-drag.js|act.setPoint(b.writes, {}, { done: true });|void b;
a wheel notch is a final write|point-drag.js|act.setPoint(b.writes, { q: steps.q[idx] }, { done: false });|act.setPoint(b.writes, { q: steps.q[idx] }, { done: true });
a key steps against its key|point-drag.js|    if (next !== null) {|    if (true) {
a step of a field that is no field goes on|point-drag.js|if (!STEP_PARAMS.includes(param)) return false;|void param;
a step of two is a step|point-drag.js|if (delta !== 1 && delta !== -1) return false;|void delta;
a step on a page not usable goes on|point-drag.js|    if (!usable()) return false;|    void usable;
a step in a drag under way starts a burst|point-drag.js|if (dragging) return true;|void dragging;
a step on a page that may not write is not turned away|point-drag.js|const turned = refused();|const turned = false;
a step is not written|point-drag.js|    return stepBurst(n, param, delta, hold);|    return true;
a full table's tap is not heard by the theme|point-drag.js|on.added?.(refusal);|void refusal;
a full table's tap does not turn a guest away|point-drag.js|const refusal = actions.refuse() ?? fullAnswer(now.max);|const refusal = fullAnswer(now.max);
a full table's tap says another most|point-drag.js|const refusal = actions.refuse() ?? fullAnswer(now.max);|const refusal = actions.refuse() ?? fullAnswer(16);
a computer with no EQ is told its table is full|point-drag.js|      if (!now) return null;|      void now;
a begin with no plot starts|point-drag.js|    if (!options.plot) return null;|    void options;
a begin while a drag is under way starts|point-drag.js|    if (busy()) return null;|    void busy;
a begin on a page not usable starts|point-drag.js|    if (!usable()) return null;|    void usable;
a begin on a page that may not write is not turned away|point-drag.js|    const away = refused();|    const away = false;
a begin leaves a burst under way|point-drag.js|    finishBurst(); // a burst under way ends first|    void 0;
the hand has no begin|point-drag.js|    begin: start,|    begin: () => null,
a tap is not dropped by the theme|point-drag.js|const dropTap = () => {|const dropTap = () => void 0; const unusedDrop = () => {
a second finger leaves the tap|point-drag.js|    if (event.touches.length >= 2) tap = null;|    void event;
a touch watch leaves the cancel listener on|point-drag.js|    el[call]('touchcancel', end);|    if (on) el[call]('touchcancel', end);
a touch watch's move may not stop the scroll|point-drag.js|    el[call]('touchmove', move, { passive: false });|    el[call]('touchmove', move);
a touch watch forgets the elements it watches|point-drag.js|    watched[on ? 'add' : 'delete'](el);|    void watched;
a touch watch takes off what it never put on|point-drag.js|    if (!on && !watched.has(el)) return;|    void watched;
a drag that ends leaves its plot watched|point-drag.js|    watching(s.plot, false);|    void s;
tearing the hand down leaves the plot watched|point-drag.js|      watching.stop();|      void watching;
the hand's watch does not move the drag|point-drag.js|    move: (event) => touchMove(event), end: (event) => touchEnd(event),|    move: () => {}, end: (event) => touchEnd(event),
the viewBox is wider than the curve|tone-curve.js|export const VIEW_WIDTH = 1000;|export const VIEW_WIDTH = 1200;
the viewBox is taller than the curve|tone-curve.js|export const VIEW_HEIGHT = 20 * RANGE_DB;|export const VIEW_HEIGHT = 300;
the markers are read in another viewBox|point-drag.js|  x: (m.x / VIEW_WIDTH) * rect.width,|  x: (m.x / (VIEW_WIDTH + 1)) * rect.width,
a plot's map reads another viewBox height|tone-drag.js|dbAt: (y) => dbAt(y / plot.height * VIEW_HEIGHT),|dbAt: (y) => dbAt(y / plot.height * (VIEW_HEIGHT + 1)),
the point types lack the high shelf|computer.js|export const POINT_TYPES = ['PK', 'LSC', 'HSC'];|export const POINT_TYPES = ['PK', 'LSC'];
the point types name the low shelf another way|computer.js|export const POINT_TYPES = ['PK', 'LSC', 'HSC'];|export const POINT_TYPES = ['PK', 'LS', 'HSC'];
the label's words drop the Q prefix|point-drag.js|· ${qPrefix}${qText(point.q)}`|· ${qText(point.q)}`
the values of raw steps read the gain's table for the Q|point-drag.js|q: steps.q[raws.q],|q: steps.q[raws.g],
a session's values are unknown as it begins|point-drag.js|: began && pointValues(startOf(steps, began), steps);|: null;
a session's words ignore the options|point-drag.js|labelWords(now, options)|labelWords(now)
a removed point stays in hand|computer-actions.js|if (at === n) computer.repick(null);|if (at === n) void computer;
a removal puts the other points out of hand|computer-actions.js|if (at === n) computer.repick(null);|if (at !== n) computer.repick(null);
the eq's mode ignores the one being changed|computer.js|        on: holds.get('mode')?.value ?? eq.on,|        on: eq.on,
the eq has no preset|computer.js|        preset: eq.preset ?? null,|        preset: null,
the eq is always confirmed|computer.js|        verified: !eq.running_unknown,|        verified: true,
a computer with no EQ has an eq|computer.js|      const eq = modelOf(page).eq;|      const eq = modelOf(page).eq ?? {};
the device's side has an eq|computer.js|(on ? computer.eqOf(page) : null)|(computer.eqOf(page))
the view's eq is made at once|computer.js|        get eq() { return eq(); },|        eq: eq(),
the view's eq is the whole tone|computer.js|(on ? computer.eqOf(page) : null)|(on ? computer.toneOf(page) : null)
a held step is ended by the timer|point-drag.js|    if (hold) time.cancel(b.timer);|    if (false) time.cancel(b.timer);
a held step does not stop a timer already running|point-drag.js|    else rearm(b);|    else void b;
a step on no point says it stepped|point-drag.js|    if (!point) return false; // a point that is not there is no step|    if (!point) return true;
a step answers true whatever it did|point-drag.js|    return stepBurst(n, param, delta, hold);|    return (stepBurst(n, param, delta, hold), true);
a pick the core changed is not heard by the theme|point-drag.js|    if (type === 'picked') {|    if (false) {
a repick says nothing|computer.js|      notify('picked', { n });|      void n;
a removed point in hand is not said|computer-actions.js|if (at === n) computer.repick(null);|if (at === n) computer.pick(null);
a point after a removed one keeps its number|computer-actions.js|computer.repick(at - 1);|computer.repick(at);
a failed remove changes the pick|computer-actions.js|const at = result.ok ? computer?.picked() ?? null : null;|const at = computer?.picked() ?? null;
a begin does not look for its point before it ends the burst|point-drag.js|    if (!pointOf(options.n)) return null;|    void options;
a begin ignores the gesture|point-drag.js|launch(s, gesture, press, startOf(steps, point));|launch(s, 'move', press, startOf(steps, point));
a begin ignores the fingers|point-drag.js|      touchIds,|      touchIds: null,
a burst goes on for a point that was removed|point-drag.js|      if (burst?.n === n) dropBurst();|      void dropBurst;
a removed point is not said to the hand|computer.js|        notify('gone', { n: hold.at });|        void notify;
a removed point is said again at every read|computer.js|        hold.gone = true;|        void hold;
a tap has no time limit|point-drag.js|time.now() - t.time > TAP_MS|false
a tap that wandered still adds|point-drag.js|if (Math.hypot(now.x - tap.at.x, now.y - tap.at.y) > TAP_PX) tap = null;|void now;
a tap let go far away still adds|point-drag.js|if (Math.hypot(at.x - t.at.x, at.y - t.at.y) > TAP_PX|if (false
a page that may not write is not turned away|point-drag.js|const refused = () => actions.refuse() !== null;|const refused = () => false;
a move with no button down does not end the drag|point-drag.js|if (event.buttons === 0 && event.pointerType === 'mouse') {|if (false) {
Escape does not end the drag|point-drag.js|escape() { session?.drag.cancel('esc'); },|escape() {},
a second press starts while the first is down|point-drag.js|    if (busy() |    if (false 
a question's focus is not watched for a late redraw|ask.js|watch.observe(document.body, { childList: true, subtree: true });|void watch;
a press of the person's own does not end the focus watch|ask.js|document.addEventListener('pointerdown', handOff, true);|void handOff;
a key of the person's own does not end the focus watch|ask.js|document.addEventListener('keydown', handOff, true);|void handOff;
a table in a tbody counts its one child|table.js|const listed = [...rows(area)];|const listed = [...area.children];
the exact count is the mean one|table.js|if (exact) {|if (false) {
the fade class is never put on|table.js|if (fadeClass) area.classList.toggle(fadeClass, hidden > 0);|void fadeClass;
the count never reaches the callback|table.js|onMore(hidden);|void onMore;
a wheel is never wide|point-drag.js|{ reach: REACH.wheel, wide, picked: computer.picked() }|{ reach: REACH.wheel, picked: computer.picked() }
a key burst steps from the model after a re-sort|point-drag.js|b.point ??= { ...act.current(b.writes) };|b.point = { ...act.current(b.writes) };
a press with another button is a tap|point-drag.js|if ((event.button ?? 0) !== 0) return null;|void event;
a press of a second pointer is a tap|point-drag.js|if (event.isPrimary === false) return null;|void event;
a pointer cancel does not let go a drag|point-drag.js|if (s && !s.over && event.pointerId === s.pointerId) s.drag.release();|void s;
a flip does not end a burst|point-drag.js|forget: computer.track({ end: finishBurst }),|forget: computer.track({ end() {} }),
a burst does not follow a re-sort|point-drag.js|if (burst?.n === from) burst.n = to;|void from;
the pointer of an ended drag is lost|point-drag.js|s.lastPointer = s.pointerId;|s.lastPointer = null;
tearing down leaves a drag under way|point-drag.js|if (session && !session.over) session.drag.cancel('lost');|void session;
LIST
total=${#names[@]}
{ for h in core-computer core-words core-table core-point-drag; do node "$D/scripts/web-checks/node/$h.mjs" || exit 1; done; } >/dev/null 2>&1 || { echo "the unmutated copy fails core-computer.mjs"; rm -rf "$D"; exit 2; }
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
  if out=$(for h in core-computer core-words core-table core-point-drag; do node "$D/scripts/web-checks/node/$h.mjs" 2>&1 || exit 1; done); then
    echo "SURVIVED: $name"
  else
    case "$out" in *SyntaxError*) echo "BROKEN (a syntax error is no kill): $name"; rm -rf "$P"; cp -r "$D/orig" "$P"; continue ;; esac
    killed=$((killed+1)); echo "killed:   $name ($(printf '%s\n' "$out" | grep -o 'core-[a-z-]*\.mjs:[0-9][0-9]*' | head -n 1))"
  fi
  rm -rf "$P"; cp -r "$D/orig" "$P"
done
echo "$killed of $total computer write mutations killed"
rm -rf "$D"
[ "$killed" -eq "$total" ]
