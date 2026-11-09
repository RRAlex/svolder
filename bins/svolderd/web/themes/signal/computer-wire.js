/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the computer's side (the area the computer's hop opens): the volume (the buttons, a
// held one repeating, the track dragged or stepped with the keys, Mute), the mode, the presets (one
// recalled, the curve saved as one, one renamed or deleted), the points' table (a row picked, a
// value typed, a type chosen, a point added or removed) and the curve under the mouse and the
// finger: a press on a dot drags its frequency and gain, a tap on an empty spot adds a point, the
// wheel or two fingers set a Q, the arrow keys step a focused dot. The hand, the table and the
// typed cell are the core's (`core.drag.pointDrag`, `table`, `editCell`): this module forwards the
// page's events to them and draws what they say; every write goes through
// `core.actions.computer`. A viewer who may not write taps the same words and the core says why
// nothing happens.
//
// What is being changed is held by the core (`core.computer`), so a reading from the host does not
// move it. The open area is held (`keep`) from a press to the answer of its last write, and the
// shelf while a field is typed in or a list is open; meanwhile this module moves the curve, the
// dots, the cells and the level itself from what the core holds, and the sections are drawn again
// from the newest model afterwards. Flipping to the DAC ends a drag (the core's `track`).

import { askForm, askQuestion, closeAsk } from './ask.js';
import { fractionOf, lineHtml } from './draw/computer.js';
import { curvePath, shadePath } from './draw/tone.js';
import { dragAlong, sliderKeys } from './gestures.js';
import { closeList } from './lists.js';
import { shelfAsks } from './shelf-asks.js';
import { placeOpen } from './place.js';
import { POINTS, SHELF, WORDS } from './words.js';

/** The steppers a long press repeats. */
const STEPPERS = '[data-act="cvol-down"], [data-act="cvol-up"]';
const ACTS = new Set(['cvol-down', 'cvol-up', 'cmute', 'cmode', 'cpreset', 'cpreset-save', 'cpreset-rename',
  'cpreset-delete', 'cadd', 'cremove']);

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** Wire the computer's side of the page in `root` until `signal` aborts; `keep` holds the open
 * area, `shelfKeep` the shelf and `release()` gives one of the shelf's back once the press that
 * closed a list is over. */
export function wireComputer({
  root, core, live, view, keep, shelfKeep, release, signal,
}) {
  const box = root.querySelector('#sg-open');
  const shelf = root.querySelector('#sg-shelf');
  const doc = root.ownerDocument;
  const store = core.model;
  const act = core.actions.computer;
  const { hzText, gainText, qText } = core.words;

  const tone = () => core.computer.tone();
  const volume = () => core.computer.volumeOf(store.page());
  const shown = () => core.computer.side() === 'computer';
  const inert = () => !core.actions.canWrite();
  const quiet = () => view.state === 'lost' || view.state === 'waiting';

  // ------------------------------------------------------------------------------ the table

  /** Light row `n` of the table and its dot. */
  function light(n) {
    for (const row of shelf.querySelectorAll('.sg-prow')) row.classList.toggle('on', Number(row.dataset.n) === n);
    for (const dot of box.querySelectorAll('#sg-cgraph .sg-pt')) dot.classList.toggle('on', Number(dot.dataset.n) === n);
  }

  const scroller = () => shelf.querySelector('.sg-pscroll');
  const table = core.drag.table({
    box: scroller,
    note: () => shelf.querySelector('.sg-pmore'),
    row: (n) => shelf.querySelector(`.sg-prow[data-n="${n}"]`),
    lit: light,
    moreWords: POINTS.more,
  });

  // The shelf is drawn again: the table keeps its scroll and shows its picked row.
  view.shelfDrawn = () => table.restore();
  shelf.addEventListener('scroll', (event) => {
    if (event.target === scroller()) table.scrolled();
  }, { capture: true, passive: true, signal });

  // ------------------------------------------------------------------------- a drag in motion

  /** Put the level, the curve, the dots, the cells and the mode in place from what the core holds
   * now (a drag or a press in motion), without drawing the sections again. */
  function patch() {
    if (!shown()) return;
    const v = volume();
    const track = box.querySelector('#sg-cvtrack');
    if (v && track) {
      const at = (v.pct === null ? 0 : clamp(v.pct / 100, 0, 1)).toFixed(5);
      box.querySelector('#sg-cbig .sg-num').textContent = v.text;
      box.querySelector('#sg-cbig small').textContent = v.pct === null ? '' : v.unit;
      box.querySelector('#sg-cbig').classList.toggle('sg-muted', v.muted);
      track.querySelector('.sg-fill').dataset.at = at;
      track.querySelector('.sg-vknob').dataset.at = at;
      if (v.pct !== null) {
        track.setAttribute('aria-valuenow', String(v.pct));
        track.setAttribute('aria-valuetext', `${v.pct} %`);
      }
      const mute = box.querySelector('[data-act="cmute"]');
      mute.textContent = v.muted ? WORDS.unmute : WORDS.mute;
      mute.setAttribute('aria-pressed', String(v.muted));
      placeOpen(track);
    }
    const t = tone();
    const graph = box.querySelector('#sg-cgraph');
    if (!t || !graph) return;
    graph.querySelector('.sg-curve').setAttribute('d', curvePath(t.response));
    graph.querySelector('.sg-shade').setAttribute('d', shadePath(t.response));
    const eq = t.modes.find((m) => m.name === 'eq').on;
    for (const el of graph.querySelectorAll('.sg-curve, .sg-shade')) el.classList.toggle('faint', !eq);
    const dots = t.markers.map(fractionOf);
    for (const [i, p] of t.points.entries()) {
      const dot = graph.querySelector(`.sg-pt[data-n="${p.n}"]`);
      if (!dot) continue;
      dot.dataset.x = dots[i].x.toFixed(5);
      dot.dataset.y = dots[i].y.toFixed(5);
      dot.setAttribute('aria-valuenow', String(p.g));
      dot.setAttribute('aria-valuetext', gainText(p.g));
      dot.setAttribute('aria-label', POINTS.point(p.n, hzText(p.f), gainText(p.g)));
    }
    placeOpen(graph);
    for (const chip of root.querySelectorAll('[data-act="cmode"]')) {
      const on = t.modes.find((m) => m.name === chip.dataset.mode)?.on === true;
      chip.setAttribute('aria-pressed', String(on));
      chip.classList.toggle('on', on);
    }
    for (const p of t.points) {
      const row = shelf.querySelector(`.sg-prow[data-n="${p.n}"]`);
      if (!row) continue;
      const text = { f: hzText(p.f), g: gainText(p.g), q: qText(p.q) };
      for (const [param, words] of Object.entries(text)) {
        const field = row.querySelector(`[data-param="${param}"]`);
        if (field && doc.activeElement !== field && field.value !== words) field.value = words;
      }
      const type = row.querySelector('select');
      if (type && doc.activeElement !== type) type.value = p.type;
    }
    const line = shelf.querySelector('#sg-cline');
    if (line) line.innerHTML = lineHtml(t);
  }

  // An edit that ended moved a point in the table's frequency order (the core follows the point in
  // hand, a drag's number and a burst's): the focus on its dot or its cells follows it.
  let refocus = null;
  const stopHearing = core.computer.on(({ type, from, to }) => {
    if (type === 'flip') {
      table.reset();
    } else if (type === 'held') {
      patch();
    } else if (type === 'moved') {
      const found = /^(sg-cpt-|sg-crm-|sg-ct-|sg-cf-)(\d+)(.*)$/.exec(doc.activeElement?.id ?? '');
      if (found && Number(found[2]) === from) refocus = `${found[1]}${to}${found[3]}`;
    } else if (type === 'settled' && shown()) {
      live.draw('open');
      live.draw('shelf');
      if (refocus !== null) doc.getElementById(refocus)?.focus({ preventScroll: true });
      refocus = null;
    }
  });

  // ------------------------------------------------------------------------------ the volume

  /** One step of the level (a key, a button): written, shown in place by the core's hold. */
  const nudge = (delta) => {
    if (shown()) act.nudge(delta).catch(() => {});
  };

  let gripped = false;
  dragAlong(box, {
    signal,
    pick: (event) => event.target.closest?.('#sg-cvtrack'),
    onStart(at) {
      const v = volume();
      gripped = false;
      if (!shown() || !v) return;
      if (inert() || v.inert) {
        // the guest is turned away with a word; a writer whose volume is unread writes nothing
        if (inert()) core.actions.refuse();
        return;
      }
      gripped = true;
      keep.take();
      act.setVolume(Math.round(at * 100));
    },
    onMove(at) {
      if (gripped) act.setVolume(Math.round(at * 100));
    },
    onEnd() {
      if (!gripped) return;
      gripped = false;
      keep.drop();
      box.querySelector('#sg-cvtrack')?.focus({ preventScroll: true });
    },
  });

  sliderKeys(box, {
    signal,
    match: '#sg-cvtrack',
    step: (delta) => nudge(delta),
    home: () => {
      if (shown() && volume()?.pct !== null) act.setVolume(0);
    },
  });

  // A held `−` or `+` is the core's hold: one step at the press, then steps at a steady pace. The
  // open area is held while a press lasts, so a reading does not replace the button under the
  // finger.
  let hold = null;
  let give = true;
  live.onSwitch(() => {
    give = false;
    hold?.end();
    give = true;
  });
  box.addEventListener('pointerdown', (event) => {
    const button = event.target.closest?.(STEPPERS);
    if (!button || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
    hold?.end();
    hold = core.hold.repeat(event, {
      root: box,
      el: button,
      step: (el) => nudge(el.dataset.act === 'cvol-up' ? 1 : -1),
      live: () => !inert() && !quiet(),
      leave: true,
      cap: Infinity,
      onStart: () => keep.take(),
      onEnd: () => {
        hold = null;
        if (give) keep.drop();
      },
      signal,
    });
  }, { signal });
  // A long press on a phone must not open the context menu over a held button.
  box.addEventListener('contextmenu', (event) => {
    if (event.target.closest?.(STEPPERS)) event.preventDefault();
  }, { signal });

  // ---------------------------------------------------------------------------- the dragging

  const graphOf = (target) => target.closest?.('#sg-cgraph') ?? null;

  /** The words beside the dragged point: its three values, above it and inside the plot. */
  function say(s) {
    const dot = s.plot.querySelector(`.sg-pt[data-n="${s.n}"]`);
    const words = s.words();
    if (!s.label || !dot || !words) return;
    const rect = s.plot.getBoundingClientRect();
    s.label.textContent = words;
    const at = { x: Number(dot.dataset.x) * rect.width, y: Number(dot.dataset.y) * rect.height };
    const size = { width: s.label.offsetWidth, height: s.label.offsetHeight };
    const spot = core.drag.labelAt(at, { width: rect.width, height: rect.height }, size);
    s.label.style.left = `${spot.x.toFixed(1)}px`;
    s.label.style.top = `${spot.y.toFixed(1)}px`;
  }

  /** After a point was added: its row is lit and in view. */
  function pickAfterAdd(n) {
    if (n > 0) table.pick(n);
  }

  const hand = core.drag.pointDrag({
    points: (graph, rect) => core.drag.markersIn(tone()?.markers, rect),
    usable: shown,
    on: {
      pick: (n) => table.pick(n),
      begin(s) {
        const label = doc.createElement('span');
        label.className = 'sg-dlab';
        label.setAttribute('aria-hidden', 'true');
        s.plot.append(label);
        s.label = label;
        keep.take();
      },
      held: say,
      ended(s) {
        s.label?.remove();
      },
      finished(s) {
        if (!s.switched) keep.drop();
        doc.querySelector(`#sg-cpt-${s.n}`)?.focus({ preventScroll: true });
      },
      burst: (b, on) => (on ? keep.take() : keep.drop()),
      added(answer) {
        if (answer.ok) pickAfterAdd(answer.n);
        else if (answer.full) core.frame.toast(answer.error);
      },
    },
  });
  // The page goes: a drag under way is let go of and its last value written once.
  signal.addEventListener('abort', () => {
    stopHearing();
    hand.destroy();
    view.shelfDrawn = null;
  }, { once: true });

  // any press at all, anywhere on the page (a second finger beside the graph), makes a waiting tap no tap:
  // before the graph's own press, which may begin one
  box.ownerDocument.addEventListener('pointerdown', () => hand.dropTap(), { signal, capture: true });
  box.addEventListener('pointerdown', (event) => {
    hand.finishBurst();
    const graph = graphOf(event.target);
    if (!graph || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
    // A touch grabs only from a dot's own area (which keeps `touch-action: none`): a touch beside
    // it is the page's scroll, or a tap.
    const touch = event.pointerType === 'touch';
    hand.down(event, graph, { grabs: !touch || Boolean(event.target.closest('.sg-pt')) });
  }, { signal });
  box.addEventListener('pointermove', (event) => {
    hand.move(event);
  }, { signal });
  box.addEventListener('pointerup', (event) => {
    hand.up(event);
  }, { signal });
  for (const name of ['pointercancel', 'lostpointercapture']) {
    box.addEventListener(name, (event) => hand.cancel(event), { signal });
  }

  // Two fingers on the graph pinch a point's Q: spread apart to widen the bell, together to narrow
  // it. A finger already on a point makes the pinch of its point; two fingers elsewhere pinch the
  // point nearest their midpoint. The page neither zooms nor scrolls while the two are down.
  box.addEventListener('touchstart', (event) => {
    const graph = graphOf(event.target);
    if (!graph || event.touches.length !== 2) return;
    const [a, b] = event.touches;
    if (graphOf(a.target) !== graph || graphOf(b.target) !== graph) return;
    if (hand.touchStart(event, graph, { single: false }) === 'pinch') hand.watchTouch(graph, true);
  }, { signal, passive: false });

  // A notch over a dot steps its Q (up narrows) and the page does not scroll; elsewhere the page
  // scrolls.
  box.addEventListener('wheel', (event) => {
    const graph = graphOf(event.target);
    if (graph) hand.wheel(event, graph);
  }, { signal, passive: false });

  // The arrow keys on a focused dot step its frequency (left, right) and its gain (down, up) one
  // step of the grid. Escape ends a drag where it is.
  box.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') {
      hand.escape();
      return;
    }
    const dot = event.target.closest?.('.sg-pt[data-n]');
    if (dot) hand.key(event, Number(dot.dataset.n));
  }, { signal });

  // Another device is shown, the stream is lost or the window loses its focus: a drag ends where
  // it is (a lost stream writes nothing more).
  live.onSwitch(() => {
    hand.switched();
    closeAsk();
    closeList();
    core.computer.pick(null);
  });
  live.onReading('state', (state) => {
    if (state === 'lost') hand.lost();
  });
  doc.defaultView?.addEventListener('blur', () => hand.blur(), { signal });

  // --------------------------------------------------------------------------- the points

  /** Turn a field of the table into what is typed: Enter writes the value (the core holds it to the
   * hand ranges and says what is wrong with it), Escape or leaving puts the old one back. The shelf
   * is held while a field has the focus. */
  shelf.addEventListener('focusin', (event) => {
    const field = event.target.closest?.('.sg-pval, .sg-ptype');
    if (!field || shelf.dataset.typing) return;
    shelf.dataset.typing = '1';
    shelfKeep.take();
    const n = Number(field.dataset.n);
    if (n) table.pick(n, { scroll: false });
    if (!field.matches('.sg-pval')) return;
    if (!field.readOnly) field.select?.();
    let over = false;
    core.drag.editCell(field, {
      n,
      param: field.dataset.param,
      shown: field.defaultValue,
      cell: act.cell,
      // the flow is over: the field is let go of and the shelf with it; the text goes back once the
      // typed one has been read, and a value sent shows when the model has it
      restore: () => {
        if (!over) {
          over = true;
          delete shelf.dataset.typing;
          shelfKeep.dropLater();
        }
        field.blur();
      },
      // (the field is a new one, so the flows of the earlier focuses do not pile up on it)
      after: () => {
        field.value = field.defaultValue;
        if (field.isConnected) field.replaceWith(field.cloneNode(true));
      },
    });
  }, { signal });
  shelf.addEventListener('focusout', (event) => {
    if (!event.target.closest?.('.sg-ptype') || !shelf.dataset.typing) return;
    delete shelf.dataset.typing;
    shelfKeep.dropLater();
  }, { signal });

  // a type chosen in a row
  shelf.addEventListener('change', (event) => {
    const select = event.target.closest?.('.sg-ptype');
    if (!select) return;
    const n = Number(select.dataset.n);
    const { value } = select;
    if (inert()) {
      select.value = select.querySelector('[selected]')?.value ?? select.value;
      core.actions.refuse();
      return;
    }
    select.blur();
    act.setPoint(n, { type: value }, { done: true });
  }, { signal });

  // --------------------------------------------------------------------- the presses on words

  const {
    within, ask, ownPreset, rename, remove,
  } = shelfAsks({
    shelf, keep: shelfKeep, release, signal,
  });

  const device = () => ({ device: store.shown() });

  /** The question the daemon hands back for a preset that a device plays too, asked in the shelf's
   * own card under the chip (which has the focus again when it closes): resolves true for Apply,
   * false for Keep or Escape. The shelf may have been drawn again while the answer was on its way:
   * the chip is found by its id then. */
  const asker = (opener) => (words) => new Promise((resolve) => {
    let said = false;
    const here = opener.isConnected ? opener : (doc.getElementById(opener.id) ?? opener);
    ask((onClose) => askQuestion({
      within: within(here),
      words,
      yes: 'apply',
      no: 'keep',
      signal,
      opener: here,
      onClose: () => {
        onClose();
        resolve(said);
      },
      go: async () => {
        said = true;
        return { ok: true };
      },
    }));
  });

  /** The preset the curve is on if it is the person's own, else a list of the person's own. */
  const ownPresetOf = (el, label, then) => {
    const t = tone();
    ownPreset(el, (t?.presets ?? []).filter((p) => !p.built_in), t?.preset, label, then);
  };

  function press(el) {
    const what = el.dataset.act;
    const opened = device();
    if (what === 'cmode') {
      if (el.getAttribute('aria-pressed') !== 'true') act.setMode(el.dataset.mode).catch(() => {});
    } else if (what === 'cpreset') {
      act.recallPreset(el.dataset.name, { asker: asker(el) }).catch(() => {});
    } else if (what === 'cpreset-save') {
      ask((onClose) => askForm({
        within: within(el), kind: 'save', value: '', signal, opener: el, onClose,
        submit: (text) => act.savePreset(text),
      }));
    } else if (what === 'cpreset-rename') {
      ownPresetOf(el, SHELF.whichRename, (name) => rename(el, name,
        (text) => act.renamePreset(name, text, opened)));
    } else if (what === 'cpreset-delete') {
      ownPresetOf(el, SHELF.whichDelete, (name) => remove(el, name,
        () => act.deletePreset(name, opened)));
    } else if (what === 'cadd') {
      act.addNext().then((done) => done.ok && pickAfterAdd(done.n)).catch(() => {});
    } else if (what === 'cremove') {
      const n = Number(el.dataset.n);
      act.removePoint(n).then(() => {
        const t = tone();
        const next = t && doc.querySelector(`#sg-crm-${Math.min(n, t.points.length)}`);
        (next ?? doc.querySelector('#sg-cadd'))?.focus({ preventScroll: true });
      }).catch(() => {});
    }
  }

  root.addEventListener('click', (event) => {
    const el = event.target.closest?.('[data-act]');
    if (!el || !ACTS.has(el.dataset.act) || !root.contains(el)) return;
    const what = el.dataset.act;
    if (what === 'cvol-down' || what === 'cvol-up') {
      // A press of the pointer stepped already; a click with no pointer is a key: one step.
      if (event.detail === 0) nudge(what === 'cvol-up' ? 1 : -1);
      return;
    }
    if (what === 'cmute') {
      act.toggleMute().catch(() => {});
      return;
    }
    // A word that cannot be used does nothing, but for the viewer the core turns away: that call
    // is made so the toast says why.
    if (el.getAttribute('aria-disabled') === 'true' && !inert()) return;
    press(el);
  }, { signal });

  // a row is picked by a press on it (not on a word that acts)
  shelf.addEventListener('click', (event) => {
    const row = event.target.closest?.('.sg-prow');
    if (row && !event.target.closest('select, input, button')) table.pick(Number(row.dataset.n));
  }, { signal });
}
