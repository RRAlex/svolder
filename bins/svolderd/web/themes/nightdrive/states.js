/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What state the page is in, as the scene and the HUD show it: the mood of the page and the quiet
// line that says what is not as usual. Pure: the model, the stream's state and the capture's state
// in; words out.

import { STATES } from './words.js';

/** The card the shown device has, or the page's when no device is shown; `null` when none. */
export const heldCard = (page) => page?.device?.card ?? page?.card ?? null;

/**
 * How the page stands, for the scene and the dash:
 * `lost` the stream to the daemon is lost; `wait` the page reads the device again or rests;
 * `unreachable` the device (or the page) has a card that is not a fault; `nocap` the music cannot
 * be analysed (`capture.state` is `unavailable`); else `ok`. A fault on a connected device is `ok`
 * (the top line goes red, the controls stay live), and so is a guest (its controls are drawn quiet).
 * `capture` is `{ state, why }` as the analysis said it, or `null` before it has said anything.
 */
export function moodOf(page, state, capture) {
  if (state === 'lost') return 'lost';
  if (state === 'waiting' || state === 'resting') return 'wait';
  const card = heldCard(page);
  if (card && card.kind !== 'fault') return 'unreachable';
  if (capture?.state === 'unavailable') return 'nocap';
  return 'ok';
}

/** Whether the scene is drawn dull (greys, nothing moving): the device is out of reach, or the page is lost or waiting. */
export const dullMood = (mood) => mood === 'unreachable' || mood === 'lost' || mood === 'wait';

/** Whether the mountains lie flat: the device is out of reach, or the stream is lost. */
export const flatMood = (mood) => mood === 'unreachable' || mood === 'lost';

/** The quiet line of a mood: what is not as usual, in a few words; empty when nothing is. */
export function quietOf(mood, state, capture) {
  if (mood === 'lost') return STATES.reconnecting;
  if (mood === 'wait') return state === 'resting' ? STATES.restingLine : STATES.waitingLine;
  if (mood === 'nocap') return STATES.noCapture(capture?.why);
  return '';
}
