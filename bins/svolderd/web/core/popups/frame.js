/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What every popup shares: the titled box with its close button, the side list beside the page
// (one of the two at a time on a phone), the id its title is known by, and a redraw that keeps
// the focus. A popup is three files: `popups/<name>/draw.js` (model in, string out), a thin
// `popups/<name>/wire.js` (`wire({ core, frame })` gives `{ show(page), hide(), onKey(event) }`)
// and this frame, which the wire draws into. popups.js makes a frame for each popup it is given
// and loads the wire when the popup is first opened.
//
// The markup is a string (`popupHtml`), dressed by the `fp-*` classes of frame.css, which read
// only the `--frame-*` tokens: a popup has no colour of its own. The box says where the router
// is: `fp-at-list` at the popup's list (page `null`) and `fp-at-page` at a page. A wide window
// shows the side list and the page side by side either way (at the list, the first entry's
// page); at the phone width (`PHONE_MAX_PX`) the popup fills the screen and shows the list or
// the page, never both, with a button at the top of a page that goes back to the list. What a
// layout does not show (that button in a wide window, a pane on a phone) is no stop of the
// stack's Tab, which goes round by the controls that are rendered. The page pane is one of those
// stops while it holds more than it shows, so a page with no control scrolls by the keyboard.
//
// Keys: the frame attaches no key listener. The modal stack owns the keyboard and hands each key
// to the wire's `onKey`; the frame's own controls are buttons, which the browser clicks on Enter
// and Space.

import { attr, esc } from '../html.js';
import { FRAME_CLASSES, closeAsk } from '../inline-ask.js';
import { closeList, openerIn } from '../list.js';
import { claim, REACH } from '../modal.js';

/** The words of the frame's own controls and states. */
export const POPUP_WORDS = {
  close: 'Close',
  back: 'Back to the list',
  loading: 'Loading…',
  failed: 'This could not be loaded. Close it and open it again.',
};

/** The id of the popup `name`'s element. */
export const popupId = (name) => `popup-${name}`;
/** The id of its title: the modal's `labelledBy`. */
export const titleId = (name) => `popup-${name}-title`;
/** The id of its close button. */
export const closeId = (name) => `popup-${name}-close`;
/** The id of the button that goes back from a page to the list. */
export const backId = (name) => `popup-${name}-back`;
/** The id of the place of the header's tools. */
export const headId = (name) => `popup-${name}-head`;
/** The id of the place of the strip under the header. */
export const barId = (name) => `popup-${name}-bar`;
/** The id of the page pane of the popup `name` while its bar holds tabs: what a tab controls. */
export const popupPanelId = (name) => `popup-${name}-page`;
/** The id of the side list's entry `id`. */
export const entryId = (name, id) => `popup-${name}-entry-${id}`;

/** The entry whose page the page pane holds: the router's `page`, else the first entry's. */
export const currentOf = (entries, page) => page ?? entries[0]?.id ?? null;

// The pieces every popup draws, as strings. Each has one class of frame.css, dressed by the
// `--frame-*` tokens alone, so a popup's draw names no colour and no size of its own.

/** A key's name as a keycap. */
export const keycap = (name) => `<kbd class="fp-key">${esc(name)}</kbd>`;

/** A state dot: `'on'` (connected, fine), `'warn'` (to mind), anything else off. With `words` a
 * screen reader reads them; without, the dot is an ornament beside words that say the same. */
export function dot(state, words = null) {
  const kind = state === 'on' || state === 'warn' ? state : 'off';
  const said = words === null ? ' aria-hidden="true"' : ` role="img"${attr('aria-label', words)}`;
  return `<span class="fp-dot ${kind}"${said}></span>`;
}

/** A tag: one or two words in a pill (`Shown`). */
export const tag = (words) => `<span class="fp-tag">${esc(words)}</span>`;

/** The heading of a section of a page. */
export const heading = (words) => `<h3 class="fp-heading">${esc(words)}</h3>`;

/**
 * Rows as one list of terms and what each says (a description list): each of `list` is `{ label,
 * value, attrs? }`, the label and the value both markup the popup's draw has made safe (a
 * keycap, a link), and `attrs` further attributes of the row, made with `attr`.
 */
