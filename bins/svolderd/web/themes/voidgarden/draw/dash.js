/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The dash's markup. On a desktop two small glass capsules in the bottom corners: the left one holds
// the side switch (the device's chip and `This computer`), `−` the volume figure `+`, the mute key
// and the input as one chip that opens a list; the right one the cover and the track, the transport
// and, in amber, the explore group (New seed, Next scene, Effect, each with a two-line tooltip). On
// a phone (the theme's PHONE_QUERY) the features chip, one capsule (`−` figure `+`, play or pause,
// `⋯`) and the sheet `⋯` opens: the device, the computer and the input; the cover and the whole
// title; the transport and mute; the three explore actions named, on one line. A viewer who may not
// write gets the same keys drawn inert: a tap still reaches the core, which says why nothing
// happens. `Controls…` is a row of the frame's menu. Pure: the page model in, markup out.

import { esc } from '../../../core/html.js';
import { inertOf } from '../states.js';

/** The dash's fixed words. */
export const DASH = Object.freeze({
  label: 'Controls',
  quieter: 'Quieter',
  louder: 'Louder',
  mute: 'MUTE',
  muteLabel: 'Mute',
  input: 'Input',
  previous: 'Previous',
  next: 'Next',
  pause: 'Pause',
  play: 'Play',
  more: 'More controls',
  features: 'features',
  featuresLabel: 'Audio features',
  sheet: 'More controls',
  explore: 'Explore',
  nothing: 'Nothing playing',
  noMpris: 'No MPRIS player',
  silence: '−∞',
  unit: 'dB',
  sideLabel: 'Show the controls of',
  computer: 'This computer',
});

/** The transport's icons: the paths of a 16 px box, filled with the key's own colour. */
const ICONS = {
  previous: '<path d="M3 2h2v12H3zM14 2v12L6 8z"/>',
  pause: '<path d="M3 2h4v12H3zM9 2h4v12H9z"/>',
  play: '<path d="M4 2v12l9-6z"/>',
  next: '<path d="M11 2h2v12h-2zM2 2v12l8-6z"/>',
};
/** The line icons (a 24 px box, stroked in the key's own colour), as the approved mockup draws them. */
const LINES = {
  minus: 'M6 12h12',
  plus: 'M6 12h12M12 6v12',
  mute: 'M4 10h4l5-4v12l-5-4H4zM17 9l4 6M21 9l-4 6',
  seed: 'M20 12a8 8 0 1 1-3-6.2M20 4v5h-5',
  scene: 'M4 18l6-8 4 5 2-3 4 6zM16 7a2 2 0 1 0 0.01 0',
  effect: 'M12 3v5M12 16v5M3 12h5M16 12h5M6 6l3 3M15 15l3 3M18 6l-3 3M9 15l-3 3',
  bars: 'M5 18v-6M10 18V8M15 18v-9M20 18v-4',
  more: 'M6 12h.01M12 12h.01M18 12h.01',
};
const icon = (name) => (LINES[name]
  ? '<svg class="vg-ic vg-sv" viewBox="0 0 24 24" width="18" height="18" fill="none"'
    + ' stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"'
    + ` aria-hidden="true" focusable="false"><path d="${LINES[name]}"/></svg>`
  : '<svg class="vg-ic" viewBox="0 0 16 16" width="16" height="16"'
    + ` fill="currentColor" aria-hidden="true" focusable="false">${ICONS[name]}</svg>`);

/** A key of the dash. `act` is its `data-act`, `attrs` the rest (escaped, each with a leading
 * space), `cls` its extra classes; `markup` says `text` is markup the caller drew (an icon). */
function key(act, text, { inert, attrs = '', cls = '', markup = false }) {
  const classes = `vg-k${cls ? ` ${cls}` : ''}${inert ? ' quiet' : ''}`;
  return `<button type="button" class="${classes}" data-act="${act}"${attrs}`
    + `${inert ? ' aria-disabled="true"' : ''}>${markup ? text : esc(text)}</button>`;
}

