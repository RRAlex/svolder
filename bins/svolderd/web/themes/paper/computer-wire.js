/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the computer's side of the sheet (draw/computer.js, draw/volume.js): the switch by
// the title, the volume (a drag along the 0 to 100 % ruler, the keys, `− quieter`, `mute`,
// `louder +`), the tone (`off` and `EQ`, the points on the graph dragged, tapped to add, stepped
// with the arrow keys and the wheel, the table's type words, typed or stepped cells and `×`,
// `+ Add point`) and the presets. Every write goes through `core.actions.computer`. Flipping
// writes nothing: it ends what is in hand, as for another device shown (live.js).
//
// A drag holds the tone section until its writes are answered and draws the graph and the row from
// the values it asks for; a cell being typed in holds it too, so a read of the computer meanwhile
// does not take the typing away.

import { askForm, askQuestion, closeAsk } from './ask.js';
import { viewerQuiet } from './draw/side.js';
import { heldCurve, valueWords } from './draw/computer.js';
import { dragAlong, sliderKeys } from './gestures.js';
import { closeList, closeOf, openList } from './lists.js';
import { keeper } from './live.js';
import { place } from './place.js';
import { WORDS } from './words.js';
import { markersOf } from '../../core/computer.js';
import { F_MAX, F_MIN } from '../../core/tone-curve.js';

const VIEW_WIDTH = 1000;
const VIEW_HEIGHT = 260;
const TYPES = ['PK', 'LSC', 'HSC'];
/** The hand ranges of a typed value, and the places it is written to. */
const RANGE = { f: [F_MIN, F_MAX, 2], g: [-12, 12, 1], q: [0.1, 10, 2] };
const KEYS = { ArrowLeft: ['f', -1], ArrowRight: ['f', 1], ArrowDown: ['g', -1], ArrowUp: ['g', 1] };

const clamp = (value, low, high) => Math.min(Math.max(value, low), high);
const round = (value, places) => Math.round(value * 10 ** places) / 10 ** places;

