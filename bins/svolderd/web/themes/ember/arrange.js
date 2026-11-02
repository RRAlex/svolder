/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Arrange mode of the theme Ember: the person moves the screen's sections between the band across
// the top and the two columns, by dragging a section's handle or by the arrow keys on a focused
// handle. The sections are moved in the page, never drawn again (sections.js), so a part that
// the stream swaps meanwhile lands in its section by id and a handle is given to the new copy.
// While it lasts every section's own children are inert (the dial, the curve, the sliders and
// every other drag), and the desktop keys that write leave the page alone. The order is kept per
// browser, per theme and per layout, by Done alone; Escape puts back the order from before and an
// unmount leaves the stored order as it was.

import { menuOwns } from '../../core/menu.js';
import { ember, live as liveOrder } from './bind.js';
import {
  MOVABLE, ZONES, isDefault, moveBy, moveTo, orderFor, resolveOrder,
} from './order.js';
import { layout as placeEffects } from './effects.js';
import { boxOf, lay, sectionEl } from './sections.js';
import { settle } from './visualizer.js';

/** The words for each section: what its handle's label and the spoken position call it. */
const NAMES = {
  playing: 'Now playing',
  visualizer: 'Visualizer',
  volume: 'Volume',
  controls: 'Controls',
  power: 'Power',
  tone: 'Tone control',
  presets: 'Presets',
  'device-presets': 'Device presets',
  meters: 'Meters',
};

/** The sections a person may arrange, as the theme tells the core: an id and its words each. */
export const SECTIONS = MOVABLE.map((id) => ({ id, label: NAMES[id] }));

/** The words for each zone, as the spoken position says them. */
const ZONE_WORDS = { band: 'top band', left: 'left column', right: 'right column' };

const ARROWS = ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'];

/** How near the viewport's edge a held handle scrolls the page, and how far a frame. */
const EDGE = 64;
const SCROLL = 14;

/** The Arrange mode on show, or nothing. */
let active = null;

/** Whether the screen is being arranged: the keys that write leave the page alone then. */
export const arranging = () => active !== null;

/** An element of class `name` holding `text`. */
function make(tag, name, text = '') {
  const el = document.createElement(tag);
  el.className = name;
  el.textContent = text;
  return el;
}

/** A button of class `name`. */
function button(name, text) {
  const el = make('button', name, text);
  el.type = 'button';
  return el;
}

/** Whether the view's columns are locked (the device's link is lost): its sections cannot be used,
 * so there is nothing to arrange until the link is back. */
const locked = (view) => view.querySelector(':scope > .col[inert]') !== null;

/**
 * Arrange the screen in `root` (the mounted theme's element) for a device of `layout`, with the
 * order `prefs` keeps for it. Returns `{ done, cancel, reset, moveTo, order }`: Done keeps the
 * order, cancel puts back the one from before, reset puts the layout's own in place (kept by
 * Done); `moveTo(id, zone, index)` places a section as a drag would and `order()` is the order
 * now. Nothing is returned where there is no screen to arrange.
 */