/**
 * The volume as the dash says it: `text` (the figure and its unit, `−23.5 dB`, `−∞ dB` while the
 * device is muted), the `aria` words and whether it is `muted`; `null` for a side with no volume.
 * With the computer as the side shown (`comp`, the core's `computer.view()`) it is the computer's:
 * `42 %`, the figure kept while muted, `—` before the first read.
 */
export function volumeWords(page, comp) {
  if (comp?.side === 'computer') {
    const c = comp.volume;
    const text = c.pct === null ? '—' : `${c.text} ${c.unit}`;
    return { text, aria: c.aria, muted: c.muted, mute: true, inert: c.inert };
  }
  const v = page?.device?.volume;
  if (!v) return null;
  const unit = v.unit || DASH.unit;
  const muted = Boolean(v.muted);
  const said = String(v.text ?? '').trim();
  const cut = said.endsWith(unit) ? said.slice(0, -unit.length).trim() : said;
  const figures = muted ? DASH.silence : cut.replace(/^-/, '−');
  return {
    text: `${figures} ${unit}`.trim(),
    aria: muted
      ? `muted, minus infinity ${unit}`
      : `Volume ${figures.replace('−', 'minus ')} ${unit}`.trim(),
    muted,
    mute: v.mute ? (typeof v.mute === 'string' ? v.mute : true) : false,
    inert: false,
  };
}

/** `−`, the figure and `+`, together. */
function volumeGroup(v, inert) {
  if (!v) return '';
  const quiet = inert || v.inert;
  return `<span class="vg-vgrp">${key('volume-down', icon('minus'), {
    inert: quiet, markup: true, cls: 'vg-round', attrs: ` id="vg-vdown" aria-label="${DASH.quieter}"`,
  })}<span id="vg-vol" class="vg-vol${v.muted ? ' muted' : ''}" role="img"`
    + ` aria-label="${esc(v.aria)}">${esc(v.text)}</span>${key('volume-up', icon('plus'), {
      inert: quiet, markup: true, cls: 'vg-round', attrs: ` id="vg-vup" aria-label="${DASH.louder}"`,
    })}</span>`;
}

/** The mute key (an icon), where the side has one. */
function muteKey(v, inert) {
  if (!v?.mute) return '';
  return key('mute', icon('mute'), {
    inert: inert || v.inert,
    markup: true,
    cls: `vg-round${v.muted ? ' on' : ''}`,
    attrs: ` id="vg-mute"${typeof v.mute === 'string' ? ` data-key="${esc(v.mute)}"` : ''}`
      + ` aria-pressed="${v.muted}" aria-label="${DASH.muteLabel}"`,
  });
}

/** The input as one chip that opens a list (its current option and a caret); nothing for a device
 * with no input choice. */
function inputChip(device, inert) {
  const c = (device?.controls ?? []).find((x) => x.key === 'input' && x.kind === 'choice');
  const options = c?.options ?? [];
  if (options.length === 0) return '';
  const now = options.find((o) => o.on) ?? options[0];
  return key('input', `${now.label} ▾`, {
    inert,
    cls: 'vg-chip on',
    attrs: ' data-list="1" aria-haspopup="listbox" aria-expanded="false"'
      + ` aria-label="${esc(`${DASH.input}: ${now.label}`)}"`,
  });
}

/**
 * The side switch, two chips: the shown device's name and `This computer`, the side shown pressed;
 * where the computer cannot be used, the reason stands beside them. `sw` is the core's `switchOf`;
 * nothing for a page with none. The ids are the keys' own, so a keyboard flip keeps the focus.
 */
