/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hops the person declares: the sheet the `+` at the line's end opens (H6), the fields it shares
// with Edit, and the open area of a declared hop (P3: its name and note, where it sits, Edit, Move
// earlier / later, Remove). A hop that links a device edits its note only: its name and kind are the
// device's. Pure: the model in, markup out; the hands are in hops-wire.js. The whole `[[hop]]`
// list's order is the model's `fleet.hops`, so a move names the place in that list.

import { hopsOf } from '../line.js';
import { HOPS, KIND_WORDS } from '../words.js';
import { esc } from './esc.js';
import { button } from './word.js';

/** The longest a name or a note may be. */
export const NAME_MAX = 60;
export const NOTE_MAX = 120;

/** The fleet's amps that no hop links, in the fleet's order: the sheet offers them first. */
export function unlinkedAmps(page) {
  const linked = new Set((page.fleet?.hops ?? []).map((row) => row.device).filter(Boolean));
  return (page.fleet?.devices ?? []).filter((d) => d.family === 'amp' && d.url !== page.shown && !linked.has(d.url));
}

/** Where a move of the hop `row` lands, as places in the whole `[[hop]]` list: the one before the
 * previous hop of its line and the one of the next hop of its line, or null at the ends. */
export function moves(row, rows) {
  const line = rows.filter((one) => one.after === row.after);
  const at = line.findIndex((one) => one.id === row.id);
  return {
    earlier: at > 0 ? rows.indexOf(line[at - 1]) : null,
    later: at >= 0 && at < line.length - 1 ? rows.indexOf(line[at + 1]) : null,
  };
}

/** One chip of the kind group: `on` when chosen. A device's chip carries its URL and name. */
const chip = (text, on, attrs) => `<button type="button" class="sg-chip${on ? ' on' : ''}" role="radio" `
  + `aria-checked="${on}" data-act="hop-kind" ${attrs}>${esc(text)}</button>`;

/**
 * The fields of the sheet and of Edit: what it is (the kinds, and the amps svolder controls where
 * `amps` lists them), the name and the note. `prefix` makes the ids; `kind`, `name`, `note` are what
 * stands in them; `device` is the URL of the amp chosen (the name is then its own); `linked` leaves
 * the note alone.
 */
export function hopFields({
  prefix, kind = 'amp', name = '', note = '', amps = [], device = null, linked = false,
}) {
  const note$ = `<label class="sg-field"><span class="sg-lbl">${esc(HOPS.note)}</span>`
    + `<input class="sg-input" id="${prefix}-note" type="text" maxlength="${NOTE_MAX}" autocomplete="off" `
    + `value="${esc(note)}"></label>`;
  if (linked) return note$;
  const chosen = device ? amps.find((amp) => amp.url === device) : null;
  const chips = amps.map((amp) => chip(HOPS.connected(amp.name), amp.url === device,
    `data-device="${esc(amp.url)}" data-name="${esc(amp.name)}"`)).join('')
    + HOPS.kinds.map(([word, text]) => chip(text, !chosen && word === kind, `data-kind="${word}"`)).join('');
  return `<div class="sg-field"><span class="sg-lbl" id="${prefix}-what">${esc(HOPS.what)}</span>`
    + `<div class="sg-set" role="radiogroup" aria-labelledby="${prefix}-what">${chips}</div></div>`
    + `<label class="sg-field"><span class="sg-lbl">${esc(HOPS.name)}</span>`
    + `<input class="sg-input" id="${prefix}-name" type="text" maxlength="${NAME_MAX}" required autocomplete="off" `
    + `value="${esc(chosen ? chosen.name : name)}"${chosen ? ' disabled' : ''}></label>${note$}`;
}

/** The sheet "After the <DAC>" (H6): the fields, what the choice means, Cancel and Add. The first
 * amp svolder controls that no hop links is chosen at the start; none: an Amp, declared. */
export function plusSheet(page) {
  const dac = (page.fleet?.devices ?? []).find((d) => d.url === page.shown);
  const amps = unlinkedAmps(page);
  const device = amps[0]?.url ?? null;
  return `<form class="sg-sheet" id="sg-sheet-form" role="dialog" aria-modal="true" aria-labelledby="sg-sheet-title" novalidate>`
    + `<h4 id="sg-sheet-title">${esc(HOPS.sheetTitle(dac?.name ?? ''))}</h4>`
    + hopFields({ prefix: 'sg-add', amps, device })
    + `<p class="sg-note" id="sg-sheet-note">${esc(device ? HOPS.linkedNote : HOPS.declaredNote)}</p>`
    + `<div class="sg-sheetact"><button type="button" class="sg-btn sg-pill" data-act="sheet-cancel">${esc(HOPS.cancel)}</button>`
    + `<button type="submit" class="sg-btn sg-pill sg-go" id="sg-sheet-go">${esc(HOPS.addGo)}</button></div></form>`;
}

