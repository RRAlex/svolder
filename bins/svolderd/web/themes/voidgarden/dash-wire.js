/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The dash drawn from the model and the hands on it: `−` and `+` (a press steps once and a held
// one repeats through `core.repeat`; `↑` `↓` do the same from the page, `volumeKey`), `MUTE`, the
// input's chips and list, the transport, the side switch and `Controls…`, which opens the Device
// controls popup through the router. Every write goes through `core.actions` (the cap, the pacing
// and the refusals stay where they are); a viewer who may not write taps the same keys and the
// core says why nothing happens.
//
// The page's state is drawn here too: `data-mood` on the page (the CSS dulls the scene by it), the
// quiet line, and the card of a device out of reach with `Try now` and the seconds to the next
// try. The dash is out of reach (`inert`) while the device is out of reach or the stream is lost.
//
// The dash fades away after the idle time (3 s, 4 s on a phone; opacity only: it stays in the tab
// order and the accessibility tree) and any input brings it back; the input that wakes a faded dash
// only wakes it. Trouble, a held key, an open list, the keyboard focus and a mouse resting over it
// keep it up. The explore keys (`New seed`, `Next scene`, `Effect`) tell the scene as an `explore`
// event on `vg.steps`; on a phone the features chip opens the panel (`data-feat` on the page, told
// to the panel as `feat` on `vg.steps`) and `⋯` opens the sheet (`data-sheet` on the dash).
//
// The driver part answers for what the dash draws and hands every other target of the device or
// the computer to the core's driver of the Device controls popup.

import {
  closeList, closeOf, listen, openList,
} from '../../core/list.js';
import { ASK_MS, ackAsk, baseOf, noteReading } from '../../core/volume-ask.js';
import { drawDash, markCoverFailed, volumeWords } from './draw/dash.js';
import { PHONE_QUERY } from './hud.js';
import { drawCard } from './draw/card.js';
import {
  STATES, createDashFade, fadeAfter, heldCard, inertOf, moodOf, quietOf,
} from './states.js';

/** How long after the release of a press that only woke the dash its click may still come, ms. */
const CLICK_MS = 100;

/** How often the seconds on the card are counted down, ms. */
const TICK_MS = 1000;

/** The keys that work whatever the device: they reach the computer, the scene and the page. */
const FREE = ['side', 'explore', 'features', 'more', 'sheet-close'];

/** How far a swipe down on the sheet goes before it closes it, px. */
const SWIPE_PX = 40;

/** The attribute-selector form of `text`. */
const q = (text) => globalThis.CSS.escape(String(text));

/** Whether `element` takes room on the page. */
function taken(element) {
  if (!element?.isConnected) return false;
  if (typeof element.checkVisibility === 'function' && !element.checkVisibility()) return false;
  const box = element.getBoundingClientRect();
  return box.width > 0 && box.height > 0;
}

/**
 * Wire the dash and the page's states in `vg.page` (`#vg-dash`, `#vg-card`) to `vg.core` until
 * `vg.signal` aborts. `seams` hands the checks the clock (`now`, ms) and the fade's `schedule`.
 * Returns `{ driver, nudge(delta), writable(), onEnd(fn), fade }`: `nudge` is one volume step of
 * the side shown (the page's keys `↑` `↓` use it), `writable()` whether a held key may go on and
 * `onEnd(fn)` hears when what a hand holds must end (the side flipped, another device shown).
 */
export function wireDash(vg, { now, schedule } = {}) {
  const { core, page, signal } = vg;
  const store = core.model;
  const doc = page.ownerDocument;
  const win = doc.defaultView;
  const dash = page.querySelector('#vg-dash');
  const card = page.querySelector('#vg-card');
  dash.innerHTML = '<div class="vg-bar"></div>';
  const phoneQuery = win?.matchMedia?.(PHONE_QUERY) ?? null;
  const phone = () => Boolean(phoneQuery?.matches);
  const featOpen = () => page.dataset.feat === 'open';
  const sheetOpen = () => dash.dataset.sheet === 'open';
  card.innerHTML = '<p class="vg-line" role="status" hidden></p><div class="vg-cardslot"></div>';
  const bar = dash.firstElementChild;
  const line = card.querySelector('.vg-line');
  const slot = card.querySelector('.vg-cardslot');
  const view = { state: store.state(), capture: null, asked: null };
  let mood = 'ok';
  let out = false;
  let sigBar = null;
  let sigCard = '';
  let hold = null;
  let shown = store.shown();
  let fade = null;
  /** What hears that what a hand holds must end. */
  const ends = [];

  const volume = () => store.part('volume');
  const inert = () => inertOf(store.page());
  /** The page may repeat a held key: it may write and its stream is open. */
  const writable = () => !inert() && store.state() === 'open';
  const onComputer = () => core.computer.side() === 'computer';

  // ---- the page's state ----
  const sigOf = (html) => html.replace(/ data-secs="[^"]*" data-drawn="[^"]*"/, '');

  function count() {
    for (const el of slot.querySelectorAll('.vg-retry[data-secs]')) {
      const gone = (now() - Number(el.dataset.drawn)) / 1000;
      const left = Math.max(0, Math.ceil(Number(el.dataset.secs) - gone));
      const text = STATES.retryIn(left);
      if (el.textContent !== text) el.textContent = text;
    }
  }

  function drawScene(model) {
    mood = moodOf(model, view.state, view.capture);
    page.dataset.mood = mood;
    const held = mood === 'unreachable' ? heldCard(model) : null;
    const html = held ? drawCard(held, { quiet: inertOf(model), now: now() }) : '';
    if (sigOf(html) !== sigCard) {
      sigCard = sigOf(html);
      slot.innerHTML = html;
    } else if (held && typeof held.retry_in === 'number') {
      const el = slot.querySelector('.vg-retry');
      if (el) {
        el.dataset.secs = String(held.retry_in);
        el.dataset.drawn = String(now());
        count();
      }
    }
    const said = quietOf(mood, view.state, view.capture);
    if (line.textContent !== said) line.textContent = said;
    line.hidden = said === '';
    card.hidden = html === '' && said === '';
    // out of reach: the dash; lost: the dash too (the words and the frame stay)
    out = mood === 'lost' || mood === 'unreachable';
    putInert();
    fade?.hold(mood === 'lost' || mood === 'wait' || mood === 'unreachable');
  }

  /** Out of reach: every key of the device is out of reach but the side switch and the keys of the
   * scene and the page. */
  function putInert() {
    for (const el of bar.querySelectorAll('[data-act]')) {
      el.inert = out && !FREE.includes(el.dataset.act);
    }
  }

  /** The dash's height with its offset from the window's foot is the page's `--vg-dash-h`, which
   * the HUD and the panel keep clear of; it follows a wrap of the bar and a resize. */
  let height = '';
  function putHeight() {
    const px = `${Math.max(0, Math.round(win.innerHeight - dash.getBoundingClientRect().top))}px`;
    if (px === height) return;
    height = px;
    page.style.setProperty('--vg-dash-h', px);
  }

  function drawBar(model) {
    const html = drawDash(model, core.computer.view(), {
      phone: phone(), featOpen: featOpen(), sheetOpen: sheetOpen(),
    });
    if (html === sigBar) return;
    sigBar = html;
    const focus = bar.contains(doc.activeElement) ? doc.activeElement.id : '';
    bar.innerHTML = html;
    putInert();
    putHeight();
    if (focus) bar.querySelector(`#${q(focus)}`)?.focus({ preventScroll: true });
  }

  /** The volume's figure follows every reading, a held key's too: it is put right in place. */
  function drawFigure(model) {
    const v = volumeWords(model, core.computer.view());
    const el = bar.querySelector('#vg-vol');
    if (!v || !el) return;
    if (el.textContent !== v.text) el.textContent = v.text;
    el.setAttribute('aria-label', v.aria);
    el.classList.toggle('muted', Boolean(v.muted));
  }

  const live = core.freeze.createLive({
    core,
    signal,
    view,
    device: () => store.shown(),
    sections: {
      scene: { parts: ['card', 'link'], draw: drawScene },
      dash: { parts: ['link', 'card', 'volume', 'controls', 'power', 'playing'], draw: drawBar },
      figure: { parts: ['volume'], draw: drawFigure },
    },
  });
  const keep = core.freeze.keeper(live, 'dash');
  const release = core.freeze.createRelease(doc, keep, signal);

  // The music analysis's state (`no music analysis: <why>`): the scene is the page's one holder of
  // the shared stream and tells the capture's state and the letting go on `vg.steps`.
  vg.steps.addEventListener('capture', (event) => {
    const data = event.detail;
    const was = view.capture?.state;
    view.capture = { state: data.capture, why: data.why };
    if (was !== data.capture) live.draw('scene');
  }, { signal });
  vg.steps.addEventListener('release', () => {
    view.capture = null;
    live.draw('scene');
  }, { signal });

  // a window that becomes a phone, or stops being one, is drawn again with the other parts
  phoneQuery?.addEventListener?.('change', () => {
    if (!phone()) closeSheet();
    sigBar = null;
    live.draw('dash');
    fade.wake();
  }, { signal });
  win.addEventListener('resize', putHeight, { signal });
  if (typeof win.ResizeObserver === 'function') {
    const watching = new win.ResizeObserver(putHeight);
    watching.observe(bar);
    signal.addEventListener('abort', () => watching.disconnect(), { once: true });
  }

  // A cover the daemon no longer holds is not shown and leaves no gap.
  dash.addEventListener('error', (event) => {
    if (!event.target.matches?.('.vg-cover img')) return;
    event.target.closest('.vg-cover').hidden = true;
    markCoverFailed(new URL(event.target.src).searchParams.get('v'));
  }, { capture: true, signal });

  card.addEventListener('click', (event) => {
    if (!event.target.closest?.('[data-act="try"]')) return;
    Promise.resolve(core.actions.connect({ device: store.shown() })).catch(() => {});
  }, { signal });
  const timer = setInterval(() => {
    if (!doc.hidden) count();
  }, TICK_MS);
  signal.addEventListener('abort', () => clearInterval(timer), { once: true });

  // ---- the fade ----
  /** Something keeps the dash up: the keyboard focus on a key, a mouse resting over it, a hold. */
  let over = false;
  const focused = () => {
    try {
      return bar.querySelector(':focus-visible') !== null;
    } catch {
      return bar.contains(doc.activeElement);
    }
  };
  fade = createDashFade({
    after: () => fadeAfter(core.prefs, phone()),
    root: page,
    doc,
    now,
    ...(schedule ? { schedule } : {}),
    kept: () => live.frozen('dash') || focused() || over,
  });
  signal.addEventListener('abort', () => fade.stop(), { once: true });
  // the frame's own menu button fades with the dash, for the same time
  const fadeButton = () => core.frame.fade?.(true, fadeAfter(core.prefs, phone()) * 1000);
  fadeButton();
  phoneQuery?.addEventListener?.('change', fadeButton, { signal });
  signal.addEventListener('abort', () => core.frame.fade?.(false), { once: true });

  // The press that woke a faded dash goes no further, and neither does its click, whenever it
  // comes: the press is deaf from its start until just after its release.
  let deaf = false;
  let letGo = null;
  doc.addEventListener('pointerdown', () => { deaf = false; }, { capture: true, signal });
  dash.addEventListener('pointerdown', (event) => {
    if (!fade.woke(event)) return;
    deaf = true;
    event.stopPropagation();
    event.preventDefault();
  }, { capture: true, signal });
  fade.watch((kind, event) => {
    // a key press is no press of a pointer: it ends the deaf time
    if (kind === 'wake' && event && 'key' in event) deaf = false;
  });
  const released = () => {
    if (!deaf) return;
    clearTimeout(letGo);
    letGo = setTimeout(() => { deaf = false; }, CLICK_MS);
  };
  doc.addEventListener('pointerup', released, { capture: true, signal });
  doc.addEventListener('pointercancel', released, { capture: true, signal });
  dash.addEventListener('click', (event) => {
    if (!deaf) return;
    event.stopPropagation();
    event.preventDefault();
  }, { capture: true, signal });
  dash.addEventListener('pointerenter', (event) => {
    over = event.pointerType === 'mouse';
  }, { signal });
  dash.addEventListener('pointerleave', () => { over = false; }, { signal });
  signal.addEventListener('abort', () => clearTimeout(letGo), { once: true });

  // ---- the phone's sheet and features chip ----
  function setSheet(on) {
    if (on === sheetOpen()) return;
    if (on) {
      dash.dataset.sheet = 'open';
      page.dataset.sheet = 'open';
    } else {
      delete dash.dataset.sheet;
      delete page.dataset.sheet;
    }
    dash.querySelector('#vg-more')?.setAttribute('aria-expanded', String(on));
    if (!on && bar.contains(doc.activeElement) && dash.querySelector('#vg-sheet')?.contains(doc.activeElement)) {
      dash.querySelector('#vg-more')?.focus({ preventScroll: true });
    }
    putHeight();
  }
  function closeSheet() {
    setSheet(false);
  }
  /** The features panel's state on a phone: the chip's label and the panel's own. */
  const setFeatures = (on) => {
    if (on) page.dataset.feat = 'open';
    else delete page.dataset.feat;
    dash.querySelector('#vg-fchip')?.setAttribute('aria-expanded', String(on));
    dash.querySelector('#vg-fchip')?.classList.toggle('vg-gone', on);
    vg.steps.dispatchEvent(new globalThis.CustomEvent('feat', { detail: { open: on } }));
  };
  vg.steps.addEventListener('feat', (event) => {
    if (event.detail.open !== featOpen()) setFeatures(event.detail.open);
  }, { signal });
  // a swipe down on the sheet closes it
  let swipe = null;
  dash.addEventListener('pointerdown', (event) => {
    swipe = event.target.closest?.('#vg-sheet') && event.isPrimary ? { y: event.clientY } : null;
  }, { signal });
  dash.addEventListener('pointermove', (event) => {
    if (swipe && event.clientY - swipe.y > SWIPE_PX) {
      swipe = null;
      closeSheet();
    }
  }, { signal });
  doc.addEventListener('pointerup', () => { swipe = null; }, { capture: true, signal });
  doc.addEventListener('pointercancel', () => { swipe = null; }, { capture: true, signal });
  // Escape, from a key of the dash (where the focus is once the sheet is open)
  dash.addEventListener('keydown', (event) => {
    if (event.key === 'Escape' && sheetOpen()) closeSheet();
  }, { signal });

  // ---- the volume ----
  const base = (v, delta) => baseOf(v, { asked: view.asked, now: now() }, delta);

  async function send(step, target) {
    const mine = view.asked;
    const answer = await core.actions.setVolume(step, { device: target });
    if (signal.aborted || answer?.queued || answer?.dropped) return;
    if (view.asked !== mine || !mine) return;
    if (!answer?.ok) {
      view.asked = null;
      return;
    }
    // The daemon may land the write on another step (the quietest, under a cap below every step).
    const landed = answer.landed?.raw;
    if (typeof landed === 'number' && landed !== mine.raw) {
      view.asked = { raw: landed, at: now(), read: mine.read, seen: mine.read === landed };
    }
    if (view.asked) ackAsk(view, view.asked);
  }

  /** Ask for step `raw` of `target`: held to the range and the cap, noted in `view.asked`, sent.
   * The step already asked for stands while it is fresh and the device has not read another
   * since. */
  function ask(raw, target) {
    const v = volume();
    if (!v || v.raw === null || v.raw === undefined) return;
    let step = Math.min(v.hi, Math.max(v.lo, Math.round(raw)));
    if (typeof v.cap === 'number' && step > v.cap) step = v.cap;
    if (inert()) {
      core.actions.setVolume(step, { device: target })?.catch?.(() => {});
      return;
    }
    const climbing = v.target !== null && v.target !== undefined && v.target > v.raw;
    const again = (view.asked?.raw === step && now() - view.asked.at < ASK_MS
      && (!view.asked.answered || v.raw === step))
      || (!view.asked && step === v.raw && !climbing);
    if (again) return;
    view.asked = { raw: step, at: now() };
    send(step, target).catch(() => {});
  }

  /** One volume step of the shown side: the computer's, or the shown device's. A viewer who may
   * not write is told so by the core, which turns the call away before it looks at the step. */
  function nudge(delta, target = store.shown()) {
    if (onComputer()) {
      core.actions.computer.step(delta)?.catch?.(() => {});
      return;
    }
    if (!core.actions.canWrite()) {
      core.actions.setVolume(0, { device: target })?.catch?.(() => {});
      return;
    }
    const v = volume();
    if (v && v.raw !== null && v.raw !== undefined) ask(base(v, delta) + delta, target);
  }

  // A reading that is not the answer to the step asked is another hand's: the next step starts
  // from it.
  signal.addEventListener('abort', store.on('volume', (data) => {
    if ((data.device ?? store.shown()) !== store.shown()) return;
    const a = view.asked;
    if (a?.acked && data.raw !== undefined && data.raw !== a.raw) view.asked = null;
    else noteReading(view, data.raw);
  }), { once: true });

  // Another device shown ends what was asked of the old one: a hold, a list, the step asked.
  signal.addEventListener('abort', store.on('model', () => {
    if (store.shown() === shown) return;
    shown = store.shown();
    view.asked = null;
    hold?.end();
    ends.forEach((fn) => fn());
    closeList();
  }), { once: true });

  // A held `−` or `+` is the core's repeat: one step at the press and, after a pause, steps at a
  // steady pace for at most the core's cap. The bar is kept as it is while a key is down, so a
  // reading does not replace the key under the finger.
  const STEPS = '[data-act="volume-down"], [data-act="volume-up"]';
  dash.addEventListener('pointerdown', (event) => {
    const el = event.target.closest?.(STEPS);
    if (!el || !event.isPrimary || (event.pointerType === 'mouse' && event.button !== 0)) return;
    hold?.end();
    const target = store.shown();
    hold = core.repeat(event, {
      root: dash,
      el,
      step: (at) => nudge(at.dataset.act === 'volume-up' ? 1 : -1, target),
      live: writable,
      leave: true,
      onStart: () => keep.take(),
      onEnd: () => {
        hold = null;
        keep.drop();
      },
      signal,
    });
  }, { signal });
  dash.addEventListener('contextmenu', (event) => {
    if (event.target.closest?.(STEPS)) event.preventDefault();
  }, { signal });

  // ---- the other keys ----
  const write = (call) => {
    try {
      Promise.resolve(call(core.actions)).catch(() => {});
    } catch {
      // a call the core refuses by throwing is its to say
    }
  };
  const setInput = (option) => write((a) => a.setControl('input', JSON.parse(option.json)));

  function openInputList(el, c) {
    if (inert()) {
      const cur = c.options?.find((o) => o.on) ?? c.options?.[0];
      if (cur) setInput(cur);
      return;
    }
    if (closeOf(el)) return;
    const list = keep.around(() => openList({
      opener: el,
      label: c.label ?? 'Input',
      options: (c.options ?? []).map((o) => ({ value: o.label, text: o.label, on: o.on })),
      choose(label) {
        const option = c.options.find((o) => o.label === label);
        if (option) setInput(option);
      },
      signal,
      onClose: release,
      root: '#voidgarden',
      place: 'auto',
    }));
    listen(list);
  }

  dash.addEventListener('click', (event) => {
    const el = event.target.closest?.('[data-act]');
    if (!el) return;
    const what = el.dataset.act;
    if (el.matches(STEPS)) {
      // a press of the pointer stepped already; a click with no pointer is a key: one step
      if (event.detail === 0) nudge(what === 'volume-up' ? 1 : -1);
      return;
    }
    if (what === 'explore') {
      vg.steps.dispatchEvent(new globalThis.CustomEvent('explore', { detail: { kind: el.dataset.kind } }));
    } else if (what === 'features') {
      setFeatures(!featOpen());
    } else if (what === 'more') {
      setSheet(!sheetOpen());
    } else if (what === 'sheet-close') {
      closeSheet();
    } else if (what === 'side') {
      core.computer.flip(el.dataset.side);
    } else if (what === 'mute') {
      if (onComputer()) {
        const mine = core.computer.volumeOf(store.page());
        if (!mine.inert) write((a) => a.computer.setMute(!mine.muted));
      } else if (volume()) {
        write((a) => a.setControl(el.dataset.key, { value: !volume().muted }));
      }
    } else if (what === 'input') {
      const controls = store.page().device?.controls ?? [];
      const c = controls.find((x) => x.key === 'input' && x.kind === 'choice');
      if (!c) return;
      if (el.dataset.list) {
        openInputList(el, c);
      } else {
        const option = c.options?.find((o) => o.label === el.dataset.value);
        if (option) setInput(option);
      }
    } else if (what === 'prev' || what === 'play' || what === 'next') {
      write((a) => a.press(el.dataset.button));
    }
  }, { signal });

  signal.addEventListener('abort', store.on('state', (state) => {
    view.state = state;
  }), { once: true });
  signal.addEventListener('abort', core.computer.on((event) => {
    if (event.type === 'flip') {
      hold?.end();
      ends.forEach((fn) => fn());
    }
  }), { once: true });
  signal.addEventListener('abort', () => {
    hold?.end();
    closeList();
  }, { once: true });

  live.drawAll();

  // ---- the driver ----
  /** The element a target is, as the dash draws it, or nothing. */
  function find(target) {
    const one = (selector) => dash.querySelector(selector);
    if (target === 'Cover') return one('.vg-cover img');
    if (typeof target !== 'object' || target === null) return null;
    if ('computer' in target) {
      switch (target.computer) {
        case 'flip': return one('#vg-side-computer');
        case 'volume': return onComputer() ? one('[data-act="volume-up"]') : null;
        case 'mute': return onComputer() ? one('[data-act="mute"]') : null;
        default: return null;
      }
    }
    if ('transport' in target) return one(`[data-button="${q(target.transport)}"]`);
    if (!('control' in target)) return null;
    const dev = store.device();
    if (dev?.volume && target.control === dev.volume.key) return one('[data-act="volume-up"]');
    if (dev?.volume && target.control === dev.volume.mute) return one('[data-act="mute"]');
    if (target.control === 'input') {
      return one('[data-act="input"][data-list]') ?? one('[data-act="input"]');
    }
    return null;
  }

  const settle = () => new Promise((resolve) => {
    const frame = (fn) => (win.requestAnimationFrame
      ? win.requestAnimationFrame(fn) : win.setTimeout(fn, 16));
    frame(() => win.setTimeout(resolve, 30));
  });

  /** Close the popup that stands over the page, as a person does before using the dash. */
  async function clear() {
    if (!core.router.state().popup) return;
    core.router.close();
    for (let n = 0; n < 40 && core.router.state().popup; n++) await later(25);
    await settle();
  }

  /** The phone keeps most keys in the sheet: a key that stands in the closed sheet opens it first. */
  async function openSheetFor(el) {
    if (el && phone() && !sheetOpen() && el.closest('#vg-sheet')) {
      setSheet(true);
      await settle();
    }
  }

  const tap = (el) => el.click();
  const later = (ms) => new Promise((resolve) => { win.setTimeout(resolve, ms); });
  const pct = () => core.computer.volumeOf(store.page())?.pct ?? null;

  async function volumeTo(read, raw) {
    for (let n = 0; n < 120; n++) {
      const at = read();
      if (at === raw || typeof at !== 'number') return;
      tap(dash.querySelector(`[data-act="volume-${raw > at ? 'up' : 'down'}"]`));
      for (let wait = 0; wait < 40 && read() === at; wait++) await later(25);
      // a step that cannot move it (the cap) is a quiet stop: the check judges by what was sent
      if (read() === at) return;
    }
  }

  const driver = {
    async reach(target) {
      if (!find(target) && !(typeof target === 'object' && target && 'computer' in target
        && target.computer === 'flip')) {
        return core.driver.controls.reach(target);
      }
      await clear();
      fade.wake();
      await openSheetFor(find(target));
      await settle();
      const el = find(target);
      return Boolean(el) && taken(el);
    },

    async operate(target, change = {}) {
      const el = find(target);
      if (!el) return core.driver.controls.operate(target, change);
      await clear();
      fade.wake();
      await openSheetFor(el);
      if (target === 'Cover') return false;
      if ('computer' in target) {
        if (target.computer === 'flip') {
          tap(dash.querySelector(`#vg-side-${change.side === 'computer' ? 'computer' : 'dac'}`));
        }
        else if (target.computer === 'volume') {
          if ('raw' in change) await volumeTo(pct, change.raw);
          else tap(dash.querySelector(`[data-act="volume-${change.step < 0 ? 'down' : 'up'}"]`));
        } else if (!('on' in change)
          || (el.getAttribute('aria-pressed') === 'true') !== change.on) {
          tap(el);
        }
      } else if ('transport' in target) {
        tap(el);
      } else if (el.dataset.act === 'volume-up') {
        if ('raw' in change) await volumeTo(() => store.device()?.volume?.raw, change.raw);
        else tap(dash.querySelector(`[data-act="volume-${change.step < 0 ? 'down' : 'up'}"]`));
      } else if (el.dataset.act === 'mute') {
        if (!('on' in change) || Boolean(store.device()?.volume?.muted) !== change.on) tap(el);
      } else if (el.dataset.act === 'input') {
        if (el.dataset.list) {
          tap(el);
          await settle();
          const option = doc.querySelector(`.fi-list .fi-opt[data-value="${q(change.value)}"]`);
          if (option) tap(option);
          else if (!store.device()?.inert) throw new Error(`no option ${change.value}`);
        } else {
          tap(dash.querySelector(`[data-act="input"][data-value="${q(change.value)}"]`) ?? el);
        }
      }
      await settle();
      return true;
    },

    shown(target) {
      const el = find(target);
      if (!el) return core.driver.controls.shown(target);
      if (target === 'Cover') return el.getAttribute('src');
      if (typeof target === 'object' && 'computer' in target) {
        if (target.computer === 'flip') {
          return dash.querySelector('.vg-swkeys [aria-pressed="true"]')?.textContent ?? null;
        }
        if (target.computer === 'volume') return dash.querySelector('#vg-vol')?.textContent ?? null;
        return el.getAttribute('aria-pressed') === 'true' ? 'on' : 'off';
      }
      if ('transport' in target) return el.getAttribute('aria-label');
      if (el.dataset.act === 'volume-up') return dash.querySelector('#vg-vol')?.textContent ?? null;
      if (el.dataset.act === 'mute') {
        return el.getAttribute('aria-pressed') === 'true' ? 'on' : 'off';
      }
      const now = dash.querySelector('[data-act="input"].on') ?? el;
      return now.textContent.replace(/ ▾$/, '');
    },

    state: () => ({
      lost: Boolean(slot.querySelector('#vg-cardbox')) || mood === 'lost'
        || store.state() === 'lost',
      waiting: store.state() === 'waiting',
      resting: store.state() === 'resting',
      toasts: [...doc.querySelectorAll('#toasts .toast')]
        .filter((box) => !box.dataset.leaving).map((box) => box.dataset.text),
    }),
  };

  return {
    driver, nudge, writable, onEnd: (fn) => ends.push(fn), fade,
  };
}
