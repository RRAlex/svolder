/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The EQ library popup's saved-presets page, as a part of the wire (wire.js): the list of the
// side's presets, why a curve cannot be saved on a device, the overwrite question and its words.
// The wire gives it the popup's state through `here` (whether the popup stands, the count of
// openings, the side, the abort controller of the opening), the draws' `put` and the lookup of a
// fleet row.

import { askQuestion } from '../../inline-ask.js';
import {
  IDS, WORDS, askId, overwriteWhy, refusalWords, savedHtml, savedWordsHtml,
} from './draw.js';

/** The saved page's part: `{ said, asking, savedList(), liveWhy(), putSaved(), askOverwrite(i,
 * button), clear(), drop() }`. */
export function createSaved({ core, doc, here, put, fleetRow }) {
  const store = core.model;
  let said = { words: '', failed: false };
  let asking = null;

  /** Why there is no curve to save on the device: the words, or `null` when there is one. */
  function liveWhy() {
    if (here.side.computer) return null;
    const device = here.side.url ? store.device(here.side.url) : null;
    const link = here.side.url
      ? (store.part('link', here.side.url) ?? fleetRow(here.side.url)?.link) : null;
    if (link && link.state !== 'on') return WORDS.notConnected(here.side.name);
    if (!device?.presets) return WORDS.noToneControl(here.side.name);
    if (!device.tone?.response) return WORDS.notRead;
    return null;
  }

  /** The presets the side lists, as the saved page's rows. */
  function savedList() {
    if (here.side.computer) {
      const computer = store.page().host?.computer;
      if (!computer) return { rows: [], note: 'The computer\'s presets are not read yet.' };
      return {
        rows: (computer.presets ?? []).map((one) => ({
          name: one.name, built_in: one.built_in === true, imported: false, on: one.on === true,
          playable: one.playable, why: one.why,
        })),
      };
    }
    const device = here.side.url ? store.device(here.side.url) : null;
    if (!device?.presets) {
      // a device the daemon has not found has no profile to list presets from: that is not the
      // same as a device that has no tone control
      const link = here.side.url
        ? (store.part('link', here.side.url) ?? fleetRow(here.side.url)?.link) : null;
      const name = here.side.name || 'This device';
      return {
        rows: [],
        note: link && link.state !== 'on' ? WORDS.presetsUnknown(name) : WORDS.noToneControl(name),
      };
    }
    return {
      rows: device.presets.rows.map((one) => ({
        name: one.name, built_in: one.built_in === true, imported: one.imported === true,
        on: one.on === true,
      })),
    };
  }

  const putWords = () => put(IDS.savedWordsPlace, savedWordsHtml(said));
  /** The saved list, not while its question stands. */
  function putSaved() {
    if (asking) return;
    put(IDS.saved, savedHtml({
      list: savedList(), side: here.side, live: liveWhy(),
    }));
  }

  /** The press of *Overwrite with current…* on row `i`: its reason when it is disabled, else one
   * question, the safe answer focused. */
  function askOverwrite(i, button) {
    const row = savedList().rows[i];
    if (!row) return;
    const why = overwriteWhy(row, { side: here.side, live: liveWhy() });
    if (why !== null) {
      said = { words: why, failed: true };
      putWords();
      return;
    }
    said = { words: '', failed: false };
    putWords();
    const mine = here.opening;
    asking = askQuestion({
      within: doc.getElementById(askId(i)),
      text: WORDS.overwriteAsk(row.name, here.side.name),
      yes: 'overwrite', no: 'keep', words: { answers: WORDS.answers }, signal: here.stopping.signal,
      opener: button,
      go: async () => {
        const answer = await core.actions.overwritePreset(row.name, { device: here.side.url });
        if (!here.open || mine !== here.opening) return answer;
        if (answer.ok) said = { words: WORDS.overwritten(row.name), failed: false };
        else said = { words: refusalWords('overwrite', answer), failed: true };
        putWords();
        return answer;
      },
      onClose: () => {
        asking = null;
        if (here.open) putSaved();
      },
    });
  }

  return {
    get said() { return said; },
    get asking() { return asking; },
    savedList,
    liveWhy,
    putSaved,
    askOverwrite,
    clear() { said = { words: '', failed: false }; },
    drop() { asking = null; },
  };
}
