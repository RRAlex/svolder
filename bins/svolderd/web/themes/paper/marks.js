/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Where the notes stand and what is drawn over the sheet for them. On a desktop the notes are in
// the band under the two columns, a grid of three, each note in the third under the centre of the
// section it is about, in note order; ink arrows run from the EQ's note and from the red note to
// what they are about, and an ellipse rings the red note. The volume is above both columns, so the
// red note's arrow to it runs along the gap under the columns to the sheet's margin on the side of
// the needle, climbs it clear of the sections and turns in to end beside the written reading. On a phone every note follows the
// section it is about in the flow, and the lead words of the red note are ringed in pencil. The
// marks are drawn in `svg#p-marks` in pixels of the sheet, so they are drawn again after the notes,
// the sheet's width or the fonts change.

const SVG = 'http://www.w3.org/2000/svg';

/** How many of the red note's first words a phone rings. */
const LEAD_WORDS = 3;

/** The ellipse's margin round the note (px) and its tilt (degrees). */
const RING_X = 14;
const RING_Y = 10;
const RING_TILT = -3;

/** The arrow's head: how long (px) and how far from the shaft's line (radians). */
const HEAD = 10;
const SPREAD = 0.5;

/** How far inside the sheet's edge (px) the arrow to the volume climbs, how far it stops short of
 * the written reading, and how round the corner is where the lane meets the margin. */
const MARGIN_X = 14;
const STOP = 8;
const CORNER = 18;

/** How far under the columns (px) the arrow's sweep to the margin runs. */
const GAP_DROP = 8;

/** Take away the notes that were moved out of the band to follow their sections, before the band
 * is drawn again. */
export function clearMoved(root) {
  for (const note of root.querySelectorAll('.pn.moved')) note.remove();
}

/** The red note's lead words in a ring (`span.ring`), on a phone; none on a desktop. */
function ring(note, phone) {
  const wrapped = note.querySelector(':scope > .ring');
  if (!phone) {
    wrapped?.replaceWith(wrapped.textContent);
    return;
  }
  const first = note.firstChild;
  if (wrapped || first?.nodeType !== 3) return;
  const lead = new RegExp(`^\\s*\\S+(?:\\s+\\S+){0,${LEAD_WORDS - 1}}`).exec(first.data)?.[0];
  if (!lead) return;
  const span = note.ownerDocument.createElement('span');
  span.className = 'ring';
  span.textContent = lead;
  first.data = first.data.slice(lead.length);
  first.before(span);
}

/** A phone: each note follows the section it is about, in note order. */
function flow(root, notes) {
  const last = new Map();
  for (const note of notes) {
    const about = note.dataset.about;
    const anchor = root.querySelector(`#p-${about}`);
    if (!anchor) continue;
    const before = last.get(about) ?? anchor;
    if (before.nextElementSibling !== note) before.after(note);
    note.classList.add('moved');
    last.set(about, note);
  }
}

/** A desktop: each note in the band's third that its section's centre is over. */
function band(root, box, notes) {
  const wide = box.getBoundingClientRect();
  for (const note of notes) {
    if (note.parentElement !== box) box.append(note);
    note.classList.remove('moved');
    const anchor = root.querySelector(`#p-${note.dataset.about}`)?.getBoundingClientRect();
    const centre = anchor ? anchor.left + anchor.width / 2 : wide.left + wide.width / 2;
    const third = Math.min(2, Math.max(0, Math.floor(((centre - wide.left) / wide.width) * 3)));
    note.style.gridColumn = String(third + 1);
  }
}

/** The mark `tag` of `attrs`, made in the sheet's svg. */
function mark(svg, tag, attrs) {
  const el = svg.ownerDocument.createElementNS(SVG, tag);
  for (const [name, value] of Object.entries(attrs)) el.setAttribute(name, String(value));
  svg.append(el);
  return el;
}

/** The open head of an arrow at `to`, for a shaft that arrives heading from `via`. */
function head(svg, via, to, cls) {
  const heading = Math.atan2(to.y - via.y, to.x - via.x);
  const wing = (side) => `${to.x - HEAD * Math.cos(heading + side)} ${to.y - HEAD * Math.sin(heading + side)}`;
  mark(svg, 'path', { class: `arrow ${cls}`, d: `M${wing(SPREAD)}L${to.x} ${to.y}L${wing(-SPREAD)}` });
}

/** An ink arrow with an open head from the note at `from` to the point `to`, bowed a little the
 * way a hand draws it; `cls` is `ink` or `red`. */
function arrow(svg, from, to, cls) {
  const bow = (to.x - from.x) * 0.12 + (to.y > from.y ? -30 : 30);
  const cx = (from.x + to.x) / 2 + bow;
  const cy = (from.y + to.y) / 2;
  mark(svg, 'path', { class: `arrow ${cls}`, d: `M${from.x} ${from.y}Q${cx} ${cy} ${to.x} ${to.y}` });
  head(svg, { x: cx, y: cy }, to, cls);
}

