/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The Sound popup's own markup as strings: the tab strip (EQ library, Loudness, Crossfeed, Virtual
// speakers, the order of the filter chain; Loudness only while the host offers it) and, on the EQ
// library tab only, the pill bar of that tab's three pages. Both sit in the frame's bar under the
// header, which does not scroll with the page. A tab or a pill is a button with
// `data-popup-page`, so the frame's click steps the router to its page; the router's page is
// `loudness`, `crossfeed` and `speakers` for their tabs and the EQ library's own page word for the
// other (`autoeq`, `upload`, `saved`; none is `autoeq`).

import { attr, esc } from '../../html.js';
import { CROSSFEED, LOUDNESS, SPEAKERS } from '../listed.js';
import { ENTRIES } from '../eq-library/draw.js';
import { dot, popupPanelId } from '../frame.js';

/** The popup's title. */
export const TITLE = 'Sound';

/** The words of the strip. */
export const WORDS = {
  tabs: 'Sound',
  eq: 'EQ library',
  loudness: 'Loudness',
  crossfeed: 'Crossfeed',
  speakers: 'Virtual speakers',
  pills: 'EQ library pages',
  loudnessOn: 'on',
  crossfeedOn: 'on',
  speakersOn: 'on',
};

/** The id of the tab `tab` (`eq`, `ld`, `xf` or `sp`). */
export const tabId = (tab) => `popup-sound-tab-${tab}`;
/** The id of the EQ library page `id`'s pill. */
export const pillId = (id) => `popup-sound-pill-${id}`;
/** The id of the line that names what the EQ library tab is about. */
export const CONTEXT = 'popup-sound-context';

/** The tab a router page belongs to: `ld`, `xf`, `sp` or `eq`. */
export function tabOf(page) {
  if (page === LOUDNESS) return 'ld';
  if (page === CROSSFEED) return 'xf';
  return page === SPEAKERS ? 'sp' : 'eq';
}

/** The EQ library's page for a router page: its own, and `autoeq` for none. */
export const eqPage = (page) => (ENTRIES.some((one) => one.id === page) ? page : ENTRIES[0].id);

/** The tab strip. `page` is the router's, `eqAt` the EQ library page its tab opens at, `loudnessOn`
 * whether loudness plays (`null`: the host offers none, and the tab is left out unless it is the
 * one shown), `on` whether crossfeed plays and `speakersOn` whether the virtual speakers do (a dot
 * on the tab, with the word for a screen reader). */
export function tabsHtml({
  page, eqAt = ENTRIES[0].id, loudnessOn = null, on = null, speakersOn = null,
}) {
  const at = tabOf(page);
  const tab = (id, to, label, extra = '') => `<button type="button" role="tab" class="fp-tab${at === id ? ' on' : ''}"`
    + `${attr('id', tabId(id))}${attr('data-popup-page', to)}`
    + `${attr('aria-selected', String(at === id))}${attr('aria-controls', popupPanelId('sound'))}>`
    + `${esc(label)}${extra}</button>`;
  return `<div class="fp-tabs" role="tablist"${attr('aria-label', WORDS.tabs)}>`
    + `${tab('eq', eqAt, WORDS.eq)}`
    + `${loudnessOn === null && at !== 'ld' ? ''
      : tab('ld', LOUDNESS, WORDS.loudness, loudnessOn === true ? dot('on', WORDS.loudnessOn) : '')}`
    + `${tab('xf', CROSSFEED, WORDS.crossfeed, on === true ? dot('on', WORDS.crossfeedOn) : '')}`
    + `${tab('sp', SPEAKERS, WORDS.speakers, speakersOn === true ? dot('on', WORDS.speakersOn) : '')}`
    + '</div>';
}

/** The pill bar of the EQ library tab: its pages, the current one marked. */
export function pillsHtml(page) {
  const at = eqPage(page);
  return `<div class="fp-pills" role="group"${attr('aria-label', WORDS.pills)}>`
    + ENTRIES.map((entry) => `<button type="button" class="fp-pill${entry.id === at ? ' on' : ''}"`
      + `${attr('id', pillId(entry.id))}${attr('data-popup-page', entry.id)}`
      + `${attr('aria-current', entry.id === at ? 'page' : null)}>${esc(entry.label)}</button>`)
      .join('')
    + '</div>';
}

/** The whole bar for `page`: the tab strip, and the pills on the EQ library tab. */
export const barHtml = (opts) => tabsHtml(opts)
  + (tabOf(opts.page) === 'eq' ? pillsHtml(opts.page) : '');
