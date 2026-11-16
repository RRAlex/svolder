/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Virtual speakers tab's part of the Sound popup: it reads the speakers of the computer's
// model (`host.computer.eq.speakers`, the daemon's own view) with what this viewer has asked laid
// over it, and the catalogue (`actions.computer.speakersCatalogue`), draws them (draw.js) into the
// popup's frame and writes through the core's calls (speakers-actions.js).
//
// The catalogue's answer is the truth about which sets are fetched. A fetch, an import and a
// forget patch it from their own answer at once and read it again; the view's list of sets (the
// stream) only tells the part to read it again when it changes, so the answer of a call and the
// stream's update may come in either order. A set another front end removed answers `missing`:
// the part reads the catalogue again, says it in words and does not retry. A fetch in flight is
// cancelled by its Cancel button and nothing else (the request is aborted, which cancels the
// daemon's job). The page gets no progress of a download: a running fetch says so, with no count.
//
// The part draws only while it is the tab shown (`frame` is the Sound popup's view of it). A fetch
// or an import keeps running while another tab is shown or the popup is closed: its end is kept
// (the set, or the words of a failure) and the page shows it, or the fetch still running, when
// the tab shows again.

import { guarded } from '../../prefs.js';
import {
  IDS, WORDS, VIEWS, namesOf, pageHtml, shownView, switchHtml,
} from './draw.js';

/** The id of the slot the page is put in. */
export const SLOT = 'popup-sound-sp-slot';
/** The storage key of the chip shown on a narrow window. */
export const CHIP_KEY = 'svolder.speakers.chip';
/** The most a file may be to import through the page, in MiB (the route's). */
export const IMPORT_MAX_MIB = 32;

/** `bytes` as base64 (standard alphabet). */
export function base64(bytes) {
  let binary = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return globalThis.btoa(binary);
}

/** The Virtual speakers part: `{ show(), hide(), onKey(event) }` over `core`, drawn into
 * `frame`. */