export function rows(list) {
  return `<dl class="fp-rows">${list.map((one) => (
    `<div class="fp-row"${one.attrs ?? ''}>`
    + `<dt class="fp-row-label">${one.label}</dt><dd class="fp-row-value">${one.value}</dd></div>`
  )).join('')}</dl>`;
}

/** One entry of the side list, the current one (`at`) marked. A break stands before an entry
 * that starts a group, but for the first. */
function entryHtml(name, entry, at, first) {
  const on = entry.id === at;
  return `${entry.group && !first ? '<hr class="fp-rule">' : ''}`
    + `<button type="button" class="fp-entry${on ? ' on' : ''}"`
    + `${attr('id', entryId(name, entry.id))}${attr('data-popup-page', entry.id)}`
    + `${attr('aria-current', on ? 'page' : null)}>`
    + `${entry.dot === undefined ? '' : dot(entry.dot, entry.dotWords ?? null)}${esc(entry.label)}`
    + `${entry.tag === undefined ? '' : tag(entry.tag)}</button>`;
}

/** What the side list of the popup `name` holds: its `entries`, the one of `page` marked. */
export function sideHtml(name, entries, page = null) {
  const at = currentOf(entries, page);
  return entries.map((entry, i) => entryHtml(name, entry, at, i === 0)).join('');
}

/**
 * The box of the popup `name`, as markup: its head (the `title`, the close button), then the side
 * list of `entries` (none: no list) and the page pane holding `body`, which is markup the
 * popup's draw has made safe. An entry is `{ id, label, dot?, dotWords?, tag?, group? }`: beside
 * its label it may carry a state dot (`dot`: `'on'`, `'warn'` or `'off'`, with `dotWords` for a
 * screen reader when the label does not say the state), a tag after the label (`tag`: its
 * words) and a break before it (`group: true`: it starts a group). The pane is the popup's
 * place for lists (list.js): a list opened from a control in it puts its panel there, with no
 * `within` to name; the head is no such place, and holds no control but the close button. `page` is
 * the router's (`null`: the list); at a page the pane opens with the button back to the list,
 * which only the phone layout shows. `state` is `'ready'`, or `'loading'` or `'failed'`, which
 * put the frame's own line in the page pane in place of `body`. `head` is markup for the header's
 * tools, between the title and the close button (a switch), and `bar` markup for a strip under the
 * header that does not scroll with the page (tabs); each is in a place of its own, `headId(name)`
 * and `barId(name)`, drawn only when it is not empty.
 */
export function popupHtml({
  name, title, entries = [], page = null, body = '', state = 'ready', words = POPUP_WORDS,
  head = '', bar = '',
}) {
  const where = entries.length === 0 ? 'fp-single' : (page === null ? 'fp-at-list' : 'fp-at-page');
  const side = entries.length === 0 ? ''
    : `<nav class="fp-side"${attr('aria-label', title)}>${sideHtml(name, entries, page)}</nav>`;
  const inside = {
    ready: body,
    loading: `<p class="fp-note" role="status">${esc(words.loading)}</p>`,
    failed: `<p class="fp-note fp-failed" role="alert">${esc(words.failed)}</p>`,
  }[state] ?? body;
  // only a page entered from a list has a way back to it; a wide window does not show it
  const back = where === 'fp-at-page'
    ? `<button type="button" class="fp-back"${attr('id', backId(name))} data-popup-back>`
      + `‹ ${esc(words.back)}</button>`
    : '';
  return `<div class="fp-box ${where}"${attr('data-popup', name)}${attr('data-state', state)}>`
    + '<header class="fp-head">'
    + `<h2 class="fp-title"${attr('id', titleId(name))}>${esc(title)}</h2>`
    + (head === '' ? '' : `<div class="fp-head-tools"${attr('id', headId(name))}>${head}</div>`)
    + `<button type="button" class="fp-close"${attr('id', closeId(name))} data-popup-close`
    + `${attr('aria-label', words.close)}>×</button>`
    + '</header>'
    + (bar === '' ? '' : `<div class="fp-bar"${attr('id', barId(name))}>${bar}</div>`)
    + `<div class="fp-body">${side}<section class="fp-page" data-list-place>${back}${inside}`
    + '</section></div>'
    + '</div>';
}

