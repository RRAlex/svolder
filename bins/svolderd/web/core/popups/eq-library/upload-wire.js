/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The EQ library popup's upload page, as a part of the wire (wire.js): the file read, the name
// typed, the save and the words of each step. It owns what the page holds (`up`, the read turn and
// the request in flight); the wire gives it the popup's state through `here` (whether the popup
// stands, the count of openings, the side), the draws' `put` and `focusOn`, and what a landed save
// does in the other places (`putSaved`, `listed`).

import { FILE_MAX, IDS, WORDS, refusalWords, uploadFieldsHtml, uploadWordsHtml } from './draw.js';

/** What a save answered, as the page keeps it. */
export const savedOf = (answer) => ({
  name: answer.name, filters: answer.filters, skipped: answer.skipped ?? 0,
});

/** The name a preset imported from a file is called by default, as `svolder eq import` names it:
 * the file's name without its extension and without a trailing ` ParametricEQ`, cut at 40
 * characters. */
function stemOf(name) {
  const stem = String(name ?? '').replace(/\.[^./\\]*$/, '').trim();
  const bare = / parametriceq$/i.test(stem) ? stem.slice(0, -' parametriceq'.length) : stem;
  return [...bare.trim()].slice(0, 40).join('').trim();
}

/** The upload page's part: `{ up, typed(name), reset(), stop(), takeFile(file), runUpload() }`. */
export function createUpload({ core, doc, view, here, put, focusOn, putSaved, listed }) {
  let up = { filename: '', text: null, name: null, status: 'idle' };
  let uploadStop = null;
  let reading = 0;

  function putUpload() {
    put(IDS.upFields, uploadFieldsHtml({ up, side: here.side }));
    put(IDS.upWordsPlace, uploadWordsHtml({ up, side: here.side }));
  }

  /** The text of `file`. */
  function textOf(file) {
    if (typeof file.text === 'function') return file.text();
    return new Promise((resolve, reject) => {
      const reader = new view.FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.onerror = () => reject(reader.error);
      reader.readAsText(file);
    });
  }

  async function takeFile(file) {
    reading += 1;
    const turn = reading;
    const mine = here.opening;
    // each file starts with its own name: only a name typed from here on is the person's
    up = { ...up, typed: false };
    if (!file) {
      up = { ...up, filename: '', text: null, status: 'idle' };
      putUpload();
      return;
    }
    const base = { ...up, filename: file.name, text: null };
    if (file.size > FILE_MAX) {
      up = { ...base, status: 'unreadable', words: WORDS.fileTooLarge };
      putUpload();
      return;
    }
    up = { ...base, status: 'reading' };
    putUpload();
    let text = null;
    try {
      text = await textOf(file);
    } catch {
      text = null;
    }
    if (!here.open || mine !== here.opening || turn !== reading) return;
    if (text === null) {
      up = { ...base, status: 'unreadable', words: WORDS.fileUnreadable };
    } else if (text.length > FILE_MAX) {
      up = { ...base, status: 'unreadable', words: WORDS.fileTooLarge };
    } else {
      // a name typed while the file was being read is the person's: it stays
      const name = up.typed === true ? up.name : stemOf(file.name);
      up = { ...up, filename: file.name, text, name, status: 'idle' };
    }
    putUpload();
  }

  async function runUpload() {
    if (up.status === 'sending') return;
    const field = doc.getElementById(IDS.upName);
    const typed = field ? field.value : (up.name ?? '');
    up = { ...up, name: typed };
    if (up.text === null) {
      up = { ...up, status: up.status === 'unreadable' ? 'unreadable' : 'idle' };
      putUpload();
      focusOn(IDS.file);
      return;
    }
    const name = typed.trim();
    if (name === '') {
      up = { ...up, status: 'refused', field: true, words: WORDS.noName };
      putUpload();
      focusOn(IDS.upName);
      return;
    }
    const stop = new AbortController();
    uploadStop = stop;
    const mine = here.opening;
    const sent = up.text;
    const chosen = reading;
    up = { ...up, status: 'sending', field: false };
    putUpload();
    const answer = await core.actions.autoEqUpload(sent, name, { signal: stop.signal });
    if (answer.aborted || !here.open || mine !== here.opening) return;
    if (chosen !== reading) {
      // another file was chosen while this one was saving: the answer is not that file's
      if (answer.ok) {
        putSaved();
        listed();
      }
      return;
    }
    if (answer.ok) {
      up = { ...up, status: 'saved', saved: savedOf(answer) };
      putUpload();
      putSaved();
      listed();
      return;
    }
    const naming = ['taken', 'bad_name'].includes(answer.reason);
    up = { ...up, status: 'refused', field: naming, words: refusalWords('upload', answer) };
    putUpload();
    focusOn(naming ? IDS.upName : IDS.upSave);
  }

  return {
    get up() { return up; },
    typed(name) { up = { ...up, name, typed: true }; },
    reset() { up = { filename: '', text: null, name: null, status: 'idle' }; },
    stop() {
      uploadStop?.abort();
      uploadStop = null;
    },
    takeFile,
    runUpload,
  };
}
