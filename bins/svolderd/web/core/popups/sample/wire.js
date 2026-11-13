/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The sample popup's wire: thin. It draws the sample's strings (draw.js) into the popup's frame,
// opens its inline asks in their place and its list in the page pane, takes its keys from the
// modal's `onKey` (it attaches no key listener), and owns one stream want, which it releases
// when the popup has left the page.
// The device it follows is the one on show when it was opened, named in its live line.

import { esc } from '../../html.js';
import { askForm, askQuestion } from '../../inline-ask.js';
import { closeOf, openList } from '../../list.js';
import {
  ENTRIES, IDS, LIST_LABEL, VOICES, WORDS, liveHtml, pageHtml,
} from './draw.js';

/** The owner of the sample's stream want. */
const OWNER = 'popup:sample';

/** The sample's part for the router: `{ show(page), hide(), onKey(event) }`, over `core`, drawn
 * into `frame` (popups/frame.js). */
export function wire({ core, frame }) {
  const store = core.model;
  const doc = frame.el.ownerDocument;
  /** The device the popup follows, its url, while it is open. */
  let device = null;
  /** Gives the stream want back; `null` while the popup is closed. */
  let release = null;
  /** The listeners on the store, taken off at the close. */
  let offs = [];
  /** The inline ask that is open, if any. */
  let ask = null;
  /** The list that is open, if any. */
  let list = null;
  /** What the form, the question and the list gave. */
  let name = '';
  let count = 0;
  let voice = VOICES[0];
  let page = null;

  const followed = () => {
    if (device === null) return null;
    const listed = (store.page().fleet?.devices ?? []).find((one) => one.url === device);
    return { url: device, name: listed?.name ?? device };
  };
  const model = () => ({
    page, device: followed(), volume: store.part('volume', device), name, count, voice,
  });
  const live = () => frame.put(IDS.live, liveHtml(model()));
  const said = () => {
    frame.put(IDS.name, esc(name));
    frame.put(IDS.count, esc(count));
  };

  /** The popup is opening: take the want and hear the followed device's readings. */
  function begin() {
    device = store.shown();
    release = core.stream.want(OWNER, { needs: ['meters'] });
    const mine = (data) => (data?.device ?? store.shown()) === device;
    offs = [
      store.on('volume', (data) => { if (mine(data)) live(); }),
      store.on('part', (data) => { if (data.device === device && data.name === 'volume') live(); }),
    ];
  }

  /** Open an ask in its place, from the word `opener`. */
  function open(make, opener) {
    const within = doc.getElementById(IDS.ask);
    if (!within) return;
    ask = make({
      within, opener, words: WORDS,
      onClose: () => { ask = null; },
    });
  }

  /** The chip toggles its list; a pick is the voice in use from then on. */
  function pick(chip) {
    if (closeOf(chip)) return;
    list = openList({
      opener: chip, label: LIST_LABEL,
      options: VOICES.map((one) => ({ value: one, text: one, on: one === voice })),
      choose: (value) => { voice = value; frame.put(IDS.voice, esc(voice)); },
      onClose: () => { list = null; },
    });
  }

  frame.el.addEventListener('click', (event) => {
    const chip = event.target.closest?.(`#${IDS.listOpen}`);
    if (chip) {
      pick(chip);
      return;
    }
    const word = event.target.closest?.(`#${IDS.askOpen}, #${IDS.formOpen}`);
    if (!word) return;
    if (word.id === IDS.askOpen) {
      open((options) => askQuestion({
        ...options, text: WORDS.question, yes: 'save', no: 'keep',
        go: async () => { count += 1; said(); return { ok: true }; },
      }), word);
    } else {
      open((options) => askForm({
        ...options, kind: 'rename', value: name,
        submit: async (text) => { name = text; said(); return { ok: true }; },
      }), word);
    }
  });

  return {
    show(to) {
      if (release === null) begin();
      page = to;
      frame.draw({ entries: ENTRIES, page, body: pageHtml(model()) });
    },
    hide() {
      release?.();
      release = null;
      for (const off of offs) off();
      offs = [];
      device = null;
    },
    onKey: (event) => list?.key(event) === true || ask?.key(event) === true,
  };
}
