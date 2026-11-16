/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The EQ library popup's wire: thin. It holds what a person has done in the popup (the search and
// its answer, the chosen measurement and its preview, the name being typed, the file read) and
// sends each step through `core.actions` (autoeq-actions.js), drawing the pages of draw.js into the
// popup's frame. Every number on the page is the daemon's.
//
// The side. The popup follows the side the page shows when it opens: the device on show (its name
// is the target of every call, `{ device }`) or the computer (`{ computer: true }`). The title
// names it in every draw (`EQ library · Kara`, `EQ library · This computer`). The popup is not
// `stays`: a change of the shown device closes it (the router), and the side cannot flip under it
// because the page is inert; a model that changes the side nonetheless (the computer became
// unavailable) draws the popup again for the side now, with the search kept and the preview, the
// import and the overwrite of the other side dropped.
//
// Answers that arrive late. Each request has a turn and an `AbortController`: a newer search, a
// newer choice, a closed popup or a page that steps away abort the one in flight, and an answer
// that is not the newest turn's is dropped. An aborted request is no failure and says nothing. A
// search is sent after a pause in the typing (`DEBOUNCE_MS`), at once on Enter or the button, and
// not while an input method is composing. The index's answer carries no query, so the turn is how
// an answer is matched to the one it answers.
//
// Places. The first draw of a page draws the whole box; after that each thing is put in its own
// place (the status, the results, the preview, the import, the upload's fields and words, the saved
// list), and the search field and the file input are never put, so what is typed and the file
// chosen stay. A put of a place holding the same markup changes nothing. The saved list is read
// from the store at each put (the device's presets, or the computer's), so an import's answer and
// the model's event that lists the new preset reach it in either order. It is not put while its
// question is open (the question lives in it); it is put when the question closes.
//
// Two pages have their own file, as parts of this wire: the upload (upload-wire.js) and the saved
// presets with the overwrite question (saved-wire.js). This file keeps the search, the preview and
// the import, which share the choice, and the state the parts read through `here`.
//
// The popup writes nothing but through the actions, attaches no key listener (the modal hands
// `onKey` the keys) and wants nothing of the stream: the page behind follows the device.

import { openList } from '../../list.js';
import {
  ENTRIES, IDS, WORDS, defaultName, defaultOpen, entriesOf, importHtml, importWordsHtml, keyOf,
  pageHtml, previewHtml, previewWordsHtml, refusalWords, resultsHtml, statusHtml, titleOf,
} from './draw.js';
import { createSaved } from './saved-wire.js';
import { createUpload, savedOf } from './upload-wire.js';

/** How long the typing in the search field pauses before the search is sent, in milliseconds. */
export const DEBOUNCE_MS = 300;

/** The measurement of a search answer as the daemon takes it back: its own four keys. */
const variantOf = (m) => ({
  name: m.name, source: m.source, form: m.form, ...(m.rig ? { rig: m.rig } : {}),
});

/** The EQ library's part for the router: `{ show(page), hide(), onKey(event) }`, over `core`,
 * drawn into `frame` (popups/frame.js). */
