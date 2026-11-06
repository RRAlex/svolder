/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Drawing a section again without replacing its elements. A press and its release must land on the
// same element for the browser to deliver a click, so a draw that arrives between the two (the
// device's model changes at any moment) must leave the element under the finger where it is.
// `morph(root, html)` makes the children of `root` the markup `html` says, in place: an element is
// kept (its attributes, text and children brought up to date) when the new markup has one of the
// same hop (`data-hop`) or id (and the same tag and first class), or else one of the same tag and
// first class at the same place; only what the markup adds is inserted and only what it no longer
// has is removed.
//
// It is for markup like the line's: elements, attributes and text. Comments are not synced, nor
// are the properties of form controls (`value`, `checked`, `selected`), so a section with inputs
// needs more before it can use this.
//
// The `style` attribute is left alone: the markup carries none (the page's content policy forbids
// it) and every place the page sets through the CSSOM is set again by the draw that follows.

/** What keeps an element the same one across draws, or nothing. */
const keyOf = (node) => {
  const key = node.dataset?.hop ?? (node.id || null);
  return key === null ? null : `${node.nodeName} ${(node.getAttribute('class') ?? '').split(' ')[0]} ${key}`;
};

/** The first class of an element: the part of its markup that names what it is. */
const kindOf = (node) => `${node.nodeName} ${(node.getAttribute('class') ?? '').split(' ')[0]}`;

function syncAttributes(from, to) {
  for (const { name, value } of [...to.attributes]) {
    if (name !== 'style' && from.getAttribute(name) !== value) from.setAttribute(name, value);
  }
  for (const { name } of [...from.attributes]) {
    if (name !== 'style' && !to.hasAttribute(name)) from.removeAttribute(name);
  }
}

function syncChildren(parent, source) {
  const keyed = new Map([...parent.childNodes]
    .filter((node) => node.nodeType === 1 && keyOf(node)).map((node) => [keyOf(node), node]));
  const used = new Set();
  let at = 0;
  for (const next of [...source.childNodes]) {
    let match = null;
    if (next.nodeType === 1 && keyOf(next)) {
      const kept = keyed.get(keyOf(next));
      match = kept && !used.has(kept) ? kept : null;
    } else {
      // the nodes from `at` on are the ones not yet placed (a node placed stands before `at`)
      match = [...parent.childNodes].slice(at).find((node) => node.nodeType === next.nodeType
        && (next.nodeType !== 1 || (!keyOf(node) && kindOf(node) === kindOf(next)))) ?? null;
    }
    if (match === null) {
      parent.insertBefore(next, parent.childNodes[at] ?? null);
    } else {
      used.add(match);
      if (parent.childNodes[at] !== match) parent.insertBefore(match, parent.childNodes[at] ?? null);
      if (match.nodeType === 3) {
        if (match.nodeValue !== next.nodeValue) match.nodeValue = next.nodeValue;
      } else if (match.nodeType === 1) {
        syncAttributes(match, next);
        syncChildren(match, next);
      }
    }
    at += 1;
  }
  while (parent.childNodes.length > at) parent.removeChild(parent.lastChild);
}

/** Make the children of `root` what the markup `html` says, keeping the elements it still has. */
export function morph(root, html) {
  const template = document.createElement('template');
  template.innerHTML = html;
  syncChildren(root, template.content);
}
