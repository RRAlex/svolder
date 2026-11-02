/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// A slider's writes: one in flight, the newest waiting, each for the device that was shown when
// the value was asked for. A value that waits for the write in flight is dropped when its slider
// has left the page or another device is shown by then, so it never reaches a device the person
// did not ask it of. Plain callbacks, no DOM: node drives it too.

import { newest } from '../../core/api.js';

/**
 * The writer of the level `key`: `ask(raw)` writes through `actions.setControl`. `shown()` names
 * the device on the page, `connected()` whether the slider is, `landed(answer)` hears an
 * answer that carries the step the device took (for the device still shown) and `refused(words)`
 * a write the core has said why it was refused.
 */
export function createLevelWriter({ key, actions, shown, connected, landed, refused }) {
  const send = newest(async ({ device, raw }) => {
    if (!connected() || shown() !== device) return null;
    const answer = await actions.setControl(key, { raw }, { device });
    if (!answer.ok) return answer.error ?? 'The change was not made.';
    if (answer.raw !== undefined && shown() === device && connected()) landed(answer);
    return null;
  }, refused);
  return (raw) => send({ device: shown(), raw });
}