/** The place Edit opens its form in. */
const EDIT_PLACE = '<div id="sg-hop-edit" class="sg-hopedit"></div>';

/** The buttons of a hop of the line: Edit (for a linked one, the note), Move earlier, Move later; a
 * viewer who may not write has them drawn quiet. A move with nowhere to go is disabled. */
export function hopTools(row, rows, inert, { remove = true, form = false } = {}) {
  const to = moves(row, rows);
  const id = ` data-hop-id="${esc(row.id)}"`;
  const move = (act, text, at, idAttr) => `<button type="button" class="sg-btn sg-pill${inert ? ' quiet' : ''}" `
    + `data-act="${act}"${id} data-to="${at ?? ''}" id="${idAttr}"${at === null ? ' disabled' : ''}`
    + `${inert ? ' aria-disabled="true"' : ''}>${esc(text)}</button>`;
  return `<div class="sg-set">`
    + button('hop-edit', row.device ? HOPS.editNote : HOPS.edit, inert, `${id} id="sg-hop-edit-open"`, 'sg-btn sg-pill')
    + move('hop-move', HOPS.earlier, to.earlier, 'sg-hop-earlier')
    + move('hop-move', HOPS.later, to.later, 'sg-hop-later')
    + (remove ? button('hop-remove', HOPS.remove, inert, `${id} id="sg-hop-remove"`, 'sg-btn sg-pill danger') : '')
    + `</div>${remove ? '<div class="sg-ask"></div>' : ''}${form ? EDIT_PLACE : ''}`;
}

/** The form Edit opens in place: the sheet's fields, filled, with Cancel and Save. A linked hop's
 * has the note only. */
export function editForm(row) {
  const fields = hopFields({
    prefix: 'sg-edit', kind: row.kind, name: row.name, note: row.note ?? '', linked: Boolean(row.device),
  });
  return `<form class="sg-editform" novalidate><span class="sg-lbl">${esc(HOPS.edit)}</span>${fields}`
    + `<div class="sg-sheetact"><button type="button" class="sg-btn sg-pill" data-act="edit-cancel">${esc(HOPS.cancel)}</button>`
    + `<button type="submit" class="sg-btn sg-pill sg-go" id="sg-edit-go">${esc(HOPS.save)}</button></div></form>`;
}

/** The names on either side of the hop `id` on the line: the one before it, and the next one the
 * person declared or linked (a stand-in ring is not one). */
function neighbours(page, id) {
  const hops = hopsOf(page, page.shown);
  const at = hops.findIndex((hop) => hop.id === id);
  const after = hops.slice(at + 1).find((hop) => hop.kind !== 'standin');
  return { before: hops[at - 1]?.name ?? '', after: after?.name ?? null };
}

/** The open area of a declared hop (P3): its kind, its name, its note, where it sits, and the
 * buttons that change it, then the place Edit opens its form in. */
export function declaredScreen(page, hop, inert) {
  const rows = page.fleet?.hops ?? [];
  const row = rows.find((one) => one.id === hop.hopId);
  if (!row) return '';
  const near = neighbours(page, hop.id);
  const kind = row.device ? HOPS.linkedBy : HOPS.declaredBy(KIND_WORDS[row.kind]?.replace(/^your /, '') ?? row.kind);
  const line = row.device ? HOPS.linkedLine(row.device) : HOPS.declaredLine;
  return `<section id="sg-hop-main" class="sg-hopmain" data-hop-id="${esc(row.id)}"><span class="sg-lbl">${esc(kind)}</span>`
    + `<div class="sg-big sg-hopname">${esc(row.name)}</div>`
    + `<p class="sg-hopnote${row.note ? '' : ' sg-faint'}">${esc(row.note || HOPS.noNote)}</p>`
    + `<p class="sg-note">${esc(line)} ${esc(HOPS.between(near.before, near.after))}</p>`
    + hopTools(row, rows, inert) + `</section><section class="sg-hopside">${EDIT_PLACE}</section>`;
}