export function switchKeys(sw) {
  if (!sw) return '';
  const side = (name, label) => key('side', label, {
    inert: false,
    cls: `vg-chip${sw.side === name ? ' on' : ''}`,
    attrs: ` id="vg-side-${name}" data-side="${name}" aria-label="${esc(label)}"`
      + ` aria-pressed="${sw.side === name}"`,
  });
  const why = sw.unavailable
    ? '<span class="vg-why" role="status">'
      + `${esc(`computer controls unavailable: ${sw.unavailable}`)}</span>` : '';
  const keys = `${side('dac', sw.dac)}${side('computer', sw.computer)}`;
  return `<span class="vg-swkeys" role="group" aria-label="${DASH.sideLabel}">${keys}</span>${why}`;
}

/** The version of the cover whose picture failed to load, or null: the draws leave it out until a
 * draw sees another cover or none, which forgets the failure. */
let failedCover = null;

/** Remember that the picture of cover `version` failed to load (`null` forgets it). */
export function markCoverFailed(version) {
  failedCover = version ?? null;
}

/** The cover of the track, a small picture before the title; nothing where the host has none or
 * its picture failed to load. The address carries the cover's version. */
function coverMarkup(track) {
  if (String(track?.cover ?? '') !== failedCover) failedCover = null;
  const cover = track?.cover;
  if (cover === null || cover === undefined || String(cover) === failedCover) return '';
  const v = encodeURIComponent(track.cover);
  return '<span class="vg-cover" aria-hidden="true">'
    + `<img src="api/v1/host/cover?v=${v}" alt="" width="30" height="30"></span>`;
}

/** The track: its title, then the artist; the words for a page with no track. */
function trackMarkup(p) {
  if (!p) return '<div class="vg-track" id="vg-track"></div>';
  const track = p.track;
  if (!track) {
    const words = esc(p.path ? DASH.noMpris : DASH.nothing);
    return `<div class="vg-track" id="vg-track"><b class="vg-title">${words}</b></div>`;
  }
  const sub = [track.artist, track.album].filter(Boolean).map(esc).join(' · ');
  return `<div class="vg-track" id="vg-track"><b class="vg-title">${esc(track.headline)}</b>`
    + `${sub ? `<small class="vg-sub">${sub}</small>` : ''}</div>`;
}

/** One transport key: `act` is `prev`, `play` or `next`; the play key shows pause while it plays. */
function transportKey(act, status, inert, id, cls = 'vg-round') {
  const name = act === 'play' ? (status === 'playing' ? 'pause' : 'play')
    : act === 'prev' ? 'previous' : 'next';
  const label = { previous: DASH.previous, pause: DASH.pause, play: DASH.play, next: DASH.next }[name];
  return key(act, icon(name), {
    inert, markup: true, cls, attrs: ` id="${id}" data-button="${name}" aria-label="${label}"`,
  });
}

/** The transport for a player in `status`: previous, pause or play, next. The ids differ between
 * the capsule and the sheet, which are never both drawn on the same side of the phone rule except
 * the sheet's. */
function transport(status, inert, ids) {
  return transportKey('prev', status, inert, ids[0])
    + transportKey('play', status, inert, ids[1], 'vg-round vg-big')
    + transportKey('next', status, inert, ids[2]);
}

/** The explore keys: what each does, its key and its words (the tooltip's two lines). */
export const EXPLORE = Object.freeze([
  { kind: 'seed', name: 'New seed', short: '\u23e9\ufe0e Seed', key: 'S', words: 'A new world for this track' },
  { kind: 'scene', name: 'Next scene', short: '\u23e9\ufe0e Scene', key: 'N', words: 'Another world on the same track' },
  { kind: 'effect', name: 'Effect', short: 'Effect', key: 'E', words: 'Fire one of the scene’s effects' },
]);

/** One explore key. `tip` gives it the two-line tooltip (the desktop); `named` its short name (a fast-forward glyph
 * (text form) for New and Next) beside the icon (the phone's sheet). */
