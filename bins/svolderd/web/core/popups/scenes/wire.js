/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Scenes popup's wire: it reads the daemon's view of the scenes (core/scenes.js) and draws it
// (draw.js) into the popup's frame, and writes through the scenes' actions (core/scenes-actions.js):
// a tap on a scene's name recalls it, "Save current as…" saves, the pencil and the bin of a row
// ask in place (a name field, a question). The daemon's answers are the truth: the list, the lamp
// and "On now" come from the view alone, never from what a write answered, so a recall's answer
// and the stream's `scenes` event can come in either order and the row never goes back. What an
// answer says (a recall's outcome, a refusal's words) is the line under the list, which no event
// redraws. A refusal is shown in words; the page acts on its `reason` only. A scene taken away
// elsewhere answers "gone" and the list is read again once.
//
// The list is not drawn again while a row's ask stands (the ask is inside the row): the draw waits
// for it to close. The popup is a writer's alone (listed.js), stands through a change of the shown
// device, wants nothing of the stream and attaches no key listener beyond the ask's own `key`.

import { post } from '../../api.js';
import { askForm, askQuestion } from '../../inline-ask.js';
import { createScenes, said as saidWords } from '../../scenes.js';
import { createScenesActions, GONE } from '../../scenes-actions.js';
import { createWrites } from '../../write.js';
import {
  IDS, WORDS, hintHtml, listHtml, pageHtml, readingHtml, recallSaid, savedSaid, sayHtml,
} from './draw.js';

/** The Scenes part for the router: `{ show(page), hide(), onKey(event) }`, over `core`, drawn
 * into `frame` (popups/frame.js). */
