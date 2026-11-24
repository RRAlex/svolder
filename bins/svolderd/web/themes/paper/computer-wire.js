/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the computer's side of the sheet (draw/computer.js, draw/volume.js): the switch by
// the title, the volume (a drag along the 0 to 100 % ruler, the keys, `− quieter`, `mute`,
// `louder +`), the tone (`off` and `EQ`, the points on the graph dragged, tapped to add, stepped
// with the arrow keys and the wheel, the table's type words, typed or stepped cells and `×`,
// `+ Add point`) and the presets. The core does the hand's work (core.eq.points: the drag, the
// tap, the wheel and key bursts; core.eq.table: the rows below the view; core.actions.computer:
// every write); this wires Paper's elements to it and draws what it says. Flipping writes nothing:
// it ends what is in hand, as for another device shown (live.js).
//
// A drag holds the tone section until its writes are answered and draws the graph and the row from
// the values it asks for; a cell being typed in holds it too, so a read of the computer meanwhile
// does not take the typing away.

import { askForm, askQuestion, closeAsk } from './ask.js';
import { CROSSFEED, LOUDNESS, SOUND, SPEAKERS } from '../../core/popups/listed.js';
import { heldCurve, valueWords } from './draw/computer.js';
import { dragAlong, sliderKeys } from '../../core/gestures.js';
import { closeList, closeOf, openList } from './lists.js';
import { place } from './place.js';
import { WORDS } from './words.js';
import { POINT_TYPES, markersOf } from '../../core/computer.js';
import { VIEW_HEIGHT, VIEW_WIDTH } from '../../core/tone-curve.js';

const clamp = (value, low, high) => Math.min(Math.max(value, low), high);

