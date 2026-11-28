/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Device controls popup's wire: thin. It draws the controls of the shown device by kind (draw.js)
// into the popup's frame, or the computer's when the person flipped to it (a flip writes nothing:
// `core.computer.flip`), and every tap goes through `core.actions` (`core.actions.computer` for the
// computer's side) to the device the popup is about, which is the one shown when it was opened: the
// popup closes with a change of the shown device. A viewer who may not write sees every control
// inert, and a tap on one says the core's words (`core.actions.refuse()`); a refused write shows the
// daemon's words in a line of the popup.
//
// The sections are put alone (`frame.put`) when the model changes, so a control keeps its element and
// the focus; the whole page is drawn again only when the set of sections changes (a side flipped, a
// mode that has a strength). While an ask stands in a section, or a point's field holds what was
// typed, or a stepper is held, the model's changes wait: they are drawn once the hand is away. A
// volume or a level stepped again before the model has answered the last step goes on from the step
// asked, not from the stale reading. The wire attaches no key listener: the stack hands the keys to
// `onKey`.

import { askForm, askQuestion } from '../../inline-ask.js';
import { SOUND } from '../listed.js';
import {
  IDS, WORDS, pageHtml, sayHtml, sectionsOf, sideHtml,
} from './draw.js';

/** The popup's part for the router: `{ show(page), hide(), onKey(event) }`, over `core`, drawn into
 * `frame` (popups/frame.js). */
