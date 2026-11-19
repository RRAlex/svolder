/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Sound popup's wire: a strip of four tabs, EQ library, Loudness, Crossfeed and Virtual
// speakers (the order of the filter chain), over the four parts that already draw into a popup's
// frame. The EQ library part (popups/eq-library) is today's popup, drawn into a view of the frame
// that adds the tab strip, the pill bar of its three pages and one line naming what its page is
// about; the other three parts (popups/loudness, popups/crossfeed, popups/speakers) draw into
// views of their own. A view only draws while its tab is the one shown, so the part of a tab that
// is not shown cannot paint over the other: the EQ library part is not closed when the person
// goes to another tab (its search, its preview and what is typed stay and are drawn again on the
// way back), the other three are.
//
// The router's page says the tab: `loudness`, `crossfeed` and `speakers` are their tabs, any
// other (an EQ library page or none) the EQ library's. The tab strip and the pills are buttons
// with `data-popup-page`, so the frame's click steps the router. Left and Right on a tab move to
// the next or the previous tab.

import { CROSSFEED, LOUDNESS, SPEAKERS } from '../listed.js';
import { wire as speakersWire } from '../speakers/wire.js';
import { wire as crossfeedWire } from '../crossfeed/wire.js';
import { wire as loudnessWire } from '../loudness/wire.js';
import { wire as eqWire } from '../eq-library/wire.js';
import { barId } from '../frame.js';
import { esc } from '../../html.js';
import {
  CONTEXT, TITLE, barHtml, eqPage, tabId, tabOf,
} from './draw.js';

/** The Sound popup's part for the router: `{ show(page), hide(), onKey(event) }`, over `core`,
 * drawn into `frame` (popups/frame.js). */
export function wire({ core, frame }) {
  const doc = frame.el.ownerDocument;
  const store = core.model;
  /** The tab shown (`eq`, `xf` or `sp`), the router's page and the EQ library page the EQ tab
   * opens at. */
  let current = null;
  let page = null;
  let eqAt = eqPage(null);
  let open = false;
  let offs = [];

  const loudnessOn = () => {
    const ld = store.page().host?.computer?.eq?.loudness;
    return ld ? ld.on === true : null;
  };
  const crossfeedOn = () => {
    const cf = store.page().host?.computer?.eq?.crossfeed;
    return cf ? cf.on === true : null;
  };
  const speakersOn = () => {
    const sp = store.page().host?.computer?.eq?.speakers;
    return sp ? sp.on === true : null;
  };
  const bar = () => barHtml({
    page, eqAt, loudnessOn: loudnessOn(), on: crossfeedOn(), speakersOn: speakersOn(),
  });

  /** A view of the frame for the part of `tab`: what it draws gets the strip and the title of the
   * popup, and nothing it draws shows while the other tab is the one shown. */
  function viewFor(tab) {
    const live = () => open && current === tab;
    return {
      el: frame.el,
      name: frame.name,
      state: () => frame.state(),
      focus: () => frame.focus(),
      draw(model) {
        if (!live()) return;
        const context = tab === 'eq'
          ? `<p class="fp-note fpx-context" id="${CONTEXT}">${esc(model.title ?? '')}</p>` : '';
        frame.draw({
          title: TITLE, page, bar: bar(), head: model.head ?? '', body: `${context}${model.body}`,
        });
      },
      put(id, html) {
        if (live()) frame.put(id, html);
      },
      list() {},
      loading: () => {},
      failed: () => {},
    };
  }

  const parts = {
    eq: eqWire({ core, frame: viewFor('eq') }),
    ld: loudnessWire({ core, frame: viewFor('ld') }),
    xf: crossfeedWire({ core, frame: viewFor('xf') }),
    sp: speakersWire({ core, frame: viewFor('sp') }),
  };

  /** Scroll the tab strip (it scrolls on a phone) so that `tab`, the one shown by default, is in
   * view; the page behind and the popup's own pane are not moved. */
  function reveal(tab = frame.el.querySelector('.fp-tab.on')) {
    const strip = tab?.parentElement;
    if (!strip?.classList?.contains('fp-tabs')) return;
    const from = tab.offsetLeft;
    const to = from + tab.offsetWidth;
    if (from < strip.scrollLeft) strip.scrollLeft = Math.max(from - 8, 0);
    else if (to > strip.scrollLeft + strip.clientWidth) strip.scrollLeft = to - strip.clientWidth + 8;
  }

  /** The strip again, when what it says changed (a switch of loudness, crossfeed or speakers). */
  function putBar() {
    if (!open) return;
    frame.put(barId(frame.name), bar());
    reveal();
  }

  return {
    show(to) {
      const tab = tabOf(to);
      page = to;
      if (tab === 'eq') eqAt = eqPage(to);
      if (!open) {
        open = true;
        offs = [store.on('host', putBar), store.on('model', putBar), core.computer.on(putBar)];
      }
      for (const other of ['ld', 'xf', 'sp']) if (current === other && tab !== other) parts[other].hide();
      current = tab;
      parts[tab].show(to);
      reveal();
    },
    hide() {
      if (!open) return;
      parts.eq.hide();
      parts.ld.hide();
      parts.xf.hide();
      parts.sp.hide();
      open = false;
      current = null;
      for (const off of offs) off();
      offs = [];
    },
    onKey(event) {
      if (parts[current]?.onKey(event)) return true;
      const onTab = doc.activeElement?.closest?.('.fp-tabs');
      if (!onTab || (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight')) return false;
      const order = [eqAt, ...(loudnessOn() === null && current !== 'ld' ? [] : [LOUDNESS]),
        CROSSFEED, SPEAKERS];
      const at = order.findIndex((one) => tabOf(one) === current);
      const to = order[(at + (event.key === 'ArrowRight' ? 1 : -1) + order.length) % order.length];
      event.preventDefault();
      core.router.step(to);
      const next = doc.getElementById(tabId(tabOf(to)));
      next?.focus({ preventScroll: true });
      reveal(next);
      return true;
    },
  };
}