export function wire({ core, frame }) {
  const doc = frame.el.ownerDocument;
  const store = core.model;
  const scenes = createScenes({ store });
  const writes = createWrites({ store, post, toast: (...args) => core.frame.toast(...args) });
  const actions = createScenesActions({
    store, post, writes, fetch: (...args) => doc.defaultView.fetch(...args),
  });
  const words = { newName: WORDS.newName, presetName: WORDS.sceneName, answers: WORDS.answers };

  let open = false;
  /** How many times the popup was opened: an answer that comes after a close is not shown. */
  let opening = 0;
  let stop = null;
  let said = null;
  /** The name typed in the save field, kept through a draw of the whole page. */
  let typed = '';
  /** The scene being recalled, so a second tap waits for the answer. */
  let recalling = null;
  /** The ask that stands (one at a time), and whether it stands inside the list. */
  let asking = null;
  let inList = false;
  /** Whether the page was drawn with a view. */
  let hadView = false;
  let aborter = new AbortController();

  const put = (id, html) => frame.put(id, html);
  const putSay = () => put(IDS.say, sayHtml(said));
  const draw = () => frame.draw({
    page: 'scenes',
    body: pageHtml({ view: scenes.view(), page: store.page(), said, value: typed }),
  });

  /** Draw what changed, in its places: the list not while a row's ask stands in it. A view that
   * comes or goes draws the whole page again (no view, no list and no save field). */
  function refresh() {
    if (!open) return;
    const view = scenes.view();
    if (Boolean(view) !== hadView) {
      hadView = Boolean(view);
      draw();
      return;
    }
    put(IDS.reading, readingHtml(view));
    put(IDS.hint, hintHtml(store.page()));
    if (!inList) put(IDS.list, listHtml(view));
  }

  const setSaid = (next) => {
    said = next;
    if (open) putSay();
  };
  const failed = (why) => setSaid({ headline: saidWords(why), failed: true });

  /** Begin an ask: `make` draws it and returns its handle; `place` says whether it is in the
   * list. The handle is kept to hand a key to, until it closes. */
  function ask(make, listed) {
    let mine = null;
    const close = () => {
      if (asking !== mine) return;
      asking = null;
      const wasList = inList;
      inList = false;
      if (open && wasList) refresh();
    };
    mine = make(close);
    asking = mine;
    inList = listed;
    return mine;
  }

  /** The scene of the row `button` is in, by the name the row carries (never its place in the
   * list: the list may stand while the view moves). A scene the view no longer has is gone: it
   * says so, the standing ask closes and the view is read again. */
  function sceneOf(button) {
    const name = button.closest?.('.sc-row')?.dataset.name ?? null;
    if (name === null) return null;
    if (scenes.view()?.scenes.some((scene) => scene.name === name)) return name;
    asking?.close(false);
    failed(GONE);
    actions.read();
    return null;
  }

  /** The place for an ask in the row of the scene `name`, as the list is drawn now. */
  const placeOf = (name) => [...doc.querySelectorAll(`#${IDS.list} .sc-row`)]
    .find((row) => row.dataset.name === name)?.querySelector('.sc-ask') ?? null;

  /** The both-sides question of a recall, asked in its place: true on a yes. */
  function recallQuestion(question, opener) {
    return new Promise((resolve) => {
      ask((close) => askQuestion({
        within: doc.getElementById(IDS.ask), text: question, yes: 'recall', no: 'cancel',
        words: { answers: WORDS.answers }, signal: aborter.signal, opener,
        go: async () => {
          resolve(true);
          return { ok: true };
        },
        onClose: () => {
          resolve(false);
          close();
        },
      }), false);
    });
  }

  async function recall(name, button) {
    if (recalling !== null) return;
    recalling = name;
    const mine = opening;
    setSaid({ headline: WORDS.recalling });
    const answer = await actions.recall(name, { asker: (question) => recallQuestion(question, button) });
    recalling = null;
    if (!open || mine !== opening) return;
    if (answer.ok) setSaid(recallSaid(answer.name ?? name, answer.outcome, scenes.view()));
    else if (answer.declined) setSaid(null);
    else failed(answer.error);
  }

  function askRename(button) {
    const from = sceneOf(button);
    if (from === null) return;
    // the ask that stands closes first (the list is drawn again if it was frozen), then the row
    // is found as the list is now
    asking?.close(false);
    const place = placeOf(from);
    if (!place) return;
    const mine = opening;
    ask((close) => askForm({
      within: place, kind: 'rename', value: from, words, signal: aborter.signal, opener: button,
      submit: async (to) => {
        const answer = await actions.rename(from, to);
        if (!open || mine !== opening) return answer;
        if (answer.ok) setSaid({ headline: WORDS.renamed(from, answer.name ?? to) });
        else failed(answer.error);
        if (answer.gone) return { ok: true };
        return answer;
      },
      onClose: close,
    }), true);
  }

  function askDelete(button) {
    const name = sceneOf(button);
    if (name === null) return;
    asking?.close(false);
    const place = placeOf(name);
    if (!place) return;
    const mine = opening;
    ask((close) => askQuestion({
      within: place, text: WORDS.deleteAsk(name), yes: 'delete', no: 'keep', words,
      signal: aborter.signal, opener: button,
      go: async () => {
        const answer = await actions.delete(name);
        if (!open || mine !== opening) return answer;
        if (answer.ok) setSaid({ headline: WORDS.deleted(answer.name ?? name) });
        else failed(answer.error);
        if (answer.gone) return { ok: true };
        return answer;
      },
      onClose: close,
    }), true);
  }

  /** Empty the save field as a form that was never touched: no red edge for an empty field. */
  function clearField() {
    const field = doc.getElementById(IDS.field);
    if (!field) return;
    field.defaultValue = '';
    field.form?.reset();
    field.value = '';
  }

  /** Save under `name`; a name the daemon says is taken asks once before it saves over it. */
  async function save(name) {
    const mine = opening;
    const answer = await actions.save(name);
    if (!open || mine !== opening) return;
    if (answer.ok) {
      typed = '';
      clearField();
      setSaid(savedSaid(answer.name ?? name, answer.outcome));
    } else if (answer.reason === 'taken') {
      ask((close) => askQuestion({
        within: doc.getElementById(IDS.over), text: WORDS.overAsk(name), yes: 'overwrite',
        no: 'keep', words, signal: aborter.signal, opener: doc.getElementById(IDS.field),
        go: async () => {
          const over = await actions.save(name, { force: true });
          if (!open || mine !== opening) return over;
          if (over.ok) {
            typed = '';
            clearField();
            setSaid(savedSaid(over.name ?? name, over.outcome));
          } else failed(over.error);
          return over;
        },
        onClose: close,
      }), false);
    } else failed(answer.error);
  }

  frame.el.addEventListener('click', (event) => {
    if (!open) return;
    const button = event.target.closest?.('button[data-act]');
    if (!button || !frame.el.contains(button)) return;
    const act = button.dataset.act;
    if (act === 'recall') {
      const name = sceneOf(button);
      if (name !== null) recall(name, button);
    } else if (act === 'rename') askRename(button);
    else if (act === 'delete') askDelete(button);
  });

  frame.el.addEventListener('input', (event) => {
    if (open && event.target?.id === IDS.field) typed = event.target.value;
  });

  frame.el.addEventListener('submit', (event) => {
    if (!open || event.target?.id !== IDS.save) return;
    event.preventDefault();
    const field = doc.getElementById(IDS.field);
    if (!field || !field.reportValidity()) return;
    save(field.value);
  });

  return {
    show() {
      if (!open) {
        open = true;
        opening += 1;
        aborter = new AbortController();
        stop = scenes.watch(refresh);
        said = null;
        typed = '';
        recalling = null;
        asking = null;
        inList = false;
      }
      hadView = Boolean(scenes.view());
      draw();
    },
    hide() {
      if (!open) return;
      open = false;
      stop?.();
      stop = null;
      aborter.abort();
      asking = null;
      inList = false;
      said = null;
      recalling = null;
    },
    onKey: (event) => Boolean(asking?.key(event)),
  };
}