export function startArrange({ root, core, prefs, layout }) {
  const view = root.querySelector('#screen .view');
  const mount = ember.signal;
  if (!view || locked(view) || !mount || mount.aborted) return null;
  active?.cancel();

  const before = orderFor(prefs, layout);
  let order = before;
  let drag = null;
  let ended = false;
  /** The handle that has the focus (`{ id, el }`), kept while a swap takes it out of the page. */
  let focused = null;
  const stop = new AbortController();
  const opts = { signal: stop.signal };

  /** Whether the screen draws the section `id`. */
  const drawn = (id) => {
    const el = sectionEl(view, id);
    return Boolean(el) && !el.hidden;
  };
  const handleOf = (id) => sectionEl(view, id)?.querySelector(':scope > .arr-handle');

  // ------------------------------------------------------------------ the marks on the page

  const bar = make('div', 'arr-bar');
  bar.setAttribute('role', 'group');
  bar.setAttribute('aria-label', 'Arranging Ember');
  const live = make('span', 'arr-live');
  live.setAttribute('role', 'status');
  live.setAttribute('aria-live', 'polite');
  bar.append(make('b', 'arr-title', 'Arranging Ember'),
    make('span', 'arr-hint', 'Drag a section by ⠿, or focus one and use the arrow keys'), live,
    button('arr-reset', 'Reset to default'), button('arr-done', 'Done'));
  const ghost = make('div', 'arr-ghost');
  const drop = make('div', 'arr-drop');

  /** Say where the section `id` stands, to a screen reader. */
  function say(id) {
    const zone = ZONES.find((z) => order[z].includes(id));
    const shown = order[zone].filter(drawn);
    const text = `${NAMES[id]}: ${ZONE_WORDS[zone]}, ${shown.indexOf(id) + 1} of ${shown.length}`;
    live.textContent = live.textContent === text ? `${text}.` : text;
  }

  /** Give every drawn section its handle and make what else it holds inert; a section drawn
   * again by a swap, or put back by the visualizer's switch, is given them anew. */
  function decorate() {
    for (const id of MOVABLE) {
      const el = sectionEl(view, id);
      if (!el || el.hidden) continue;
      let handle = el.querySelector(':scope > .arr-handle');
      if (!handle) {
        handle = button('arr-handle', '⠿');
        handle.dataset.id = id;
        handle.setAttribute('aria-label', `Move ${NAMES[id]}`);
        el.prepend(handle);
      }
      el.dataset.arr = id;
      for (const child of el.children) {
        if (child === handle || child.hasAttribute('inert')) continue;
        child.setAttribute('inert', '');
        child.dataset.arrInert = '';
      }
      if (drag?.id === id && !el.classList.contains('arr-lift')) {
        lift(el);
        try {
          handle.setPointerCapture(drag.pointer);
        } catch {
          // The pointer is gone: the drag ends at its next event.
        }
      }
    }
  }

  /** Take the marks off the page. */
  function undecorate() {
    for (const handle of view.querySelectorAll('.arr-handle')) handle.remove();
    for (const el of view.querySelectorAll('[data-arr-inert]')) {
      el.removeAttribute('inert');
      delete el.dataset.arrInert;
    }
    for (const el of view.querySelectorAll('[data-arr]')) delete el.dataset.arr;
  }

  /** The element is lifted: it follows the pointer, and `ghost` holds its old place. */
  function lift(el) {
    el.classList.add('arr-lift');
    el.style.setProperty('--arr-x', `${drag.x}px`);
    el.style.setProperty('--arr-y', `${drag.y}px`);
  }

  // ------------------------------------------------------------------ moving

  /** Put the sections in `next` (and, for a key, give the focus back to the one moved: a browser
   * without `moveBefore` drops it). */
  function apply(next, id) {
    const held = id && document.activeElement === handleOf(id);
    order = next;
    liveOrder.order = order;
    lay(view, order, true);
    placeEffects();
    if (id) {
      say(id);
      if (held) handleOf(id)?.focus();
    }
  }

  /** The arrow `key` on the handle of the section `id`. */
  function step(id, key) {
    apply(moveBy(order, id, key, drawn), id);
    handleOf(id)?.scrollIntoView({ block: 'nearest' });
  }

  // ------------------------------------------------------------------ the drag

  /** Whether the sections of `box` (which stand side by side or one under the other) are laid in
   * a row. */
  function inRow(box, rects) {
    if (rects.length > 1) {
      return rects[1].top < rects[0].bottom - 1 && rects[1].left >= rects[0].right - 1;
    }
    const style = getComputedStyle(box);
    return box.classList.contains('p-band') && style.display === 'grid'
      && style.gridTemplateColumns.trim().split(/\s+/).length > 1;
  }

  /** Every place the dragged section may land, each with the line that shows it: `{ zone, index,
   * x1, y1, x2, y2 }` in the viewport's coordinates. The index counts the zone's other sections. */
  function places() {
    const out = [];
    for (const zone of ZONES) {
      const box = boxOf(view, zone);
      if (!box) continue;
      const others = [...box.children]
        .filter((el) => el.dataset.arr && el.dataset.arr !== drag.id);
      const rects = others.map((el) => el.getBoundingClientRect());
      const area = box.getBoundingClientRect();
      if (rects.length === 0) {
        const y = area.top + 6;
        out.push({ zone, index: 0, x1: area.left, x2: area.right, y1: y, y2: y });
      } else if (inRow(box, rects)) {
        for (let index = 0; index <= rects.length; index += 1) {
          const at = rects[Math.min(index, rects.length - 1)];
          const x = index < rects.length ? at.left - 8 : at.right + 8;
          out.push({ zone, index, x1: x, x2: x, y1: at.top, y2: at.bottom });
        }
      } else {
        for (let index = 0; index <= rects.length; index += 1) {
          let y;
          if (index === 0) y = rects[0].top - 8;
          else if (index === rects.length) y = rects[index - 1].bottom + 8;
          else y = (rects[index - 1].bottom + rects[index].top) / 2;
          out.push({ zone, index, x1: area.left, x2: area.right, y1: y, y2: y });
        }
      }
    }
    return out;
  }

  /** The place nearest to the point (`x`, `y`) of the viewport. */
  function nearest(x, y) {
    let best = null;
    let far = Infinity;
    for (const place of places()) {
      const dx = Math.max(place.x1 - x, 0, x - place.x2);
      const dy = Math.max(place.y1 - y, 0, y - place.y2);
      const d = Math.hypot(dx, dy);
      if (d < far) {
        far = d;
        best = place;
      }
    }
    return best;
  }

  /** Show the line of `place`, in the view's own coordinates. */
  function showDrop(place) {
    const box = view.getBoundingClientRect();
    const row = place.x1 === place.x2;
    drop.style.setProperty('left', `${place.x1 - box.left - (row ? 2 : 0)}px`);
    drop.style.setProperty('top', `${place.y1 - box.top - (row ? 0 : 2)}px`);
    drop.style.setProperty('width', row ? '4px' : `${place.x2 - place.x1}px`);
    drop.style.setProperty('height', row ? `${place.y2 - place.y1}px` : '4px');
    drop.hidden = false;
  }

  /** The pointer is at (`x`, `y`): the section follows it and the line shows where it lands. */
  function follow(x, y) {
    drag.at = [x, y];
    drag.x = x - drag.from[0] + (window.scrollX - drag.scroll[0]);
    drag.y = y - drag.from[1] + (window.scrollY - drag.scroll[1]);
    const el = sectionEl(view, drag.id);
    if (el) {
      el.style.setProperty('--arr-x', `${drag.x}px`);
      el.style.setProperty('--arr-y', `${drag.y}px`);
    }
    drag.place = nearest(x, y);
    if (drag.place) showDrop(drag.place);
  }

  /** While the held handle is near the viewport's top or bottom the page scrolls under it. */
  function scrollNear() {
    if (!drag || drag.scrolling) return;
    const y = drag.at[1];
    // The bar stands over the top of the page: a handle held under it or just below it scrolls up.
    const top = Math.max(EDGE, bar.getBoundingClientRect().bottom + 24);
    const by = y < top ? -SCROLL : y > window.innerHeight - EDGE ? SCROLL : 0;
    if (by === 0) return;
    drag.scrolling = true;
    requestAnimationFrame(() => {
      if (!drag) return;
      drag.scrolling = false;
      window.scrollBy(0, by);
      follow(...drag.at);
      scrollNear();
    });
  }

  /** A press on a handle: the section is lifted, its old place keeps a faint outline. */
  function lead(handle, event) {
    const id = handle.dataset.id;
    const el = sectionEl(view, id);
    if (!el || drag) return;
    event.preventDefault();
    try {
      handle.setPointerCapture(event.pointerId);
    } catch {
      // A pointer that is gone cannot be captured: the drag ends at its next event.
    }
    const box = el.getBoundingClientRect();
    const area = view.getBoundingClientRect();
    ghost.style.setProperty('left', `${box.left - area.left}px`);
    ghost.style.setProperty('top', `${box.top - area.top}px`);
    ghost.style.setProperty('width', `${box.width}px`);
    ghost.style.setProperty('height', `${box.height}px`);
    view.append(ghost, drop);
    drag = {
      id, pointer: event.pointerId, from: [event.clientX, event.clientY],
      scroll: [window.scrollX, window.scrollY], x: 0, y: 0, at: [event.clientX, event.clientY],
      place: null, scrolling: false,
    };
    view.classList.add('arr-dragging');
    lift(el);
    follow(event.clientX, event.clientY);
  }

  /** The drag ends: the section lands in the place the line shows, or stays where it was. */
  function land(keep) {
    if (!drag) return;
    const { id, place, pointer } = drag;
    const el = sectionEl(view, id);
    drag = null;
    el?.classList.remove('arr-lift');
    el?.style.removeProperty('--arr-x');
    el?.style.removeProperty('--arr-y');
    ghost.remove();
    drop.remove();
    view.classList.remove('arr-dragging');
    try {
      handleOf(id)?.releasePointerCapture(pointer);
    } catch {
      // Captured by nothing any more.
    }
    if (keep && place) apply(moveTo(order, id, place.zone, place.index, drawn), id);
    handleOf(id)?.focus();
  }

  // ------------------------------------------------------------------ the ways in and out

  /** Arrange mode ends. `how` is `done` (the order is kept), `cancel` (the order from before is
   * back) or `away` (the page is going: nothing is touched but this mode's own marks). */
  function end(how) {
    if (ended) return;
    ended = true;
    liveOrder.order = null;
    land(false);
    if (how === 'done') {
      const stored = prefs.get('arrange', null);
      const plain = stored && typeof stored === 'object' && !Array.isArray(stored);
      const all = plain ? { ...stored } : {};
      if (isDefault(order, layout)) delete all[layout];
      else all[layout] = order;
      let kept = true;
      if (Object.keys(all).length > 0) kept = prefs.set('arrange', all);
      else prefs.remove('arrange');
      if (!kept) core.frame.toast('This browser would not keep the order.');
    } else if (how === 'cancel') {
      order = before;
      lay(view, order);
    }
    stop.abort();
    mount.removeEventListener('abort', leave);
    observer.disconnect();
    undecorate();
    for (const box of view.querySelectorAll('[data-zone]')) delete box.dataset.zone;
    bar.remove();
    ghost.remove();
    drop.remove();
    view.classList.remove('arranging', 'arr-dragging');
    for (const band of view.querySelectorAll(':scope > .p-band')) {
      if (band.children.length === 0) band.remove();
    }
    if (how !== 'away') {
      view.classList.toggle('arranged', !isDefault(order, layout, drawn));
      settle();
      placeEffects();
      core.frame.realign();
      document.getElementById('fx-button')?.focus();
    }
    if (active === api) active = null;
  }

  const api = {
    done: () => end('done'),
    cancel: () => end('cancel'),
    reset() {
      if (ended) return;
      land(false);
      order = resolveOrder(null, layout);
      liveOrder.order = order;
      lay(view, order, true);
      placeEffects();
      live.textContent = 'Reset to the default order.';
    },
    moveTo(id, zone, index) {
      if (!ended) apply(moveTo(order, id, zone, index, drawn), id);
    },
    order: () => order,
  };

  // The page: a section changed by a swap, or put back, is decorated again.
  const observer = new MutationObserver(() => {
    if (ended) return;
    decorate();
    // A handle drawn again by a swap took the focus with it.
    const at = document.activeElement;
    if (focused && !focused.el.isConnected && (!at || at === document.body)) {
      handleOf(focused.id)?.focus({ preventScroll: true });
    }
  });
  view.classList.add('arranging', 'arranged');
  liveOrder.order = order;
  lay(view, order, true);
  (root.querySelector('.app') ?? root).prepend(bar);
  decorate();
  observer.observe(view, { childList: true, subtree: true });
  placeEffects();
  core.frame.realign();
  live.textContent = 'Arranging Ember';

  view.addEventListener('focusin', (event) => {
    const handle = event.target.closest?.('.arr-handle');
    focused = handle ? { id: handle.dataset.id, el: handle } : null;
  }, opts);
  view.addEventListener('focusout', (event) => {
    if (event.relatedTarget) focused = null;
  }, opts);
  view.addEventListener('keydown', (event) => {
    const handle = event.target.closest?.('.arr-handle');
    if (!handle || event.ctrlKey || event.altKey || event.metaKey || !ARROWS.includes(event.key)) {
      return;
    }
    event.preventDefault();
    step(handle.dataset.id, event.key);
  }, opts);
  view.addEventListener('pointerdown', (event) => {
    const handle = event.target.closest?.('.arr-handle');
    if (!handle || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) {
      return;
    }
    lead(handle, event);
  }, opts);
  view.addEventListener('pointermove', (event) => {
    if (drag && event.pointerId === drag.pointer) {
      follow(event.clientX, event.clientY);
      scrollNear();
    }
  }, opts);
  // The drag ends wherever the pointer is let go, even when the handle could not capture it.
  window.addEventListener('pointerup', (event) => {
    if (drag && event.pointerId === drag.pointer) land(true);
  }, opts);
  window.addEventListener('pointercancel', (event) => {
    if (drag && event.pointerId === drag.pointer) land(false);
  }, opts);
  view.addEventListener('lostpointercapture', (event) => {
    if (drag && event.pointerId === drag.pointer && handleOf(drag.id) === event.target) land(false);
  }, opts);
  document.addEventListener('keydown', (event) => {
    // The frame's menu and picker have their own Escape.
    if (event.key !== 'Escape' || menuOwns(event) || document.getElementById('picker')) return;
    event.preventDefault();
    end('cancel');
  }, opts);
  bar.addEventListener('click', (event) => {
    if (event.target.closest('.arr-done')) end('done');
    else if (event.target.closest('.arr-reset')) api.reset();
  }, opts);
  // A theme that unmounts while arranging takes the marks away and leaves the stored order alone.
  const leave = () => end('away');
  mount.addEventListener('abort', leave, { once: true });

  active = api;
  return api;
}

/** Arrange the screen on show: the Arrange row and its key. The first handle takes the focus, so
 * the keyboard is at once in use. Nothing is done where there is nothing to arrange or the page
 * is not in use (a lost link makes the screen inert). */
export function openArrange() {
  const core = ember.core;
  const root = document.getElementById('screen')?.closest('#theme');
  const device = core?.model.page().device;
  if (!core || !root || !device || active || document.getElementById('screen').inert) return null;
  const mode = startArrange({ root, core, prefs: core.prefs, layout: device.layout });
  root.querySelector('.view .arr-handle')?.focus();
  return mode;
}

/** Whether Arrange can start: a screen of a device in use is on show and nothing is being
 * arranged. */
export const canArrange = () => {
  const view = document.querySelector('#screen .view');
  return active === null && ember.core !== null && view !== null && !locked(view)
    && !document.getElementById('screen').inert;
};

/** The screen is drawn anew (another device, another shape): what was arranged cannot go on, and
 * the order from before is back. */
export function endArrange() {
  if (!active) return false;
  active.cancel();
  return true;
}