/** Wire the computer's side of the sheet in `root` until `signal` aborts. */
export function wireComputer({
  root, core, live, signal,
}) {
  const comp = core.computer;
  const actions = core.actions.computer;
  const { labelAt } = core.eq.bands;
  const { markersIn } = core.eq.points;
  const page = () => core.model.page();
  const on = () => comp.side() === 'computer';
  const quiet = () => !core.actions.canWrite(page());
  const tone = () => comp.toneOf(page());
  const points = () => tone()?.points ?? [];
  const keep = core.freeze.keeper(live, 'tone');
  const nothing = () => {};

  // ---- the switch -----------------------------------------------------------------------------

  root.addEventListener('click', (event) => {
    const word = event.target.closest?.('[data-act="side"]');
    if (word) comp.flip(word.dataset.side);
  }, { signal });

  // ---- the volume -----------------------------------------------------------------------------

  const vbox = root.querySelector('#p-volume');
  const pct = () => comp.volumeOf(page())?.pct ?? null;
  const drawVolume = () => live.draw('volume');

  /** Ask for `value` %: held and shown at once, sent by the core (which turns a viewer who may not
   * write away, and says why). */
  function setPct(value) {
    actions.setVolume(value).catch(nothing);
    drawVolume();
  }

  /** A step of `delta` % (one is 1 %, held to the sink's own rule; a larger one is a page key). */
  function nudge(delta) {
    if (!on() || pct() === null) return;
    actions.nudge(delta).catch(nothing);
    drawVolume();
  }

  if (vbox) {
    let gripped = false;
    let offset = 0;
    dragAlong(vbox, {
      signal,
      pick: (event) => (on() && pct() !== null ? event.target.closest?.('#p-ruler') : null),
      onStart(at, event) {
        gripped = false;
        if (quiet()) {
          core.actions.refuse();
          return;
        }
        gripped = true;
        // a press on the needle keeps its distance from the pointer, so it does not jump
        offset = event.target.closest?.('#p-needle') ? pct() / 100 - at : 0;
        setPct(Math.round(clamp(at + offset, 0, 1) * 100));
      },
      onMove(at) {
        if (gripped && on()) setPct(Math.round(clamp(at + offset, 0, 1) * 100));
      },
      onEnd() {
        if (!gripped) return;
        gripped = false;
        vbox.querySelector('#p-needle')?.focus({ preventScroll: true });
      },
    });

    sliderKeys(vbox, {
      signal,
      match: '#p-needle',
      step: nudge,
      home() {
        if (on() && pct() !== null) setPct(0);
      },
    });

    // The written steps repeat while pressed; a click with no press (the keyboard) steps once.
    vbox.addEventListener('pointerdown', (event) => {
      const word = event.target.closest?.('[data-act="c-down"], [data-act="c-up"]');
      if (!word || !on() || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
      core.repeat(event, {
        root,
        el: word,
        signal,
        step: (el) => nudge(el.dataset.act === 'c-up' ? 1 : -1),
        live: () => on() && !quiet() && pct() !== null,
      });
    }, { signal });
    vbox.addEventListener('click', (event) => {
      const word = event.target.closest?.('[data-act]');
      if (!word || !on()) return;
      if (word.dataset.act === 'c-down' || word.dataset.act === 'c-up') {
        if (event.detail === 0) nudge(word.dataset.act === 'c-up' ? 1 : -1);
      } else if (word.dataset.act === 'c-mute') {
        if (pct() === null) return;
        actions.toggleMute().catch(nothing);
        drawVolume();
      }
    }, { signal });
  }

  // ---- the tone -------------------------------------------------------------------------------

  const box = root.querySelector('#p-tone');
  if (box) wireTone(box);

  /** The Sound popup's page for a stage word: the EQ library is its first tab (no page); Space is
   * the virtual speakers' tab while they play, else the crossfeed's. */
  function soundPage(stage) {
    if (stage === 'loudness') return LOUDNESS;
    if (stage !== 'space') return null;
    const space = comp.levelsOf(page())?.stages.find((s) => s.key === 'space');
    return space?.kind === 'speakers' ? SPEAKERS : CROSSFEED;
  }

  function wireTone(section) {
    live.onSwitch(() => {
      closeList();
      keep.reset();
    });

    // ---- the table: the point in hand lit, the rows below the view counted

    const label = () => section.querySelector('#p-more');
    const table = core.eq.table({
      box: () => section.querySelector('#p-ptsbox'),
      // the words under the table are Paper's own element, shown and hidden with the count
      note: () => null,
      row: (n) => section.querySelector(`tr[data-n="${n}"]`),
      lit(n) {
        for (const el of section.querySelectorAll('tr[data-n], .pt[data-n]')) {
          el.classList.toggle('sel', Number(el.dataset.n) === n);
        }
      },
      rows: (area) => area.querySelectorAll('tbody tr'),
      exact: true,
      fadeClass: 'fade',
      moreWords: WORDS.moreBelow,
      onMore(hidden) {
        const words = label();
        if (!words) return;
        words.hidden = hidden === 0;
        words.textContent = WORDS.moreBelow(hidden);
      },
    });
    /** The point `n` is the one the hand is on: its row and its mark are drawn so, in place. */
    const select = (n) => table.pick(n, { scroll: false });
    // the points the table lists scroll: how many are not fully in view below, and the fade over
    // the last (a section drawn again starts from the markup's own count)
    section.addEventListener('scroll', () => table.scrolled(), { capture: true, signal });

    // ---- the mode, the type words, the removing and adding

    /** Send the change `fields` of point `n` at once (a type). */
    function send(n, fields) {
      actions.setPoint(n, fields, { done: true }).catch(nothing);
      live.draw('tone');
    }

    /** `+ Add point` and a tap on a full table say how many the EQ holds at most. */
    const full = (max) => core.frame.toast(`${max} is the most`);

    section.addEventListener('click', (event) => {
      const el = event.target.closest?.('[data-act]');
      if (!el || !on()) return;
      const n = Number(el.dataset.n);
      switch (el.dataset.act) {
        case 'c-bypass':
          if (quiet()) {
            core.actions.refuse();
            return;
          }
          actions.setBypass(el.getAttribute('aria-pressed') !== 'true').catch(nothing);
          live.draw('tone');
          break;
        case 'c-stage':
          core.router.open(SOUND, soundPage(el.dataset.stage));
          break;
        case 'c-mode':
          actions.setMode(el.dataset.mode).catch(nothing);
          live.draw('tone');
          break;
        case 'c-type': {
          if (quiet()) {
            core.actions.refuse();
            return;
          }
          if (closeOf(el)) return;
          const now = points()[n - 1]?.type;
          keep.around(() => openList({
            opener: el,
            label: WORDS.typeList,
            options: POINT_TYPES.map((t) => ({ value: t, text: t, on: t === now })),
            choose: (type) => send(n, { type }),
            signal,
            onClose: () => keep.release(),
          }));
          break;
        }
        case 'c-remove':
          actions.removePoint(n).catch(nothing);
          break;
        case 'c-add':
          actions.addNext().then((done) => {
            if (done.ok) select(done.n);
            else if (done.full) full(done.max);
          }).catch(nothing);
          break;
        default:
      }
    }, { signal });

    // ---- the cells: typed or stepped

    // A cell with text typed in it holds the section, so a read of the computer meanwhile does
    // not take the typing away; it is let go when the text is sent, put back or left.
    const dirty = new Set();
    const release = (cell) => {
      if (dirty.delete(cell)) keep.drop();
    };
    section.addEventListener('focusin', (event) => {
      const cell = event.target.closest?.('.p-cell');
      if (!cell) return;
      cell.select?.();
    }, { signal });
    section.addEventListener('input', (event) => {
      const cell = event.target.closest?.('.p-cell');
      if (cell && !dirty.has(cell)) {
        dirty.add(cell);
        keep.take();
      }
    }, { signal });
    section.addEventListener('focusout', (event) => {
      const cell = event.target.closest?.('.p-cell');
      if (cell) release(cell);
    }, { signal });

    /** The cell's text put back as the point has it. */
    function putBack(cell) {
      const now = points()[Number(cell.dataset.n) - 1];
      if (now) cell.value = valueWords[cell.dataset.param](now[cell.dataset.param]);
    }

    /** A typed value is written by the core as typed (`2.9k`, `+3,2`, rounded to the places of its
     * field, held to the hand range); words that are no number (toasted), or the value already
     * there, put the cell back as it was. */
    function commit(cell) {
      release(cell);
      const n = Number(cell.dataset.n);
      const { param } = cell.dataset;
      const now = points()[n - 1];
      if (!now) return;
      actions.cell(n, param, cell.value, valueWords[param](now[param])).then((answer) => {
        if ((!answer.ok || answer.nothing) && cell.isConnected) putBack(cell);
      }).catch(nothing);
      live.draw('tone');
    }

    section.addEventListener('change', (event) => {
      const cell = event.target.closest?.('.p-cell');
      if (cell && on()) commit(cell);
    }, { signal });
    section.addEventListener('keydown', (event) => {
      const cell = event.target.closest?.('.p-cell');
      if (!cell || !on()) return;
      if (event.key === 'Enter') {
        event.preventDefault();
        commit(cell);
      } else if (event.key === 'Escape') {
        event.preventDefault();
        release(cell);
        putBack(cell);
      } else if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
        event.preventDefault();
        actions.stepPoint(Number(cell.dataset.n), cell.dataset.param, event.key === 'ArrowUp' ? 1 : -1)
          .catch(nothing);
        live.draw('tone');
      }
    }, { signal });
    // a cell that cannot be typed in (a guest's) says why on the touch
    section.addEventListener('pointerdown', (event) => {
      if (on() && quiet() && event.target.closest?.('.p-cell')) core.actions.refuse();
    }, { signal });

    // ---- the graph: a drag, a tap, the arrow keys, the wheel (the core's hand)

    /** The drag's words beside its point: `105 Hz, +5.5 dB`. */
    function say(graph, rect, point, values) {
      let words = graph.querySelector('.p-dlab');
      if (!words) {
        words = graph.ownerDocument.createElement('span');
        words.className = 'p-dlab';
        words.setAttribute('aria-hidden', 'true');
        graph.append(words);
      }
      words.textContent = `${valueWords.f(values.f)}, ${valueWords.g(values.g)}`;
      const spot = labelAt({ x: point.x * rect.width, y: point.y * rect.height },
        { width: rect.width, height: rect.height },
        { width: words.offsetWidth, height: words.offsetHeight });
      words.style.left = `${spot.x.toFixed(1)}px`;
      words.style.top = `${spot.y.toFixed(1)}px`;
    }

    /** The graph and the row drawn from the values a drag asks for, in place. The section holds
     * the numbers it was drawn with: `s.drawn` is the point's number in it. */
    function drawHeld(s) {
      const graph = section.querySelector('#p-graph');
      const t = tone();
      if (!graph || !t || !s.wrote) return;
      const n = s.drawn;
      const values = s.values();
      const list = t.points.map((p) => (p.n === n ? { ...p, ...values } : p));
      graph.querySelector('.p-curve').setAttribute('d', heldCurve(list));
      // every dot, the held one and the others it moves, sits on the summed curve
      let point = null;
      for (const m of markersOf(list)) {
        const dot = graph.querySelector(`.pt[data-n="${m.n}"]`);
        if (!dot) continue;
        dot.dataset.x = (m.x / VIEW_WIDTH).toFixed(5);
        dot.dataset.y = (m.y / VIEW_HEIGHT).toFixed(5);
        if (m.n === n) point = { x: m.x / VIEW_WIDTH, y: m.y / VIEW_HEIGHT };
      }
      const mark = graph.querySelector(`.pt[data-n="${n}"]`);
      mark.setAttribute('aria-valuenow', String(values.g));
      mark.setAttribute('aria-valuetext', valueWords.g(values.g));
      place(graph);
      for (const param of ['f', 'g', 'q']) {
        const cell = section.querySelector(`#p-c${n}${param}`);
        if (cell && cell !== cell.ownerDocument.activeElement) cell.value = valueWords[param](values[param]);
      }
      say(graph, graph.getBoundingClientRect(), point, values);
    }

    const hand = core.eq.points({
      points: (plot, rect) => markersIn(tone()?.markers, rect),
      usable: () => on() && Boolean(tone()),
      on: {
        pick: select,
        begin(s) {
          s.drawn = s.n;
          keep.take();
          select(s.n);
        },
        held: drawHeld,
        ended(s) {
          if (s.lastPointer === null) return;
          try {
            s.plot.releasePointerCapture(s.lastPointer);
          } catch {
            // already let go
          }
        },
        // the last value goes after every write of the drag, then the section is given back
        finished(s) {
          if (signal.aborted) return;
          // a switch has reset the holds and forgotten the drag: nothing is left to give back
          if (!s.switched) keep.drop();
          section.querySelector('.p-dlab')?.remove();
          // a drag ended by a flip leaves the focus where the person has put it since
          if (!s.switched) section.querySelector(`#p-pt-${s.n}`)?.focus({ preventScroll: true });
        },
        added(answer) {
          if (answer?.ok) select(answer.n);
          else if (answer?.full) full(answer.max);
        },
      },
    });
    signal.addEventListener('abort', () => hand.destroy(), { once: true });

    // any press at all, anywhere on the page (a second finger, a right button), makes a waiting tap no tap:
    // before the graph's own press, which may begin one
    root.ownerDocument.addEventListener('pointerdown', () => hand.dropTap(), { signal, capture: true });
    section.addEventListener('pointerdown', (event) => {
      if (!on() || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
      const graph = event.target.closest?.('#p-graph');
      if (!graph) return;
      // a touch grabs only from a point's own area (which keeps `touch-action: none`): a touch
      // beside it is the page's scroll
      const grabs = event.pointerType !== 'touch' || Boolean(event.target.closest('.pt'));
      hand.down(event, graph, { grabs });
    }, { signal });
    section.addEventListener('pointermove', (event) => {
      hand.move(event);
    }, { signal });
    section.addEventListener('pointerup', (event) => {
      const done = hand.up(event);
      if (done !== true) done?.catch?.(nothing);
    }, { signal });
    for (const type of ['pointercancel', 'lostpointercapture']) {
      section.addEventListener(type, (event) => hand.cancel(event), { signal });
    }

    // another device is shown, or the stream is lost: what is in hand writes nothing more
    live.onSwitch(() => hand.switched());
    live.onReading('state', (state) => {
      if (state === 'lost') hand.lost();
    });

    // the arrow keys on a focused point step its frequency and its gain, live, and the burst of
    // keys ends in one final write
    section.addEventListener('keydown', (event) => {
      const point = event.target.closest?.('.pt[data-n]');
      if (!point || !on()) return;
      if (hand.key(event, Number(point.dataset.n)) === true) live.draw('tone');
    }, { signal });

    // the wheel over a point steps its Q: up narrows the bell; a burst ends in one final write
    section.addEventListener('wheel', (event) => {
      const point = event.target.closest?.('.pt[data-n]');
      if (!point || !on() || quiet()) return;
      if (hand.wheel(event, point.closest('#p-graph')) === true) live.draw('tone');
    }, { signal, passive: false });

    // a row the focus reaches (a cell, a word) is the one in hand
    section.addEventListener('focusin', (event) => {
      const row = event.target.closest?.('tr[data-n]');
      if (row) select(Number(row.dataset.n));
    }, { signal });
  }

  // ---- the presets ----------------------------------------------------------------------------

  const pbox = root.querySelector('#p-presets');
  if (pbox) {
    const pkeep = core.freeze.keeper(live, 'presets');
    live.onSwitch(() => {
      closeList();
      closeAsk();
      pkeep.reset();
    });
    const within = () => pbox.querySelector('.p-ask');
    /** Paper's own question for a preset the daemon says two sides would play: resolves true on
     * `apply`, false on `keep` or Escape. */
    const asker = (words) => new Promise((resolve) => {
      let said = false;
      pkeep.around(() => askQuestion({
        within: within(),
        words,
        yes: 'apply',
        no: 'keep',
        signal,
        onClose: () => {
          pkeep.release();
          resolve(said);
        },
        go: async () => {
          said = true;
          return { ok: true };
        },
      }));
    });
    const target = () => ({ device: core.model.shown() });
    pbox.addEventListener('click', (event) => {
      const el = event.target.closest?.('[data-act^="c-"]');
      if (!el || !on()) return;
      if (el.dataset.act === 'c-preset') {
        if (el.getAttribute('aria-disabled') === 'true' && !quiet()) return;
        actions.recallPreset(el.dataset.name, { asker }).catch(nothing);
      } else if (el.dataset.act === 'c-save') {
        pkeep.around(() => askForm({
          within: within(),
          kind: 'save',
          value: '',
          signal,
          opener: el,
          onClose: () => pkeep.release(),
          submit: (text) => actions.savePreset(text),
        }));
      } else if (el.dataset.act === 'c-edit') {
        if (closeOf(el)) return;
        const opened = target();
        const own = (tone()?.presets ?? []).filter((r) => !r.built_in);
        const options = own.flatMap((r) => [
          { value: `rename:${r.name}`, text: WORDS.rename(r.name), on: false },
          { value: `delete:${r.name}`, text: WORDS.remove(r.name), on: false },
        ]);
        pkeep.around(() => openList({
          opener: el,
          label: WORDS.editList,
          options,
          signal,
          onClose: () => pkeep.release(),
          choose: (value) => {
            const [kind, ...rest] = value.split(':');
            const from = rest.join(':');
            if (kind === 'rename') {
              pkeep.around(() => askForm({
                within: within(),
                kind: 'rename',
                value: from,
                signal,
                opener: el,
                onClose: () => pkeep.release(),
                submit: (text) => actions.renamePreset(from, text, opened),
              }));
            } else {
              pkeep.around(() => askQuestion({
                within: within(),
                words: WORDS.askDelete(from),
                yes: 'delete',
                no: 'keep',
                signal,
                opener: el,
                onClose: () => pkeep.release(),
                go: () => actions.deletePreset(from, opened),
              }));
            }
          },
        }));
      }
    }, { signal });
  }
}