/** Give `el` the focus; whether it took it (one that is not rendered does not). */
function give(doc, el) {
  if (!el) return false;
  el.focus?.({ preventScroll: true });
  return doc.activeElement === el;
}

/**
 * Put `html` into `place` and keep the focus: when it was inside `place`, it goes to the element
 * that has the same id afterwards, and when there is none or it takes no focus (it is not
 * rendered), to the first of `fallback` that takes it, so the focus never falls onto the page
 * under the popup. `fallback` is an element, or a function that gives the choices in order, asked
 * after the redraw. A focus outside `place` is left alone.
 */
export function redraw(place, html, fallback = null) {
  const doc = place.ownerDocument;
  const at = doc.activeElement;
  const had = Boolean(at) && at !== place && place.contains(at);
  const id = had ? at.id : '';
  place.innerHTML = html;
  if (!had) return;
  const twin = id ? doc.getElementById(id) : null;
  if (twin && place.contains(twin) && give(doc, twin)) return;
  const choices = typeof fallback === 'function' ? fallback() : [fallback];
  choices.some((choice) => give(doc, choice));
}

/**
 * The frame of the popup `name` in `document`: its element and what draws into it. `router` is
 * the core's: the close button and a click beside the box (pressed and let go on the veil) close
 * the popup, an entry of the side list steps to its page and the back button to the list.
 * `title` and `entries` are the title and the side list as far as they are known before the
 * popup's first draw. Returns
 * `{ el, name, labelledBy, focus(), state(), draw({ title, entries, page, body }), put(id, html),
 * list(entries), loading(page), failed(page) }`: `focus()` is the control the focus opens on,
 * `draw` draws the whole box, `put` one place inside it by its id and `list` the side list
 * alone, all keeping the focus. A `put` of what the place holds already changes nothing, so a
 * control in it keeps its element and the focus; `list` leaves the page pane as it stands, with
 * what is typed, asked or scrolled there. A draw that names
 * a `title` changes it from then on (a popup whose title names what it is about), and one
 * that names `entries` changes the side list; a draw that names neither keeps both. `draw`,
 * `loading` and `failed` close the inline ask and the list that are open in the popup, and so does
 * a `put` that replaces a place holding the list's opener or an ask.
 * `titleWords(page)` is what the browser tab's title says of the popup at
 * `page`: its title, then at a page the label of that page's entry. `drawn()`, if given, is
 * called after every draw of the whole box (popups.js keeps the tab's title in step with it).
 */