export function wire({ core, frame }) {
  const doc = frame.el.ownerDocument;
  const store = core.model;
  /** One per opening: what a held stepper and an ask stand under. */
  let aborter = new AbortController();
  let open = false;
  /** How many times the popup was opened: an answer that comes after a close is not shown. */
  let opening = 0;
  let offs = [];
  let said = null;
  /** The ask that stands (one at a time). */
  let asking = null;
  /** How many steppers are held, and whether the model changed meanwhile or under an ask. */
  let holding = 0;
  let stale = false;
  /** The ids of the sections drawn, and the side they were drawn for. */
  let drawn = [];
  /** The step asked of a control before the model has shown it: `{ from, raw }` by control. */
  const asked = new Map();

  /** Everything a draw needs, read from the model now. */
  function modelOf() {
    const page = store.page();
    const url = store.shown();
    const device = store.device(url);
    const view = core.computer.view(page);
    const side = view?.side ?? 'dac';
    const on = side === 'computer';
    const computer = view ? {
      sw: view.sw,
      volume: view.volume,
      tone: view.tone,
      levels: on ? core.computer.levelsOf(page) : null,
      bypass: on ? core.computer.bypassOf(page) : null,
    } : null;
    return {
      device, computer, side, inert: !core.actions.canWrite() || Boolean(device?.inert),
      writer: core.actions.canWrite(),
    };
  }

  const draw = () => {
    const model = modelOf();
    drawn = sectionsOf(model).map((one) => one.id);
    frame.draw({ title: WORDS.title, body: pageHtml(model, said) });
  };

  /** Whether the hand is on something a redraw would take from it: an ask, a held stepper, or a
   * point's field with what was typed and not sent. */
  function busy() {
    if (asking || holding > 0) return true;
    const at = doc.activeElement;
    return Boolean(at?.matches?.('input[data-act="c-point"]') && frame.el.contains(at)
      && at.value !== at.defaultValue);
  }

  /** The figures of a held stepper put into the page as they are, the sections as they were. */
  function patchFigures() {
    const model = modelOf();
    const volume = model.device?.volume;
    const figure = frame.el.querySelector('[data-shown="volume"]');
    if (volume && figure) figure.textContent = volume.muted ? WORDS.muted : (volume.text ?? '');
    const computer = model.computer?.volume;
    const level = frame.el.querySelector('[data-shown="c-volume"]');
    if (computer && level) {
      level.textContent = computer.muted ? WORDS.muted
        : (computer.pct === null ? computer.text : `${computer.text} ${computer.unit}`);
    }
  }

  /** Draw what changed, each section in its place; the whole page when the sections are others. */
  function refresh() {
    if (!open) return;
    if (holding > 0) {
      stale = true;
      patchFigures();
      return;
    }
    if (busy()) {
      stale = true;
      return;
    }
    stale = false;
    const model = modelOf();
    const sections = sectionsOf(model);
    if (sections.map((one) => one.id).join() !== drawn.join()) {
      draw();
      return;
    }
    frame.put(IDS.side, sideHtml(model.computer, model.side, model.device?.name));
    for (const one of sections) frame.put(one.id, one.html);
  }

  const later = () => {
    if (stale) refresh();
  };

  const say = (next) => {
    said = next;
    if (open) frame.put(IDS.say, sayHtml(said));
  };

  /** A tap that cannot write: the core says why (a toast) and the popup keeps the words in place. */
  function refuseTap() {
    const away = core.actions.refuse();
    if (away?.error) say({ text: away.error, failed: true });
  }

  /** What an answer to a write says: a refusal's words in place, a landed write clears them. A
   * dropped or declined write has none. */
  async function write(call) {
    const mine = opening;
    let answer;
    try {
      answer = await call;
    } catch {
      answer = { ok: false };
    }
    if (!open || mine !== opening) return answer;
    if (answer?.ok === false && typeof answer.error === 'string' && answer.error) {
      say({ text: answer.error, failed: true });
    } else if (answer?.ok === true && said) say(null);
    return answer;
  }

  /** The value a step starts from: the one asked of `key` while the model has not moved since, else
   * the model's. */
  function base(key, current) {
    const held = asked.get(key);
    return held && held.from === current ? held.raw : current;
  }

  /** One step of the shown device's volume. */
  function stepVolume(dir) {
    const device = store.shown();
    const volume = store.device(device)?.volume;
    if (!volume || typeof volume.raw !== 'number') return;
    let next = Math.max(volume.lo, Math.min(volume.hi, base('volume', volume.raw) + dir));
    if (typeof volume.cap === 'number') next = Math.min(next, volume.cap);
    asked.set('volume', { from: volume.raw, raw: next });
    write(core.actions.setVolume(next, { device })).then((answer) => {
      if (answer?.ok === false && !answer.dropped) asked.delete('volume');
    });
  }

  /** One step of the slider `key` of the shown device. */
  function stepSlider(key, dir) {
    const device = store.shown();
    const control = (store.device(device)?.controls ?? []).find((one) => one.key === key);
    if (!control || typeof control.raw !== 'number') return;
    const next = Math.max(control.lo, Math.min(control.hi, base(key, control.raw) + dir));
    asked.set(key, { from: control.raw, raw: next });
    write(core.actions.setControl(key, { raw: next }, { device })).then((answer) => {
      if (answer?.ok === false) asked.delete(key);
    });
  }

  /** The body a control of the model sends (a toggle's or an option's `json`), or null. */
  function bodyOf(json) {
    try {
      return JSON.parse(json);
    } catch {
      return null;
    }
  }

  /** One step of the volume stepper `el` (the device's, or the computer's). */
  function stepOf(el) {
    const dir = el.dataset.step === '-1' ? -1 : 1;
    if (el.dataset.act === 'volume-down' || el.dataset.act === 'volume-up') stepVolume(dir);
    else if (el.dataset.act === 'c-volume-down' || el.dataset.act === 'c-volume-up') {
      write(core.actions.computer.step(dir));
    }
  }

  /** Begin an ask: `make(onClose)` draws it and returns its handle, which is kept to hand a key to
   * until it closes; what waited for it is drawn then. */
  function ask(make) {
    let mine = null;
    const onClose = () => {
      if (asking !== mine) return;
      asking = null;
      later();
    };
    mine = make(onClose);
    asking = mine;
  }

  const answers = { newName: WORDS.newName, presetName: WORDS.presetName, answers: WORDS.answers };

  function askSave(el) {
    const device = store.shown();
    asking?.close(false);
    ask((onClose) => askForm({
      within: doc.getElementById(IDS.ask), kind: 'save', value: '', words: answers,
      signal: aborter.signal, opener: el, onClose,
      submit: (text) => write(core.actions.savePreset(text, { device })),
    }));
  }

  function askRename(el) {
    const device = store.shown();
    const from = el.dataset.name;
    asking?.close(false);
    ask((onClose) => askForm({
      within: doc.getElementById(IDS.ask), kind: 'rename', value: from, words: answers,
      signal: aborter.signal, opener: el, onClose,
      submit: (to) => write(core.actions.renamePreset(from, to, { device })),
    }));
  }

  function askDelete(el) {
    const device = store.shown();
    const name = el.dataset.name;
    asking?.close(false);
    ask((onClose) => askQuestion({
      within: doc.getElementById(IDS.ask), text: WORDS.askDelete(name), yes: 'delete', no: 'keep',
      words: answers, signal: aborter.signal, opener: el, onClose,
      go: () => write(core.actions.deletePreset(name, { device })),
    }));
  }

  function askSlot(el) {
    const device = store.shown();
    const slot = Number(el.dataset.slot);
    asking?.close(false);
    ask((onClose) => askQuestion({
      within: doc.getElementById(IDS.cask), text: WORDS.askSlot(el.dataset.name), yes: 'save',
      no: 'keep', words: answers, signal: aborter.signal, opener: el, onClose,
      go: () => write(core.actions.saveSlot(slot, true, { device })),
    }));
  }

  /** What a tap on the control `el` does. */
  function act(el) {
    const device = store.shown();
    const model = store.device(device);
    const { act: what } = el.dataset;
    if (what === 'side') {
      if (el.getAttribute('aria-disabled') !== 'true') core.computer.flip(el.dataset.side);
      return;
    }
    if (what === 'sound') {
      core.router.open(SOUND);
      return;
    }
    // a control that cannot be used: for a viewer who may not write the core says why
    if (el.getAttribute('aria-disabled') === 'true') {
      refuseTap();
      return;
    }
    if (!model) return;
    switch (what) {
      case 'volume-down': case 'volume-up': case 'c-volume-down': case 'c-volume-up':
        stepOf(el);
        break;
      case 'mute':
        write(core.actions.setControl(el.dataset.key, { value: !model.volume?.muted }, { device }));
        break;
      case 'c-mute':
        write(core.actions.computer.toggleMute());
        break;
      case 'mode':
        write(core.actions.setMode(el.dataset.mode, { device }));
        break;
      case 'loudness':
        write(core.actions.setLoudness({ step: Number(el.dataset.step) }, { device }));
        break;
      case 'band':
        write(core.actions.setBand(Number(el.dataset.band), el.dataset.param,
          { step: Number(el.dataset.step) }, { device }));
        break;
      case 'preset':
        write(core.actions.recallPreset(el.dataset.name, { device }));
        break;
      case 'preset-save': askSave(el); break;
      case 'preset-rename': askRename(el); break;
      case 'preset-delete': askDelete(el); break;
      case 'slot':
        write(core.actions.recallSlot(Number(el.dataset.slot), { device }));
        break;
      case 'slot-save': askSlot(el); break;
      case 'setting':
        write(core.actions.applySetting(el.dataset.name, { device }));
        break;
      case 'choice': {
        const control = (model.controls ?? []).find((one) => one.key === el.dataset.key);
        const option = control?.options?.find((one) => one.label === el.dataset.value);
        const body = option ? bodyOf(option.json) : null;
        if (body) write(core.actions.setControl(control.key, body, { device }));
        break;
      }
      case 'toggle': {
        const control = (model.controls ?? []).find((one) => one.key === el.dataset.key);
        const body = control ? bodyOf(control.json) : null;
        if (body) write(core.actions.setControl(control.key, body, { device }));
        break;
      }
      case 'power': {
        const body = model.power ? bodyOf(model.power.json) : null;
        if (body) write(core.actions.setControl(model.power.key, body, { device }));
        break;
      }
      case 'power-toggle': {
        const toggle = (model.power?.toggles ?? []).find((one) => one.key === el.dataset.key);
        const body = toggle ? bodyOf(toggle.json) : null;
        if (body) write(core.actions.setControl(toggle.key, body, { device }));
        break;
      }
      case 'slider':
        stepSlider(el.dataset.key, el.dataset.step === '-1' ? -1 : 1);
        break;
      case 'transport':
        write(core.actions.press(el.dataset.button));
        break;
      case 'c-mode':
        write(core.actions.computer.setMode(el.dataset.mode));
        break;
      case 'c-bypass': {
        const bypass = core.computer.bypassOf(store.page());
        if (bypass) write(core.actions.computer.setBypass(!bypass.on));
        break;
      }
      case 'c-add':
        write(core.actions.computer.addNext());
        break;
      case 'c-remove':
        write(core.actions.computer.removePoint(Number(el.dataset.n)));
        break;
      case 'c-preset':
        write(core.actions.computer.recallPreset(el.dataset.name, { device }));
        break;
      default:
        break;
    }
  }

  /** A point's field: Enter sends what was typed (rounded and held to the hand range by the core) through
   * `core.eq.table.editCell`, which puts the old text back at once; the model's words replace it. A flow
   * lasts one Enter or one leaving of the field, and is made again at the next key. */
  const flows = new WeakMap();
  function flowOf(field) {
    let flow = flows.get(field);
    if (!flow) {
      const shown = field.defaultValue;
      flow = core.eq.table.editCell(field, {
        n: Number(field.dataset.n),
        param: field.dataset.param,
        shown,
        cell: async (n, param, typed, was) => {
          if (!core.actions.canWrite()) {
            refuseTap();
            return;
          }
          await write(core.actions.computer.cell(n, param, typed, was));
        },
        restore: () => { field.value = shown; },
        after: () => {
          flows.delete(field);
          later();
        },
      });
      flows.set(field, flow);
    }
    return flow;
  }

  const pointer = (event) => {
    const el = event.target.closest?.('[data-repeat]');
    if (!open || !el || !frame.el.contains(el)) return;
    if (el.getAttribute('aria-disabled') === 'true') return;
    if (event.pointerType === 'mouse' && event.button !== 0) return;
    core.repeat(event, {
      root: frame.el,
      step: (button) => stepOf(button),
      live: () => open && core.actions.canWrite(),
      signal: aborter.signal,
      onStart: () => { holding += 1; },
      onEnd: () => {
        holding = Math.max(0, holding - 1);
        if (holding === 0) later();
      },
    });
  };

  frame.el.addEventListener('pointerdown', pointer);

  frame.el.addEventListener('click', (event) => {
    if (!open) return;
    const el = event.target.closest?.('button[data-act]');
    if (!el || !frame.el.contains(el)) return;
    // a press of a stepper already stepped it (the pointer's repeat); a click with no pointer
    // (a key, a screen reader) steps once
    if (el.dataset.repeat && event.detail > 0 && el.getAttribute('aria-disabled') !== 'true') return;
    act(el);
  });

  frame.el.addEventListener('change', (event) => {
    const el = event.target;
    if (!open || !el?.matches?.('select[data-act="c-point"]')) return;
    if (!core.actions.canWrite()) {
      refuseTap();
      return;
    }
    write(core.actions.computer.setPoint(Number(el.dataset.n), { type: el.value }, { done: true }));
  });

  // a flow is made when the field takes the focus, so leaving it puts back what was not sent
  frame.el.addEventListener('focusin', (event) => {
    const field = event.target;
    if (open && field?.matches?.('input[data-act="c-point"]')) flowOf(field);
  });

  return {
    show() {
      if (!open) {
        open = true;
        opening += 1;
        aborter = new AbortController();
        said = null;
        asking = null;
        holding = 0;
        stale = false;
        asked.clear();
        offs = [
          store.on('model', refresh), store.on('device', refresh), store.on('part', refresh),
          store.on('volume', refresh), store.on('host', refresh), store.on('state', refresh),
          core.computer.on(refresh),
        ];
      }
      draw();
    },
    hide() {
      if (!open) return;
      open = false;
      aborter.abort();
      asking?.close(false);
      asking = null;
      holding = 0;
      for (const off of offs) off();
      offs = [];
      said = null;
    },
    onKey(event) {
      if (asking?.key(event) === true) return true;
      const field = event.target?.closest?.('input[data-act="c-point"]');
      if (!field || !frame.el.contains(field)) return false;
      if (event.key === 'Enter') return flowOf(field).key(event);
      if (event.key === 'Escape' && field.value !== field.defaultValue) return flowOf(field).key(event);
      return false;
    },
  };
}