export function wire({ core, frame }) {
  const store = core.model;
  const doc = frame.el.ownerDocument;
  const view = doc.defaultView;
  /** Whether the popup stands, the page it shows and the count of openings (an answer of an earlier
   * opening is dropped). */
  let open = false;
  let page = null;
  let opening = 0;
  /** The side the popup was drawn for: `{ computer, url, name }`. */
  let side = { computer: false, url: null, name: '' };

  let query = '';
  let search = { status: 'idle' };
  let searching = 0;
  let searchStop = null;
  let timer = 0;
  /** Whether an input method is composing in the search field. */
  let composing = false;
  /** The disclosures the person set, by group key: whether each is open. */
  let opened = new Map();

  let selected = null;
  let compare = null;
  let preview = { status: 'idle' };
  let previewing = 0;
  let previewStop = null;
  let importing = { status: 'idle' };
  let importStop = null;


  let pick = null;
  let stopping = new AbortController();
  let offs = [];

  const fleetRow = (url) => (store.page().fleet?.devices ?? []).find((one) => one.url === url);

  /** The side now. */
  function sideNow() {
    if (core.computer?.side?.() === 'computer') {
      return { computer: true, url: null, name: WORDS.computer };
    }
    const url = store.shown() ?? null;
    let name = fleetRow(url)?.name ?? '';
    if (!name && url) {
      try {
        name = decodeURIComponent(url);
      } catch {
        name = url;
      }
    }
    return { computer: false, url, name };
  }
  const sideKey = (one) => (one.computer ? '\0computer' : one.url);

  /** What the draws are given. */
  function model() {
    return {
      query, search, selected, preview, compare, importing, side, open: opened, up: upload.up,
      list: saved.savedList(), live: saved.liveWhy(), words: saved.said.words,
      failed: saved.said.failed,
    };
  }

  function draw() {
    frame.draw({
      title: titleOf(side), entries: entriesOf(), page, body: pageHtml(page, model()),
    });
  }

  /** Put `html` in the place `id`, if it is drawn. */
  const put = (id, html) => frame.put(id, html);

  /** Put the field of the import with the caret where the person had it. */
  function putImport() {
    const field = doc.getElementById(IDS.importName);
    const had = field !== null && doc.activeElement === field;
    put(IDS.importPlace, importHtml({ selected, state: importing, side }));
    put(IDS.importWordsPlace, importWordsHtml({ selected, state: importing, side }));
    if (had) doc.getElementById(IDS.importName)?.focus({ preventScroll: true });
  }
  function putPreview() {
    put(IDS.preview, previewHtml({ selected, preview, compare, side }));
    put(IDS.previewWords, previewWordsHtml({ selected, preview, side }));
  }
  const putStatus = () => put(IDS.status, statusHtml(search));
  const putResults = () => put(IDS.results, resultsHtml({
    answer: search.answer, selected, open: opened,
  }));

  /** Give the focus to the element `id`, if it is drawn and takes it. */
  function focusOn(id) {
    const el = doc.getElementById(id);
    const off = el?.getAttribute('aria-disabled') === 'true' || el?.disabled;
    if (el && frame.el.contains(el) && !off) el.focus({ preventScroll: true });
  }

  /** The popup's state, as the parts (upload-wire.js, saved-wire.js) read it. */
  const here = {
    get open() { return open; },
    get opening() { return opening; },
    get side() { return side; },
    get stopping() { return stopping; },
  };
  const saved = createSaved({ core, doc, here, put, fleetRow });
  const upload = createUpload({
    core, doc, view, here, put, focusOn, putSaved: saved.putSaved, listed,
  });

  /** Stop what is in flight: the controllers that are given. */
  function stopAll() {
    for (const stop of [searchStop, previewStop, importStop]) stop?.abort();
    upload.stop();
    searchStop = null;
    previewStop = null;
    importStop = null;
    view.clearTimeout(timer);
    timer = 0;
  }

  // ---- the search ----

  async function runSearch(text) {
    view.clearTimeout(timer);
    timer = 0;
    searchStop?.abort();
    const query0 = text.trim();
    if (query0 === '') {
      searching += 1;
      searchStop = null;
      search = { status: 'idle' };
      opened = new Map();
      putStatus();
      putResults();
      return;
    }
    const stop = new AbortController();
    searchStop = stop;
    searching += 1;
    const turn = searching;
    const mine = opening;
    search = { ...search, status: 'loading', query: query0 };
    putStatus();
    const answer = await core.actions.autoEqSearch(query0, { signal: stop.signal });
    // an aborted request, a newer search's turn, or a popup that closed: none is this one's answer
    if (answer.aborted || !open || mine !== opening || turn !== searching) return;
    if (answer.ok && Array.isArray(answer.groups)) {
      opened = new Map();
      search = { status: 'ready', query: query0, answer };
    } else {
      search = {
        status: 'failed', query: query0,
        words: refusalWords('search', answer.ok ? { reason: 'failed' } : answer),
      };
    }
    putStatus();
    putResults();
  }

  /** Send the search after the pause, unless an input method is composing. */
  function later() {
    view.clearTimeout(timer);
    timer = 0;
    if (composing) return;
    const text = query;
    if (text.trim() === '') {
      runSearch(text);
      return;
    }
    timer = view.setTimeout(() => {
      timer = 0;
      runSearch(text);
    }, DEBOUNCE_MS);
  }

  // ---- the preview ----

  async function runPreview() {
    previewStop?.abort();
    const stop = new AbortController();
    previewStop = stop;
    previewing += 1;
    const turn = previewing;
    const mine = opening;
    const second = compare ? compare.variant : undefined;
    preview = { status: 'loading' };
    putPreview();
    const answer = side.computer
      ? await core.actions.autoEqPreview(selected.variant,
        { computer: true, compare: second, signal: stop.signal })
      : await core.actions.autoEqPreview(selected.variant,
        { device: side.url, compare: second, signal: stop.signal });
    if (answer.aborted || !open || mine !== opening || turn !== previewing) return;
    preview = answer.ok && answer.preview ? { status: 'ready', answer }
      : { status: 'failed', words: refusalWords('preview', answer) };
    putPreview();
  }

  /** Choose measurement `v` of headphone `g` of the answer on show. */
  function choose(g, v) {
    const group = search.answer?.groups?.[g];
    const one = group?.variants?.[v];
    if (!one) return;
    selected = {
      variant: variantOf(one),
      words: one.words,
      group: {
        name: group.name,
        variants: group.variants.map((m) => ({ variant: variantOf(m), words: m.words })),
      },
    };
    compare = null;
    // an import still on its way is not stopped by another choice: a save that lands says so
    // in the import's words (`late`)
    importing = { status: 'idle' };
    for (const button of frame.el.querySelectorAll('.fpe-variant')) {
      button.setAttribute('aria-pressed', String(keyOf(search.answer.groups[button.dataset.g]
        .variants[button.dataset.v]) === keyOf(selected.variant)));
    }
    putImport();
    runPreview();
  }

  function chooseCompare(opener) {
    if (!selected) return;
    const others = selected.group.variants
      .filter((m) => keyOf(m.variant) !== keyOf(selected.variant));
    pick = openList({
      opener, label: WORDS.compareLabel, signal: stopping.signal,
      options: others.map((m, i) => ({ value: i, text: m.words, on: false })),
      choose: (value) => {
        compare = others[Number(value)] ?? null;
        if (compare) {
          runPreview();
          focusOn(IDS.compareClear);
        }
      },
      onClose: () => { pick = null; },
    });
  }

  // ---- the import ----

  /** A save has landed: the lists of both sides are read again (the side shown now, and the other
   * one, which its own popup page or the Devices popup may be showing: the computer's model when a
   * host is there, the device on show). */
  function listed() {
    const shown = store.shown();
    if (side.computer) {
      core.computer?.resync?.();
      if (shown) store.refresh(shown);
    } else {
      if (side.url) store.refresh(side.url);
      if (store.page().host) core.computer?.resync?.();
    }
  }

  async function runImport() {
    if (!selected || importing.status === 'sending') return;
    const field = doc.getElementById(IDS.importName);
    const typed = (field ? field.value : (importing.name ?? defaultName(selected)));
    const name = typed.trim();
    importing = { ...importing, name: typed };
    if (name === '') {
      importing = { name: typed, status: 'refused', words: WORDS.noName };
      putImport();
      focusOn(IDS.importName);
      return;
    }
    if (!side.computer && !side.url) {
      importing = { name: typed, status: 'refused', words: WORDS.noTarget };
      putImport();
      return;
    }
    const stop = new AbortController();
    importStop = stop;
    const mine = opening;
    const variant = selected.variant;
    const sentWords = selected.words;
    importing = { name: typed, status: 'sending' };
    putImport();
    // the daemon shortens the headphone's own name itself: it is sent only once it is changed
    const named = name === defaultName(selected) ? undefined : name;
    const answer = side.computer
      ? await core.actions.autoEqImport(variant,
        { computer: true, name: named, signal: stop.signal })
      : await core.actions.autoEqImport(variant,
        { device: side.url, name: named, signal: stop.signal });
    // the choice moved on while it was saving: this answer is for the measurement it was sent for
    if (answer.aborted || !open || mine !== opening) return;
    if (keyOf(variant) !== keyOf(selected?.variant ?? {})) {
      // saved for a measurement that is no longer the chosen one: the lists change, and the
      // import's words say what landed, under what the new choice says
      if (answer.ok) {
        importing = { ...importing, late: WORDS.savedLate(answer.name, sentWords) };
        putImport();
        saved.putSaved();
        listed();
      }
      return;
    }
    if (answer.ok) {
      importing = { name: typed, status: 'saved', saved: savedOf(answer) };
      putImport();
      saved.putSaved();
      listed();
      return;
    }
    importing = {
      name: typed, status: 'refused', words: refusalWords('import', answer),
      ...(answer.reason === 'taken' ? { suggest: `${name} (${variant.source})` } : {}),
    };
    putImport();
    if (['taken', 'bad_name'].includes(answer.reason)) focusOn(IDS.importName);
    else focusOn(IDS.importSave);
  }

  // ---- events ----

  frame.el.addEventListener('input', (event) => {
    const field = event.target;
    if (!open) return;
    if (field.id === IDS.query) {
      query = field.value;
      if (!event.isComposing) later();
    } else if (field.id === IDS.importName) {
      importing = { ...importing, name: field.value };
    } else if (field.id === IDS.upName) {
      upload.typed(field.value);
    }
  });
  // an input method that ends its composition has typed the word: the search follows the pause
  frame.el.addEventListener('compositionstart', (event) => {
    if (event.target.id === IDS.query) composing = true;
  });
  frame.el.addEventListener('compositionend', (event) => {
    if (event.target.id !== IDS.query || !open) return;
    composing = false;
    query = event.target.value;
    later();
  });
  frame.el.addEventListener('change', (event) => {
    if (open && event.target.id === IDS.file) upload.takeFile(event.target.files?.[0] ?? null);
  });
  // `toggle` does not bubble: it is caught on the way down. A disclosure the person set is kept
  // through the next put of the results, unless it is how the draw would have it anyway.
  frame.el.addEventListener('toggle', (event) => {
    const el = event.target;
    if (!open || !el.classList?.contains('fpe-group') || !search.answer) return;
    const key = el.dataset.key;
    const group = search.answer.groups.find((one) => one.key === key);
    if (!group) return;
    if (el.open === defaultOpen({ answer: search.answer, group, selected })) opened.delete(key);
    else opened.set(key, el.open);
  }, true);

  frame.el.addEventListener('submit', (event) => {
    const form = event.target.closest?.('form[data-form]');
    if (!form) return;
    // stopped before anything else can return: a submit the browser took as its own would be a GET
    // of the page with the fields in the address
    event.preventDefault();
    if (!open) return;
    if (form.dataset.form === 'search') {
      query = doc.getElementById(IDS.query)?.value ?? query;
      runSearch(query);
    } else if (form.dataset.form === 'import') {
      runImport();
    } else if (form.dataset.form === 'upload') {
      upload.runUpload();
    }
  });

  frame.el.addEventListener('click', (event) => {
    if (!open) return;
    const target = event.target;
    const variant = target.closest?.('.fpe-variant');
    if (variant) {
      choose(Number(variant.dataset.g), Number(variant.dataset.v));
      return;
    }
    const button = target.closest?.('button');
    if (!button) return;
    if (button.id === IDS.compareOpen) {
      if (!pick) chooseCompare(button);
    } else if (button.id === IDS.compareClear) {
      compare = null;
      runPreview();
      focusOn(IDS.compareOpen);
    } else if (button.dataset.act === 'suggest') {
      importing = { name: importing.suggest, status: 'idle' };
      putImport();
      focusOn(IDS.importName);
    } else if (button.dataset.act === 'overwrite') {
      saved.askOverwrite(Number(button.dataset.i), button);
    }
  });

  /** The model changed: the saved list and the title follow it. */
  function update() {
    if (!open) return;
    const now = sideNow();
    if (sideKey(now) !== sideKey(side)) {
      stopAll();
      // the search in flight was stopped: its query is sent again for the side now, and the
      // turn moves on so that nothing of the stopped one is taken
      searching += 1;
      side = now;
      selected = null;
      compare = null;
      preview = { status: 'idle' };
      importing = { status: 'idle' };
      saved.clear();
      const kept = search.status === 'loading' ? search.query : null;
      draw();
      if (kept) runSearch(kept);
      return;
    }
    side = now;
    if ((page ?? ENTRIES[0].id) === 'saved') saved.putSaved();
  }

  return {
    show(to) {
      if (!open) {
        open = true;
        opening += 1;
        stopping = new AbortController();
        offs = [
          store.on('model', update), store.on('device', update),
          store.on('part', ({ name }) => {
            if (['presets', 'tone', 'link'].includes(name)) update();
          }),
          store.on('host', update),
        ];
      }
      page = to;
      side = sideNow();
      saved.clear();
      draw();
    },
    hide() {
      if (!open) return;
      open = false;
      stopping.abort();
      stopAll();
      for (const off of offs) off();
      offs = [];
      pick = null;
      saved.drop();
      query = '';
      search = { status: 'idle' };
      opened = new Map();
      selected = null;
      compare = null;
      preview = { status: 'idle' };
      importing = { status: 'idle' };
      upload.reset();
      composing = false;
      saved.clear();
    },
    onKey: (event) => Boolean(saved.asking?.key(event) || pick?.key(event)),
  };
}