export function createPopupFrame({
  document, router, name, title: first, entries: known = [], words = POPUP_WORDS,
  drawn = () => {},
}) {
  const el = document.createElement('div');
  el.id = popupId(name);
  el.className = 'fp';
  // the element takes the focus when nothing inside it can
  el.tabIndex = -1;
  // an ask or a list drawn in it is the popup's from now on, before the stack has it
  claim(el);
  /** What the side list held at the last draw, and before the first one the entries the popup
   * was added with: the loading and failed states draw it, so the box does not change its shape
   * when the popup's module arrives. */
  let entries = known;
  /** The title of the last draw that named one; before that, the one the popup was added with. */
  let title = first;
  /** The page of the last draw of the whole box. */
  let at = null;

  /** Whether the pointer's last press, and its release, were on the veil. A drag that starts in
   * the box (text selected in a field) and ends on the veil, or the other way round, is a click
   * on the veil (the two ends' common ancestor) with one end in the box. */
  let pressedVeil = false;
  let releasedVeil = false;
  el.addEventListener('pointerdown', (event) => {
    pressedVeil = event.target === el;
    releasedVeil = false;
  });
  el.addEventListener('pointerup', (event) => {
    releasedVeil = event.target === el;
  });

  el.addEventListener('click', (event) => {
    const target = event.target;
    // the veil closes the popup when the press and the release were both on it
    const veil = target === el && pressedVeil && releasedVeil;
    pressedVeil = false;
    releasedVeil = false;
    if (veil || target.closest?.('[data-popup-close]')) {
      router.close();
      return;
    }
    if (target.closest?.('[data-popup-back]')) {
      router.step(null);
      return;
    }
    const entry = target.closest?.('[data-popup-page]');
    if (!entry) return;
    // At the list a wide window already shows the first entry's page in the pane, and a popup with
    // no side list shows the page of the tab pressed: a step to it would push an entry of the
    // history that changes nothing a person sees.
    const there = entry.classList.contains('on') && (Boolean(entry.closest('.fp-single'))
      || (Boolean(entry.closest('.fp-at-list')) && shown(el.querySelector('.fp-page'))));
    if (!there) router.step(entry.dataset.popupPage);
  });

  /** Whether `control` is drawn: the phone layout does not show the side list at a page. Where
   * the browser cannot say, it is taken to be. */
  function shown(control) {
    return Boolean(control) && (control.checkVisibility?.() ?? true);
  }
  /** Whether Tab reaches `control`, as the stack reads it (modal.js): not hidden, not under
   * anything inert, and visible. */
  const tabbed = (control) => !control.closest('[hidden], [inert]')
    && (control.checkVisibility?.({ visibilityProperty: true }) ?? true);
  /** Where the focus may go inside the popup, the best first: the current entry of the side
   * list, the way back from a page, the close button, the popup itself. */
  const places = () => [
    el.querySelector('.fp-tab.on'), el.querySelector('.fp-entry.on'),
    el.querySelector('[data-popup-back]'), el.querySelector('[data-popup-close]'),
  ].filter(shown).concat(el);

  /** The page pane is a stop of the keyboard while it holds more than it shows and has no
   * control in it: a page of words alone (a list of keys) could not be scrolled without a
   * pointer otherwise, and a pane with a control that is shown scrolls from that control, so it
   * gets no stop of its own. It is then a region named after the entry whose page it holds,
   * else after the title. Any other pane is no stop, unless it has the focus, which it keeps.
   * It is looked at after each draw and `put` and when the box changes size (the window, the
   * phone layout, a page of another length); what grows inside the pane by other means, the box
   * staying as it is, is seen at the next of those. */
  function reach() {
    const pane = el.querySelector('.fp-page');
    if (!pane) return;
    const controls = [...pane.querySelectorAll(REACH)].some(tabbed);
    if (pane.scrollHeight > pane.clientHeight + 1 && !controls) {
      pane.tabIndex = 0;
      pane.setAttribute('role', 'region');
      pane.setAttribute('aria-labelledby', el.querySelector('.fp-entry.on')?.id ?? titleId(name));
    } else if (document.activeElement !== pane) {
      for (const mark of ['tabindex', 'role', 'aria-labelledby']) pane.removeAttribute(mark);
      // Firefox makes a scroll container a stop of Tab of its own when it sits after a bar of
      // controls; a popup with a bar says plainly that the pane is no stop
      if (el.querySelector('.fp-bar')) pane.tabIndex = -1;
    }
    // a bar of tabs (`.fp-tab`, `role="tab"`) makes the pane the panel of the tab shown, which the
    // tabs name by `aria-controls` (the pane's id is `popupPanelId`)
    const shown = el.querySelector('.fp-bar .fp-tab.on');
    if (shown) {
      pane.id = popupPanelId(name);
      pane.setAttribute('role', 'tabpanel');
      pane.setAttribute('aria-labelledby', shown.id);
    } else if (pane.id === popupPanelId(name)) {
      pane.removeAttribute('id');
      // the panel's marks only: a region this call just set keeps its role and name
      if (pane.getAttribute('role') === 'tabpanel') {
        pane.removeAttribute('role');
        pane.removeAttribute('aria-labelledby');
      }
    }
  }
  // A caret left on the page under the popup (a click on its words leaves one) is no place to
  // move from: with one there, Firefox takes End or Home on the focused pane for the caret's,
  // drops the pane's focus and scrolls nothing. It goes as the pane takes the focus; words that
  // are selected, and a caret inside the popup, stay. An input's caret is the input's own and
  // is not this one; a caret in an editable element of the page would go too (no theme has one).
  el.addEventListener('focusin', (event) => {
    if (!event.target.classList?.contains('fp-page')) return;
    const selection = document.defaultView?.getSelection?.();
    if (selection?.isCollapsed && selection.rangeCount > 0 && !el.contains(selection.anchorNode)) {
      selection.removeAllRanges();
    }
  });
  /** The box's size changes with the window's and with what the pane holds: the pane is looked
   * at again. Where the browser has no such observer, a draw alone looks. */
  let watching = null;
  try {
    watching = new (document.defaultView.ResizeObserver)(() => reach());
  } catch {
    // no observer: `reach` runs after each draw
  }
  let watched = null;

  /** Whether `place` holds `html` already, as the browser would parse it. */
  function holds(place, html) {
    const parsed = document.createElement('template');
    parsed.innerHTML = html;
    return parsed.innerHTML === place.innerHTML;
  }

  /** Draw the whole box. An inline ask or a list open in it closes first: its markup goes with
   * the box, and one left held would take the next Escape for itself. The focus it had stays in
   * the popup. The box is one element through every draw (its classes, its state and what it
   * holds are drawn again, the element stays), so a theme's entry animation runs once each time
   * the popup enters the page: the part arriving after the loading line, a step and a redraw
   * neither cut it nor start it again. */
  function paint(model) {
    const held = el.contains(document.activeElement);
    closeAsk(el);
    closeList(el);
    const html = popupHtml({ name, title, words, ...model });
    const box = el.firstElementChild;
    if (box === null) {
      redraw(el, html, places);
    } else {
      const parsed = document.createElement('template');
      parsed.innerHTML = html;
      const next = parsed.content.firstElementChild;
      for (const { name: attribute, value } of next.attributes) box.setAttribute(attribute, value);
      redraw(box, next.innerHTML, places);
    }
    if (held && !el.contains(document.activeElement)) places().some((to) => give(document, to));
    if (watched !== el.firstElementChild) {
      watched = el.firstElementChild;
      watching?.observe(watched);
    }
    reach();
    drawn();
  }

  return {
    el,
    name,
    labelledBy: titleId(name),
    /** Where the focus opens: the current entry of the side list, else (the list is not shown: a
     * phone at a page) the way back, else the close button. */
    focus: () => places()[0],
    /** The popup's words for the tab's title at `page`: its title alone at the list, with the
     * page's label at a page whose entry is known. */
    titleWords(page = null) {
      const label = page === null ? null : entries.find((entry) => entry.id === page)?.label;
      return label ? `${title} · ${label}` : title;
    },
    /** What the box shows: `'ready'`, `'loading'` or `'failed'`; `null` before the first draw. */
    state: () => el.querySelector('.fp-box')?.dataset.state ?? null,
    draw({
      title: named, entries: list = entries, page = null, body = '', head = '', bar = '',
    }) {
      if (named !== undefined) title = named;
      entries = list;
      at = page;
      paint({
        entries, page, body, head, bar,
      });
    },
    put(id, html) {
      const place = document.getElementById(id);
      if (place && el.contains(place) && !holds(place, html)) {
        // a list whose opener is in the place, and an ask drawn in it (or its place), go with it
        const held = el.contains(document.activeElement);
        if (openerIn(place)) closeList(el);
        if (place.querySelector(`.${FRAME_CLASSES.box}`)) closeAsk(el);
        redraw(place, html, places);
        // a focused option or answer went with its panel: the focus stays in the popup
        if (held && !el.contains(document.activeElement)) places().some((to) => give(document, to));
      }
      reach();
    },
    /** Draw the side list alone, with `list` as its entries from now on: the page pane, an ask or
     * a list open in it and what is typed there stay. A box drawn with no side list (or not
     * drawn yet) keeps the entries for its next draw. */
    list(list) {
      entries = list;
      const side = el.querySelector('.fp-side');
      if (side) {
        const html = sideHtml(name, entries, at);
        if (!holds(side, html)) redraw(side, html, places);
      }
      reach();
      drawn();
    },
    loading(page = null) {
      at = page;
      paint({ entries, page, state: 'loading' });
    },
    failed(page = null) {
      at = page;
      paint({ entries, page, state: 'failed' });
    },
  };
}