/** A red arrow from the top of the note at `box` to the written reading `text` of the volume, with
 * the head pointing in: up into the gap under the two columns, along it to the sheet's margin on
 * the needle's side (`right` or left), up the margin and in to the reading's near edge. `gap` is
 * the page's y of the gap's line, `rel` turns page pixels into the sheet's and `width` is the
 * sheet's. */
function marginArrow(svg, { rel, width }, box, text, gap, right) {
  const dir = right ? 1 : -1;
  const from = rel(box.left + box.width / 2, box.top - 4);
  const lane = rel(0, gap).y;
  const margin = right ? width - MARGIN_X : MARGIN_X;
  const to = rel(right ? text.right + STOP : text.left - STOP, text.top + text.height / 2);
  const corner = margin - dir * CORNER;
  const up = lane - CORNER;
  const mid = (up + to.y) / 2;
  mark(svg, 'path', {
    class: 'arrow red',
    d: `M${from.x} ${from.y}C${from.x} ${lane} ${corner - dir * 20} ${lane} ${corner} ${lane}`
      + `Q${margin} ${lane} ${margin} ${up}C${margin} ${mid} ${margin} ${to.y} ${to.x} ${to.y}`,
  });
  head(svg, { x: margin, y: to.y }, to, 'red');
}

/** What `note` points at: the section it is about; nothing for a note with no section on show. */
function targetOf(root, note) {
  const rect = root.querySelector(`#p-${note.dataset.about}`)?.getBoundingClientRect();
  return rect && rect.width > 0 && rect.height > 0 ? rect : null;
}

/** The arrows and the ellipse of a desktop, in `svg` (pixels of `paper`). */
function draw(root, svg, paper, red) {
  const origin = paper.getBoundingClientRect();
  const rel = (x, y) => ({ x: x - origin.left, y: y - origin.top });
  const eq = root.querySelector('#p-notes .pn[data-note="eq"]');
  // Two notes about one section would cross: the red note's arrow is the one drawn there.
  const shared = Boolean(eq && red?.dataset.note && red.dataset.about === eq.dataset.about);
  for (const note of new Set([shared ? null : eq, red && red.dataset.note ? red : null])) {
    const aim = note && targetOf(root, note);
    if (!aim) continue;
    const box = note.getBoundingClientRect();
    const below = aim.top > box.bottom;
    if (note === red && note.dataset.about === 'volume' && !below) {
      const cols = root.querySelector('.p-cols')?.getBoundingClientRect();
      const text = root.querySelector('#p-volume-text')?.getBoundingClientRect();
      const ruler = root.querySelector('#p-ruler')?.getBoundingClientRect();
      const needle = root.querySelector('#p-needle')?.getBoundingClientRect();
      if (text && ruler && needle) {
        const gap = Math.min(box.top - 8, (cols?.bottom ?? box.top) + GAP_DROP);
        const right = needle.left + needle.width / 2 > ruler.left + ruler.width / 2;
        marginArrow(svg, { rel, width: paper.clientWidth }, box, text, gap, right);
        continue;
      }
    }
    const x = Math.min(aim.right - 16, Math.max(aim.left + 16, box.left + box.width / 2));
    arrow(svg, rel(box.left + box.width / 2, below ? box.bottom + 4 : box.top - 4),
      rel(x, below ? aim.top - 4 : aim.bottom + 6), note === red ? 'red' : 'ink');
  }
  if (!red) return;
  const box = red.getBoundingClientRect();
  const c = rel(box.left + box.width / 2, box.top + box.height / 2);
  mark(svg, 'ellipse', {
    class: 'ring', cx: c.x, cy: c.y, rx: box.width / 2 + RING_X, ry: box.height / 2 + RING_Y,
    transform: `rotate(${RING_TILT} ${c.x} ${c.y})`,
  });
}

/**
 * Put the notes in `root` where they stand and draw the marks over them: on a desktop each in the
 * band's third under its section's centre, arrows from the EQ's note and the red note to their
 * sections (the EQ's is left out where the red note is about the same section) and an ellipse
 * round the red note; on a phone (`phone`) each after its section and a ring round the red note's
 * lead words. The red note may be the card's, drawn under the title.
 */
export function placeNotes(root, { phone }) {
  const svg = root.querySelector('#p-marks');
  const paper = root.querySelector('#paper');
  const box = root.querySelector('#p-notes');
  if (!svg || !paper || !box) return;
  const notes = [...root.querySelectorAll('.pn[data-note]')]
    .sort((a, b) => Number(a.dataset.n) - Number(b.dataset.n));
  if (phone) flow(root, notes);
  else band(root, box, notes);
  const red = root.querySelector('.pn.red');
  if (red) ring(red, phone);
  svg.replaceChildren();
  svg.setAttribute('viewBox', `0 0 ${paper.clientWidth} ${paper.clientHeight}`);
  if (!phone) draw(root, svg, paper, red);
}
