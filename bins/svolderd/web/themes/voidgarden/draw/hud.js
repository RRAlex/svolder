/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The HUD's drawing: the modules' markup (DOM text, so a screen reader, the contrast check and the
// driver read the words) and the one 2D canvas that draws what is not text over the rows' empty
// slots: the bars (VU, volume, an amp's meters), the EQ's sparkline and the link's bars and 8 s
// history. The markup is pure (the modules of hud.js in, a string out); `paintHud` draws into a
// context it is given, placing each part by the slot's box (`rect` reads it, so a harness can hand
// boxes in).
//
//   hudMarkup(modules)              the modules as markup, every one shown at opacity 0 until the
//                                   wire sets its strength
//   metersInPlace(root, amp)        put an amp's meters in the drawn rows; false when the rows
//                                   are not those the meters give (draw the HUD again)
//   paintHud(ctx, root, data, opt)  draw the slots of `root`

import { esc } from '../../../core/html.js';
import { HUD } from '../words.js';

/** The mark `text` is read as by a screen reader where the row's tone is not the words' own. */
const TONES = new Set(['on', 'warn', 'bad']);

/** The PipeWire config check's chip: a button that opens the config popup. */
function chipMarkup(chip) {
  const outcome = esc(chip.outcome ?? '');
  const label = esc(HUD.chipOpens(chip.text));
  return `<button type="button" class="vg-h-chip" data-act="config" data-outcome="${outcome}"`
    + ` aria-haspopup="dialog" title="${esc(chip.summary ?? '')}" aria-label="${label}">`
    + `${esc(chip.text)}</button>`;
}

/** The top line's inside: the device and its link words on the left, the verdict (or the trouble)
 * on the right, which carries the tone. */
export function topMarkup(top) {
  const tone = TONES.has(top.tone) ? ` data-tone="${top.tone}"` : '';
  const right = top.right ? `<span class="vg-h-r"${tone}>${esc(top.right)}</span>` : '';
  return `<span class="vg-h-l">${esc(top.left)}</span>${right}`;
}

/** Put the Volume row's reading in the row already drawn (`row` is hud.js's): its words, its bar
 * and its tone. Says whether it could (the row is drawn). */
export function volumeInPlace(root, row) {
  const el = root.querySelector('[data-row="volume"]');
  if (!el || !row) return false;
  const words = el.querySelector('.vg-h-v');
  if (words.textContent !== row.text) words.textContent = row.text;
  el.querySelector('.vg-h-slot').dataset.v = Number(row.slot.value).toFixed(4);
  if (TONES.has(row.tone)) el.dataset.tone = row.tone;
  else delete el.dataset.tone;
  return true;
}

/** One row: its label, its words, then its slot, note and chip. */
function rowMarkup(r) {
  const tone = TONES.has(r.tone) ? ` data-tone="${r.tone}"` : '';
  const bar = r.slot?.kind === 'bar'
    ? ` data-v="${Number(r.slot.value).toFixed(4)}"${r.slot.off ? ' data-off="1"' : ''}` : '';
  const slot = r.slot
    ? `<i class="vg-h-slot" data-slot="${esc(r.slot.kind)}" data-id="${esc(r.slot.id)}"${bar}`
      + ' aria-hidden="true"></i>'
    : '';
  const note = r.note ? ` <span class="vg-h-note">${esc(r.note)}</span>` : '';
  const chip = r.chip ? chipMarkup(r.chip) : '';
  return `<div class="vg-h-row" data-row="${esc(r.id)}"${tone}>`
    + `<span class="vg-h-k">${esc(r.label)}</span>`
    + `<span class="vg-h-v">${esc(r.text)}</span>${slot}${note}${chip}</div>`;
}

/** The modules of `modules` (hud.js `modulesOf`) as markup. */
export function hudMarkup(modules) {
  return modules.map((m) => {
    const sub = m.sub ? `<span class="vg-h-sub"> · ${esc(m.sub)}</span>` : '';
    return `<section class="vg-h-mod" data-mod="${esc(m.id)}" data-on="0"`
      + ` aria-label="${esc(m.title)}"><h2 class="vg-h-title">${esc(m.title)}${sub}</h2>`
      + `${m.rows.map(rowMarkup).join('')}</section>`;
  }).join('');
}

/** Put an amp's meters in the rows drawn: each bar's fill and its words. Says whether it could (the
 * same meters as were drawn, with or without the line that says why there are none). */
export function metersInPlace(root, amp) {
  const meters = amp?.meters?.meters ?? [];
  const drawn = [...root.querySelectorAll('[data-row^="meter:"]')];
  if (drawn.length !== meters.length) return false;
  if (drawn.some((el, i) => el.dataset.row !== `meter:${meters[i].key}`)) return false;
  const stated = root.querySelector('[data-row="meters-state"]');
  const why = amp?.meters?.state ?? null;
  if (Boolean(stated) !== Boolean(why)) return false;
  if (stated && stated.querySelector('.vg-h-v').textContent !== why) return false;
  meters.forEach((m, i) => {
    const text = drawn[i].querySelector('.vg-h-v');
    if (text.textContent !== m.text) text.textContent = m.text;
    const bar = drawn[i].querySelector('.vg-h-slot');
    const read = typeof m.fraction === 'number';
    bar.dataset.v = (read ? Math.min(1, Math.max(0, m.fraction)) : 0).toFixed(4);
    if (read) delete bar.dataset.off;
    else bar.dataset.off = '1';
  });
  return true;
}

/** A bar: the track, the fill, the 24 segments' gaps and the end ticks. */
function bar(c, box, v, ink, mark) {
  const { x, y, w, h } = box;
  c.fillStyle = ink.hair;
  c.fillRect(x, y, w, h);
  c.fillStyle = ink.fill;
  c.fillRect(x, y, Math.round(w * Math.min(1, Math.max(0, v))), h);
  c.fillStyle = ink.gap;
  for (let i = 1; i < 24; i++) c.fillRect(x + Math.round((w * i) / 24), y, 1, h);
  c.fillStyle = ink.dim;
  for (let i = 0; i <= 4; i++) {
    c.fillRect(x + Math.round(((w - 1) * i) / 4), y + h + 1, 1, i % 2 ? 2 : 3);
  }
}

/** A sparkline of `values` (0 to 1, oldest first) across `box`, with a baseline. */
function spark(c, box, values, ink) {
  const { x, y, w, h } = box;
  c.fillStyle = ink.hair;
  c.fillRect(x, y + h, w, 1);
  c.strokeStyle = ink.line;
  c.lineWidth = 1;
  c.beginPath();
  const n = values.length;
  values.forEach((v, i) => {
    const px = x + (n > 1 ? (i / (n - 1)) * w : 0);
    const py = y + h - Math.min(1, Math.max(0, v)) * h;
    if (i) c.lineTo(px, py);
    else c.moveTo(px, py);
  });
  c.stroke();
}

/** The link: four signal bars, then the 8 s history of its strength. */
function link(c, box, slot, ink) {
  const { x, y, w, h } = box;
  const sigW = 34;
  const on = slot?.warn ? ink.warn : ink.fill;
  for (let i = 0; i < 4; i++) {
    c.fillStyle = i < (slot?.bars ?? 0) ? on : ink.hair;
    c.fillRect(x + i * 8, y + h - (i + 1) * 3, 6, (i + 1) * 3 + 1);
  }
  const points = slot?.points ?? [];
  c.strokeStyle = slot?.warn ? ink.warn : ink.line;
  c.fillStyle = ink.hair;
  c.fillRect(x + sigW, y + h, w - sigW, 1);
  if (points.length === 0) return;
  c.beginPath();
  points.forEach((p, i) => {
    const px = x + sigW + p.x * (w - sigW);
    const py = y + h - p.y * h;
    if (i) c.lineTo(px, py);
    else c.moveTo(px, py);
  });
  c.stroke();
}

/** 
 * Draw the slots of the HUD `root` into `ctx`, which is `root`'s size in CSS pixels (the caller
 * scales it for the screen's density). `data`: `{ sparks: { id: values }, link: slot, alpha: {
 * module: 0 to 1 }, ink }` with the colours as CSS colour strings (`fill`, `line`, `hair`, `gap`,
 * `dim`, `warn`). `opt.rect(el)` is an element's box (the element's own by default).
 */
export function paintHud(ctx, root, data, { rect = (el) => el.getBoundingClientRect() } = {}) {
  const base = rect(root);
  ctx.clearRect(0, 0, base.width, base.height);
  for (const el of root.querySelectorAll('.vg-h-slot')) {
    const module = el.closest('[data-mod]')?.dataset.mod;
    const alpha = data.alpha?.[module] ?? 0;
    if (alpha <= 0.01) continue;
    const r = rect(el);
    const box = { x: r.left - base.left, y: r.top - base.top, w: r.width, h: r.height };
    ctx.globalAlpha = alpha;
    const kind = el.dataset.slot;
    if (kind === 'bar') bar(ctx, { ...box, h: Math.min(box.h, 5) }, Number(el.dataset.v),
      el.dataset.off ? { ...data.ink, fill: data.ink.hair } : data.ink,
      el.dataset.pk === undefined ? NaN : Number(el.dataset.pk));
    else if (kind === 'spark') spark(ctx, box, data.sparks?.[el.dataset.id] ?? [], data.ink);
    else if (kind === 'link') link(ctx, box, data.link, data.ink);
  }
  ctx.globalAlpha = 1;
}
