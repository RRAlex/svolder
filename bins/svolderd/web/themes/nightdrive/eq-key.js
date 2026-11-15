/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The dash's EQ key: the EQ of the side shown, on or off, from the main view. The computer's EQ is
// on or off; a device's is EQ or Flat (what the key turns off is the curve: Loudness reads as off
// and a press brings EQ back). The key's state is the model's (the computer's with the mode this
// viewer is changing laid over it, the device's tone mode), so the stream's event and the write's
// answer can come in either order. Every write goes through `core.actions`.

/** What the key stands for on `page`: `{ side, on, mode }` (`mode` is the mode a press asks for), or
 * `null` where there is no EQ to turn off (no tone, or a tone with no Flat and EQ). `comp` is the
 * core's `computer.view()`. */
export function eqKeyOf(page, comp = null) {
  if (comp?.side === 'computer') {
    return comp.eq ? { side: 'computer', on: comp.eq.on === true, mode: comp.eq.on === true ? 'off' : 'eq' } : null;
  }
  const tone = page?.device?.tone;
  const names = (tone?.modes ?? []).map((m) => m.name);
  if (!names.includes('eq') || !names.includes('flat')) return null;
  const on = tone.mode === 'eq';
  return { side: 'device', on, mode: on ? 'flat' : 'eq' };
}

/**
 * The press of the key: writes the other mode of the side shown through the log. A viewer who may
 * not write is turned away by the core, which says why (the mode asked is the one it is in).
 * Returns a promise of the write's answer, `null` where the key has nothing to do.
 */
export function pressEqKey({ core, act, inert }) {
  const store = core.model;
  const key = eqKeyOf(store.page(), core.computer.view());
  if (!key) return Promise.resolve(null);
  if (key.side === 'computer') {
    return act.act('mode', key.mode, (a) => a.computer.setMode(key.mode));
  }
  if (inert) {
    core.actions.setMode(store.part('tone')?.mode);
    return Promise.resolve(null);
  }
  core.toneNote?.clicked(key.mode);
  return act.act('mode', key.mode, (a) => a.setMode(key.mode, { device: store.shown() }));
}