export function wire({ core, frame, view: window = frame.el.ownerDocument.defaultView }) {
  const doc = frame.el.ownerDocument;
  const store = core.model;
  const actions = core.actions.computer;
  const keep = guarded((() => {
    try {
      return window.localStorage;
    } catch {
      return null;
    }
  })());
  let open = false;
  let refusal = null;
  let offs = [];
  let headed = false;
  /** The catalogue as last read (`{ sets, fetching, note, max_sets }`) and the read's token. */
  let cat = null;
  let reads = 0;
  /** The ids of the sets the view listed when the catalogue was last read. */
  let seen = '';
  /** The job running (`{ id, kind, stop }`), the set filter, the chip and the import's choice. */
  let job = null;
  let filter = 'all';
  let importRoom = 'no';
  let importing = null;
  const chipAt = () => {
    const n = Number(keep.read(CHIP_KEY));
    return Number.isInteger(n) && n >= 0 && n < VIEWS.length ? n : 0;
  };
  let chip = chipAt();

  const computerNow = () => store.page().host?.computer ?? null;
  const spNow = () => computerNow()?.eq?.speakers ?? null;
  const shown = () => shownView(spNow(), actions.speakersAsked());
  const model = () => ({
    sp: shown(), cat, filter, chip, refusal, importRoom, importing: importing !== null,
    busy: job?.id ?? null, kind: job?.kind ?? null, computer: computerNow() !== null,
    crossfeedOn: computerNow()?.eq?.crossfeed?.on === true,
  });
  const slot = () => `<div id="${SLOT}">${pageHtml(model())}</div>`;

  function draw() {
    const sp = shown();
    headed = Boolean(sp);
    frame.draw({ page: 'speakers', body: slot(), head: switchHtml(sp) });
  }

  function refresh() {
    if (!open) return;
    const sp = shown();
    if (Boolean(sp) !== headed) {
      draw();
      return;
    }
    // the open file dialog's input and a typed name live in the page: keep them across a redraw
    const typed = doc.getElementById(IDS.label)?.value ?? '';
    frame.put(SLOT, pageHtml(model()));
    const label = doc.getElementById(IDS.label);
    if (label && typed) label.value = typed;
    frame.put(`popup-${frame.name}-head`, switchHtml(sp));
  }

  /** Read the catalogue again; the newest read wins. */
  async function load() {
    const mine = ++reads;
    seen = (spNow()?.sets ?? []).map((one) => one.id).join(',');
    const answer = await actions.speakersCatalogue();
    if (!open || mine !== reads) return;
    if (answer.ok) {
      cat = {
        sets: answer.sets ?? [], fetching: answer.fetching, note: answer.note ?? null,
        max_sets: answer.max_sets,
      };
    } else if (!answer.aborted && cat === null) {
      refusal = answer.error || WORDS.noCatalogue;
    }
    refresh();
  }

  /** Put `set` (a catalogue set as an answer gives it) into the list now. */
  function patch(set) {
    if (!cat) return;
    const at = cat.sets.findIndex((one) => one.id === set.id);
    const sets = [...cat.sets];
    if (at < 0) sets.push(set);
    else sets[at] = set;
    cat = { ...cat, sets };
  }

  /** The view's list of sets changed: another front end fetched or forgot one. */
  function followSets() {
    const ids = (spNow()?.sets ?? []).map((one) => one.id).join(',');
    if (open && cat && ids !== seen) load();
  }

  function send(fields) {
    refusal = null;
    const sent = actions.setSpeakers(fields);
    sent.then((answer) => {
      if (!open || answer?.queued) return;
      if (answer?.ok) refusal = null;
      else if (answer?.reason === 'missing') load();
      refresh();
    });
    return sent;
  }

  /** The set to choose for the room `room`: the set in use if it is of that room, else the first
   * fetched one. */
  function setFor(room) {
    const sets = (cat?.sets ?? []).filter((one) => one.room === room && one.fetched && !one.damaged);
    const now = shown()?.set;
    return sets.find((one) => one.id === now)?.id ?? sets[0]?.id ?? null;
  }

  async function run(id, kind, call) {
    if (job) return;
    refusal = null;
    const stop = new AbortController();
    job = { id, kind, stop };
    refresh();
    const answer = await call(stop.signal);
    job = null;
    if (answer.aborted) {
      refresh();
      return;
    }
    if (answer.ok) {
      refusal = null;
      if (answer.set) patch(answer.set);
      for (const gone of answer.evicted ?? []) {
        const row = cat?.sets.find((one) => one.id === gone);
        if (row) patch({ ...row, fetched: false });
      }
    } else {
      refusal = answer.error;
      if (open && (answer.reason === 'missing' || answer.reason === 'not_listed')) load();
    }
    refresh();
    if (answer.ok && open) load();
  }

  const fetchSet = (id) => run(id, 'fetch', (signal) => actions.speakersFetch(id, { signal }));

  async function forgetSet(id) {
    await run(id, 'forget', (signal) => actions.speakersForget(id, { signal }));
    const row = cat?.sets.find((one) => one.id === id);
    if (row && !refusal) {
      if (row.own) cat = { ...cat, sets: cat.sets.filter((one) => one.id !== id) };
      else patch({ ...row, fetched: false });
      refresh();
    }
  }

  async function readFile(file) {
    refusal = null;
    if (file.size > IMPORT_MAX_MIB * 1024 * 1024) {
      refusal = WORDS.fileTooBig(IMPORT_MAX_MIB);
      refresh();
      return;
    }
    const stop = new AbortController();
    importing = { stop };
    refresh();
    let data;
    try {
      data = base64(new Uint8Array(await file.arrayBuffer()));
    } catch {
      importing = null;
      refusal = WORDS.fileRead;
      refresh();
      return;
    }
    const typed = doc.getElementById(IDS.label)?.value.trim();
    const label = typed || file.name.replace(/\.[^.]*$/, '');
    const answer = await actions.speakersImport({ data, label, room: importRoom },
      { signal: stop.signal });
    importing = null;
    if (answer.ok) {
      if (answer.set) patch(answer.set);
      const field = doc.getElementById(IDS.label);
      if (field) field.value = '';
    } else if (!answer.aborted) {
      refusal = answer.error;
    }
    refresh();
    if (answer.ok && open) load();
  }

  const off = (button) => button.getAttribute('aria-disabled') === 'true';

  /** Open the file dialog: an input of its own, made now, so that a redraw of the page while the
   * dialog is open cannot take it away. */
  function chooseFile() {
    const input = doc.createElement('input');
    input.type = 'file';
    input.accept = '.sofa,.wav,audio/wav';
    input.addEventListener('change', () => {
      const [file] = input.files ?? [];
      if (open && file) readFile(file);
    });
    input.click();
  }

  frame.el.addEventListener('click', (event) => {
    if (!open) return;
    const button = event.target.closest?.('button');
    if (!button || !frame.el.contains(button) || off(button)) return;
    const { act } = button.dataset;
    if (act === 'on') {
      const sp = shown();
      if (sp) send({ on: !sp.on });
    } else if (act === 'room') {
      const to = setFor(button.dataset.room);
      if (to === null) {
        filter = button.dataset.room;
        const named = namesOf(shown(), cat)[button.dataset.room];
        refusal = named ? WORDS.noneOf(named.name) : WORDS.noCatalogue;
        refresh();
      } else if (to !== shown()?.set) {
        send({ set: to });
      }
    } else if (act === 'use') {
      send({ set: button.dataset.id });
    } else if (act === 'fetch') {
      fetchSet(button.dataset.id);
    } else if (act === 'forget') {
      forgetSet(button.dataset.id);
    } else if (act === 'cancel') {
      job?.stop.abort();
    } else if (act === 'filter') {
      filter = button.dataset.filter;
      refresh();
    } else if (act === 'chip') {
      chip = Number(button.dataset.chip);
      keep.write(CHIP_KEY, String(chip));
      refresh();
      doc.getElementById(`popup-sound-sp-chip-${chip}`)?.focus({ preventScroll: true });
    } else if (act === 'import-room') {
      importRoom = button.dataset.room;
      refresh();
    } else if (act === 'import' && !importing) {
      chooseFile();
    }
  });

  return {
    show() {
      if (!open) {
        open = true;
        offs = [
          store.on('model', refresh), store.on('host', () => { refresh(); followSets(); }),
          core.computer.on(() => refresh()),
          actions.onSpeakersRefusal((why) => {
            refusal = why;
            refresh();
          }),
        ];
      } else {
        refusal = null;
      }
      chip = chipAt();
      draw();
      load();
    },
    /** The tab goes: a fetch or an import runs on, and what it ends with waits for the tab. */
    hide() {
      if (!open) return;
      open = false;
      reads++;
      for (const stop of offs) stop();
      offs = [];
      refusal = null;
    },
    /** Left and Right on a chip move to the next or the previous one. */
    onKey(event) {
      const on = doc.activeElement?.closest?.('.fps-chips');
      if (!open || !on || (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight')) return false;
      const step = event.key === 'ArrowRight' ? 1 : -1;
      chip = (chip + step + VIEWS.length) % VIEWS.length;
      keep.write(CHIP_KEY, String(chip));
      refresh();
      doc.getElementById(`popup-sound-sp-chip-${chip}`)?.focus({ preventScroll: true });
      return true;
    },
  };
}