function exploreKey(e, { tip = false, named = false }) {
  const inner = icon(e.kind === 'effect' ? 'effect' : e.kind)
    + (named ? `<span class="vg-xn" aria-hidden="true">${esc(e.short)}</span>` : '')
    + (tip ? `<span class="vg-tip" role="tooltip" id="vg-tip-${e.kind}"><b>${esc(e.name)} `
      + `<kbd>${e.key}</kbd></b><span>${esc(e.words)}</span></span>` : '');
  return key('explore', inner, {
    inert: false,
    markup: true,
    cls: named ? 'vg-xl' : 'vg-round vg-xp',
    attrs: ` id="vg-ex-${e.kind}${named ? '-s' : ''}" data-kind="${e.kind}"`
      + ` aria-label="${esc(`${e.name} (${e.key})`)}"${tip ? ` aria-describedby="vg-tip-${e.kind}"` : ''}`,
  });
}

/**
 * The dash for `page`. `comp` is the core's `computer.view()`: with the computer as the side shown
 * its volume keys stand where the device's would. `opts.phone` draws the phone's parts instead of
 * the desktop's two capsules; `featOpen` and `sheetOpen` say which of the features panel and the
 * sheet is open. The player's transport only when the player has buttons to give (a guest is not
 * given them).
 */
export function drawDash(page, comp = null, {
  phone = false, featOpen = false, sheetOpen = false,
} = {}) {
  const device = page?.device ?? null;
  const inert = inertOf(page);
  const p = device?.playing ?? page?.host ?? null;
  const v = volumeWords(page, comp);
  const status = p?.track?.status;
  if (phone) {
    const play = p?.buttons
      ? transportKey('play', status, inert, 'vg-tr-1', 'vg-round vg-big') : '';
    const more = key('more', icon('more'), {
      inert: false,
      markup: true,
      cls: 'vg-round',
      attrs: ` id="vg-more" aria-haspopup="dialog" aria-expanded="${sheetOpen}"`
        + ` aria-label="${DASH.more}"`,
    });
    const chip = key('features', `${icon('bars')}${DASH.features}`, {
      inert: false,
      markup: true,
      cls: `vg-fchip${featOpen ? ' vg-gone' : ''}`,
      attrs: ` id="vg-fchip" aria-expanded="${featOpen}" aria-label="${DASH.featuresLabel}"`,
    });
    const rows = `<div class="vg-sh-row vg-sh-dev">${switchKeys(comp?.sw)}${inputChip(device, inert)}</div>`
      + `<div class="vg-sh-row vg-now">${coverMarkup(p?.track)}${trackMarkup(p)}</div>`
      + `<div class="vg-sh-row vg-sh-tr">${p?.buttons ? transport(status, inert, ['vg-tr-0', 'vg-tr-s', 'vg-tr-2']) : ''}`
      + `${muteKey(v, inert)}</div>`
      + `<div class="vg-sh-row vg-sh-ex">${EXPLORE.map((e) => exploreKey(e, { named: true })).join('')}</div>`;
    return `${chip}<div class="vg-cap vg-pcap">${volumeGroup(v, inert)}${play}${more}</div>`
      + '<div class="vg-sheet-back" data-act="sheet-close"></div>'
      + `<div class="vg-sheet" id="vg-sheet" role="dialog" aria-label="${DASH.sheet}">${rows}</div>`;
  }
  const left = switchKeys(comp?.sw) + volumeGroup(v, inert) + muteKey(v, inert)
    + inputChip(device, inert);
  const right = `${coverMarkup(p?.track)}${trackMarkup(p)}`
    + `${p?.buttons ? `<span class="vg-tr">${transport(status, inert, ['vg-tr-0', 'vg-tr-1', 'vg-tr-2'])}</span>` : ''}`
    + '<span class="vg-sep" aria-hidden="true"></span>'
    + `<span class="vg-explore" role="group" aria-label="${DASH.explore}">`
    + `${EXPLORE.map((e) => exploreKey(e, { tip: true })).join('')}</span>`;
  return `<div class="vg-cap vg-l vg-grp vg-keys">${left}</div>`
    + `<div class="vg-cap vg-r vg-now">${right}</div>`;
}