/** Wire the computer's side of the sheet in `root` until `signal` aborts. */
export function wireComputer({
  root, core, live, view, signal,
}) {
  const comp = core.computer;
  const actions = core.actions.computer;
  const { computerSteps, createDrag, grab, labelAt, plotMap, pointWriter, REACH, startOf, wheelSteps }
    = core.drag;
  const tables = computerSteps();
  const page = () => core.model.page();
  const on = () => comp.side() === 'computer';
  const quiet = () => viewerQuiet(page());
  const tone = () => comp.toneOf(page());
  const points = () => tone()?.points ?? [];
  const keep = keeper(live, 'tone');
  const nothing = () => {};

  /** A tap that cannot write: the core turns it away and says why (a guest, a page whose writes
   * are off). */
  const refuse = () => actions.setMode(tone()?.modes?.[1]?.on ? 'eq' : 'off').catch(nothing);

  // ---- the switch -----------------------------------------------------------------------------

  root.addEventListener('click', (event) => {
    const word = event.target.closest?.('[data-act="side"]');
    if (word) comp.flip(word.dataset.side);
  }, { signal });

  // ---- the volume -----------------------------------------------------------------------------

  const vbox = root.querySelector('#p-volume');
  const pct = () => comp.volumeOf(page())?.pct ?? null;
  const drawVolume = () => live.draw('volume');

  /** Ask for `value` %: held and shown at once, sent by the core. */
  function setPct(value) {
    if (quiet()) {
      refuse();
      return;
    }
    actions.setVolume(value).catch(nothing);
    drawVolume();
  }

  /** One step of 1 % (the core holds it to the sink's own rule); a viewer who may not write is
   * told so. */
  function step(delta) {
    if (!on() || pct() === null) return;
    if (quiet()) {
      refuse();
      return;
    }
    actions.step(delta).catch(nothing);
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
          refuse();
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
      step(delta) {
        if (!on() || pct() === null) return;
        if (Math.abs(delta) === 1) step(delta);
        else setPct(clamp(pct() + delta, 0, 100));
      },
      home() {
        if (on() && pct() !== null) setPct(0);
      },
    });

    // The written steps repeat while pressed; a click with no press (the keyboard) steps once.
    vbox.addEventListener('pointerdown', (event) => {
      const word = event.target.closest?.('[data-act="c-down"], [data-act="c-up"]');
      if (!word || !on() || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
      core.hold.repeat(event, {
        root,
        el: word,
        signal,
        step: (el) => step(el.dataset.act === 'c-up' ? 1 : -1),
        live: () => on() && !quiet() && pct() !== null,
      });
    }, { signal });
    vbox.addEventListener('click', (event) => {
      const word = event.target.closest?.('[data-act]');
      if (!word || !on()) return;
      if (word.dataset.act === 'c-down' || word.dataset.act === 'c-up') {
        if (event.detail === 0) step(word.dataset.act === 'c-up' ? 1 : -1);
      } else if (word.dataset.act === 'c-mute') {
        if (pct() === null) return;
        if (quiet()) {
          refuse();
          return;
        }
        actions.setMute(!comp.volumeOf(page()).muted).catch(nothing);
        drawVolume();
      }
    }, { signal });
  }

  // ---- the tone -------------------------------------------------------------------------------

  const box = root.querySelector('#p-tone');
  if (box) wireTone(box);

  /** The point `n` is the one the hand is on: its row and its mark are drawn so, in place. */
  function select(n) {
    view.point = n;
    for (const el of box.querySelectorAll('tr[data-n], .pt[data-n]')) {
      el.classList.toggle('sel', Number(el.dataset.n) === n);
    }
  }

  function wireTone(section) {
    live.onSwitch(() => {
      closeList();
      keep.reset();
    });

    // ---- the mode, the type words, the removing and adding

    /** Send the change `fields` of point `n` at once (a typed value, a step, a type). */
    function send(n, fields) {
      if (quiet()) {
        refuse();
        return;
      }
      actions.setPoint(n, fields, { done: true }).catch(nothing);
      live.draw('tone');
    }

    section.addEventListener('click', (event) => {
      const el = event.target.closest?.('[data-act]');
      if (!el || !on()) return;
      const n = Number(el.dataset.n);
      switch (el.dataset.act) {
        case 'c-mode':
          if (quiet()) refuse();
          else {
            actions.setMode(el.dataset.mode).catch(nothing);
            live.draw('tone');
          }
          break;
        case 'c-type': {
          if (quiet()) {
            refuse();
            return;
          }
          if (closeOf(el)) return;
          const now = points()[n - 1]?.type;
          keep.around(() => openList({
            opener: el,
            label: WORDS.typeList,
            options: TYPES.map((t) => ({ value: t, text: t, on: t === now })),
            choose: (type) => send(n, { type }),
            signal,
            onClose: () => keep.drop(),
          }));
          break;
        }
        case 'c-remove':
          if (quiet()) refuse();
          else {
            if (view.point === n) view.point = null;
            actions.removePoint(n).catch(nothing);
          }
          break;
        case 'c-add':
          if (quiet()) {
            refuse();
            return;
          }
          add(comp.addAt(comp.nextFree(points()), 0));
          break;
        default:
      }
    }, { signal });

    /** Add the point `made` (or tell the table is full); the new one is the one in hand. */
    function add(made) {
      if (!made) {
        core.frame.toast(`${tone()?.points?.length ?? 16} is the most`);
        return;
      }
      actions.addPoint(made).then((done) => {
        if (done.ok) select(done.n);
      }).catch(nothing);
    }

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
      select(Number(cell.dataset.n));
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

    /** The cell's text as a value within the hand ranges, or `null` for words that are no number. */
    function parsed(cell) {
      const text = cell.value.replace('−', '-').replace(/[^0-9+\-.]/g, '');
      const value = Number(text);
      if (text === '' || !Number.isFinite(value)) return null;
      const [low, high, places] = RANGE[cell.dataset.param];
      return round(clamp(value, low, high), places);
    }

    /** A typed value is sent as typed (within the hand ranges); words that are no number, or the
     * value already there, put the cell back as it was. */
    function commit(cell) {
      release(cell);
      const n = Number(cell.dataset.n);
      const { param } = cell.dataset;
      const now = points()[n - 1];
      const value = parsed(cell);
      if (!now || value === null || value === now[param] || cell.value === valueWords[param](now[param])) {
        if (now) cell.value = valueWords[param](now[param]);
        return;
      }
      cell.value = valueWords[param](value);
      send(n, { [param]: value });
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
        if (quiet()) refuse();
        else commit(cell);
      } else if (event.key === 'Escape') {
        event.preventDefault();
        release(cell);
        const now = points()[Number(cell.dataset.n) - 1];
        if (now) cell.value = valueWords[cell.dataset.param](now[cell.dataset.param]);
      } else if (event.key === 'ArrowUp' || event.key === 'ArrowDown') {
        event.preventDefault();
        if (quiet()) refuse();
        else stepOf(Number(cell.dataset.n), cell.dataset.param, event.key === 'ArrowUp' ? 1 : -1);
      }
    }, { signal });
    // a cell that cannot be typed in (a guest's) says why on the touch
    section.addEventListener('pointerdown', (event) => {
      if (on() && quiet() && event.target.closest?.('.p-cell')) refuse();
    }, { signal });

    /** The table's step after (or before) point `n`'s `param`: one step of the hand's grid. */
    function stepOf(n, param, delta) {
      const now = points()[n - 1];
      if (!now) return;
      const table = tables[param];
      const at = startOf(tables, now)[param];
      const next = table[clamp(at + delta, 0, table.length - 1)];
      if (Math.sign(next - now[param]) === delta) send(n, { [param]: next });
    }

    // ---- the graph: a drag, a tap, the arrow keys, the wheel

    /** The marks on the drawn graph in px. */
    const marksIn = (graph, rect) => [...graph.querySelectorAll('.pt[data-n]')].map((p) => ({
      band: Number(p.dataset.n),
      x: Number(p.dataset.x) * rect.width,
      y: Number(p.dataset.y) * rect.height,
    }));

    /** The drag's words beside its point: `105 Hz, +5.5 dB`. */
    function say(graph, rect, point, values) {
      let label = graph.querySelector('.p-dlab');
      if (!label) {
        label = graph.ownerDocument.createElement('span');
        label.className = 'p-dlab';
        label.setAttribute('aria-hidden', 'true');
        graph.append(label);
      }
      label.textContent = `${valueWords.f(values.f)}, ${valueWords.g(values.g)}`;
      const spot = labelAt({ x: point.x * rect.width, y: point.y * rect.height },
        { width: rect.width, height: rect.height },
        { width: label.offsetWidth, height: label.offsetHeight });
      label.style.left = `${spot.x.toFixed(1)}px`;
      label.style.top = `${spot.y.toFixed(1)}px`;
    }

    /** The graph and the row drawn from the values a drag asks for, in place. */
    function drawHeld(n, raws) {
      const graph = section.querySelector('#p-graph');
      const t = tone();
      if (!graph || !t) return;
      const values = { f: tables.f[raws.f], g: tables.g[raws.g], q: tables.q[raws.q] };
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

    /** The drags begun and not finished, and the one the pointer has now. */
    const pending = [];
    let active = null;
    let tap = null;
    let rest = 0;

    function begin(graph, n, event, rect, at) {
      const now = points()[n - 1];
      const write = pointWriter({ actions: core.actions, n });
      const entry = { n, pointer: event.pointerId, drag: null, ended: false };
      let untrack = nothing;
      entry.drag = createDrag({
        band: n,
        gesture: 'move',
        start: startOf(tables, now),
        press: at,
        plot: { width: rect.width, height: rect.height },
        steps: tables,
        write,
        map: plotMap({ width: rect.width, height: rect.height }),
        on: {
          held: (raws) => drawHeld(n, raws),
          answer: nothing,
          toast: (words) => core.frame.toast(words),
          ended: () => {
            if (active === entry) active = null;
            untrack();
            try {
              graph.releasePointerCapture(entry.pointer);
            } catch {
              // already let go
            }
            // the last value goes after every write of the drag, then the section is given back
            (async () => {
              await Promise.all(pending.map((p) => p.drag.settled));
              await write.release().catch(nothing);
              const at = pending.indexOf(entry);
              if (at >= 0) pending.splice(at, 1);
              // a switch has reset the holds and forgotten the drag: nothing is left to give back
              if (!entry.switched) keep.drop();
              section.querySelector('.p-dlab')?.remove();
              section.querySelector(`#p-pt-${n}`)?.focus({ preventScroll: true });
            })();
          },
        },
      });
      untrack = comp.track({ cancel: (why) => entry.drag.cancel(why) });
      keep.take();
      pending.push(entry);
      active = entry;
      select(n);
      try {
        graph.setPointerCapture(event.pointerId);
      } catch {
        // without capture the drag still ends on the cancel
      }
    }

    section.addEventListener('pointerdown', (event) => {
      if (!on() || active || !event.isPrimary
        || (event.pointerType === 'mouse' && event.button !== 0)) return;
      const graph = event.target.closest?.('#p-graph');
      if (!graph || !tone()) return;
      const rect = graph.getBoundingClientRect();
      const at = { x: event.clientX - rect.left, y: event.clientY - rect.top };
      const touch = event.pointerType === 'touch';
      // a touch grabs only from a point's own area (which keeps `touch-action: none`): a touch
      // beside it is the page's scroll
      const n = touch && !event.target.closest('.pt') ? null
        : grab(marksIn(graph, rect), at, { reach: touch ? REACH.touch : REACH.mouse });
      if (n === null) {
        // a press on an empty spot that is let go where it was is a tap that adds a point
        tap = { id: event.pointerId, at, time: event.timeStamp };
        return;
      }
      event.preventDefault();
      if (quiet()) {
        refuse();
        return;
      }
      begin(graph, n, event, rect, at);
    }, { signal });

    section.addEventListener('pointermove', (event) => {
      if (!active || event.pointerId !== active.pointer) return;
      const graph = section.querySelector('#p-graph');
      if (!graph) return;
      const rect = graph.getBoundingClientRect();
      active.drag.move({ x: event.clientX - rect.left, y: event.clientY - rect.top });
    }, { signal });

    const finish = (cancelled) => (event) => {
      if (tap && event.pointerId === tap.id) {
        const graph = section.querySelector('#p-graph');
        const mine = tap;
        tap = null;
        if (!cancelled && graph && on() && event.target.closest?.('#p-graph')) {
          const rect = graph.getBoundingClientRect();
          const at = { x: event.clientX - rect.left, y: event.clientY - rect.top };
          if (Math.hypot(at.x - mine.at.x, at.y - mine.at.y) < 8) {
            if (quiet()) refuse();
            else {
              const map = plotMap({ width: rect.width, height: rect.height });
              add(comp.addAt(map.hzAt(at.x), map.dbAt(at.y)));
            }
          }
        }
        return;
      }
      if (!active || event.pointerId !== active.pointer) return;
      const graph = section.querySelector('#p-graph');
      if (graph && !cancelled) {
        const rect = graph.getBoundingClientRect();
        active.drag.move({ x: event.clientX - rect.left, y: event.clientY - rect.top });
      }
      active.drag.release();
    };
    section.addEventListener('pointerup', finish(false), { signal });
    section.addEventListener('pointercancel', finish(true), { signal });
    section.addEventListener('lostpointercapture', finish(true), { signal });

    // another device is shown, or the stream is lost: what is in hand writes nothing more
    live.onSwitch(() => {
      const ending = pending.splice(0);
      active = null;
      for (const entry of ending) {
        entry.switched = true;
        entry.drag.cancel('switch');
      }
    });
    live.onReading('state', (state) => {
      if (state !== 'lost') return;
      for (const p of pending) p.drag.cancel('lost');
    });

    // the arrow keys on a focused point step its frequency and its gain
    section.addEventListener('keydown', (event) => {
      const point = event.target.closest?.('.pt[data-n]');
      const move = KEYS[event.key];
      if (!point || !move || !on() || event.altKey || event.ctrlKey || event.metaKey) return;
      event.preventDefault();
      if (active) return;
      if (quiet()) refuse();
      else stepOf(Number(point.dataset.n), move[0], move[1]);
    }, { signal });

    // the wheel over a point steps its Q: up narrows the bell
    section.addEventListener('wheel', (event) => {
      const point = event.target.closest?.('.pt[data-n]');
      if (!point || active || !on() || quiet()) return;
      event.preventDefault();
      const got = wheelSteps(rest, event.deltaY, event.deltaMode);
      rest = got.rest;
      if (got.steps !== 0) stepOf(Number(point.dataset.n), 'q', got.steps > 0 ? 1 : -1);
    }, { signal, passive: false });

    // the points the table lists scroll: how many are not fully in view below, and the fade
    // over the last (a section drawn again starts from the markup's own count)
    section.addEventListener('scroll', () => {
      const scroller = section.querySelector('#p-ptsbox');
      const label = section.querySelector('#p-more');
      if (!scroller || !label) return;
      const bottom = scroller.scrollTop + scroller.clientHeight;
      const below = [...scroller.querySelectorAll('tbody tr')]
        .filter((r) => r.offsetTop + r.offsetHeight > bottom + 1).length;
      label.hidden = below === 0;
      label.textContent = WORDS.moreBelow(below);
      scroller.classList.toggle('fade', below > 0);
    }, { capture: true, signal });
    // a row the focus reaches (a cell, a word) is the one in hand
    section.addEventListener('focusin', (event) => {
      const row = event.target.closest?.('tr[data-n]');
      if (row) select(Number(row.dataset.n));
    }, { signal });
  }

  // ---- the presets ----------------------------------------------------------------------------

  const pbox = root.querySelector('#p-presets');
  if (pbox) {
    const pkeep = keeper(live, 'presets');
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
          pkeep.drop();
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
        if (quiet()) refuse();
        else actions.recallPreset(el.dataset.name, { asker }).catch(nothing);
      } else if (el.dataset.act === 'c-save') {
        pkeep.around(() => askForm({
          within: within(),
          kind: 'save',
          value: '',
          signal,
          opener: el,
          onClose: () => pkeep.drop(),
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
          onClose: () => pkeep.drop(),
          choose: (value) => {
            const [kind, ...rest] = value.split(':');
            const from = rest.join(':');
            const done = (call) => async () => {
              const answer = await call();
              comp.resync();
              return answer;
            };
            if (kind === 'rename') {
              pkeep.around(() => askForm({
                within: within(),
                kind: 'rename',
                value: from,
                signal,
                opener: el,
                onClose: () => pkeep.drop(),
                submit: (text) => done(() => core.actions.renamePreset(from, text, opened))(),
              }));
            } else {
              pkeep.around(() => askQuestion({
                within: within(),
                words: WORDS.askDelete(from),
                yes: 'delete',
                no: 'keep',
                signal,
                opener: el,
                onClose: () => pkeep.drop(),
                go: done(() => core.actions.deletePreset(from, opened)),
              }));
            }
          },
        }));
      }
    }, { signal });
  }
}
