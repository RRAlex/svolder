/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The comparison the theme Ember's drawing is held to: whether two HTML fragments are the same
// page for the browser, under the rules that let the drawing differ from the templates in
// what the browser ignores. The tags are read one by one and compared in order, so a different
// tag, text, attribute value or missing space between two tags is a difference.
//
// - The attributes htmx reads are dropped (hx-post, hx-ext, hx-target, hx-swap, hx-trigger,
//   hx-get, hx-push-url); hx-swap-oob and hx-preserve stay.
// - Attribute order does not matter; a value is read with its entities decoded.
// - A run of whitespace (the browser's ASCII whitespace: not a no-break space) is one space, and
//   the fragment is trimmed.
// - data-json, data-texts, data-db and data-readings are compared as JSON, and so is the text of
//   a `<script type="application/json">` block (read as it stands, with no character references
//   decoded: the page parses it as JSON).
// - The numbers of x1 y1 x2 y2 cx cy x y r d data-at data-glow and data-y may differ by 0.011:
//   the rest of the value must be the same.

import assert from 'node:assert/strict';

const DROPPED = new Set([
  'hx-post', 'hx-ext', 'hx-target', 'hx-swap', 'hx-trigger', 'hx-get', 'hx-push-url',
]);
const JSON_ATTRS = new Set(['data-json', 'data-texts', 'data-db', 'data-readings']);
const NUMERIC_ATTRS = new Set([
  'x1', 'y1', 'x2', 'y2', 'cx', 'cy', 'x', 'y', 'r', 'd', 'data-at', 'data-glow', 'data-y',
]);
const TOLERANCE = 0.011;

const NAMED = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'" };

/** `text` with its character references decoded. */
function decode(text) {
  return text.replace(/&(?:#x([0-9a-f]+)|#(\d+)|([a-z]+));/gi, (whole, hex, dec, name) => {
    if (hex !== undefined) return String.fromCodePoint(parseInt(hex, 16));
    if (dec !== undefined) return String.fromCodePoint(parseInt(dec, 10));
    return NAMED[name.toLowerCase()] ?? whole;
  });
}

/** The tags and texts of `html`, in order. */
function tokens(html) {
  const out = [];
  const source = html.replace(/^[ \t\n\r\f]+|[ \t\n\r\f]+$/g, '');
  let at = 0;
  while (at < source.length) {
    if (source[at] !== '<') {
      const end = source.indexOf('<', at);
      const stop = end < 0 ? source.length : end;
      out.push({ text: decode(source.slice(at, stop).replace(/[ \t\n\r\f]+/g, ' ')) });
      at = stop;
      continue;
    }
    if (source.startsWith('</', at)) {
      const end = source.indexOf('>', at);
      out.push({ close: source.slice(at + 2, end).trim().toLowerCase() });
      at = end + 1;
      continue;
    }
    const name = /^<([a-zA-Z][^\s/>]*)/.exec(source.slice(at))[1];
    at += 1 + name.length;
    const attrs = new Map();
    let self = false;
    for (;;) {
      while (/\s/.test(source[at])) at++;
      if (source.startsWith('/>', at)) { self = true; at += 2; break; }
      if (source[at] === '>') { at += 1; break; }
      const attr = /^([^\s=/>]+)/.exec(source.slice(at))[1];
      at += attr.length;
      let value = '';
      while (/\s/.test(source[at])) at++;
      if (source[at] === '=') {
        at++;
        while (/\s/.test(source[at])) at++;
        const quote = source[at];
        if (quote === '"' || quote === "'") {
          const end = source.indexOf(quote, at + 1);
          value = source.slice(at + 1, end);
          at = end + 1;
        } else {
          value = /^[^\s>]*/.exec(source.slice(at))[0];
          at += value.length;
        }
      }
      if (!DROPPED.has(attr.toLowerCase())) attrs.set(attr.toLowerCase(), decode(value));
    }
    out.push({ open: name.toLowerCase(), attrs, self });
    if (name.toLowerCase() === 'script' && !self && attrs.get('type') === 'application/json') {
      const end = source.indexOf('</script', at);
      const stop = end < 0 ? source.length : end;
      if (stop > at) out.push({ text: source.slice(at, stop), json: true });
      at = stop;
    }
  }
  return out;
}

/** Whether attribute `name`'s values `a` and `b` are the same, or why not. */
function sameValue(name, a, b) {
  if (JSON_ATTRS.has(name)) {
    try {
      assert.deepStrictEqual(JSON.parse(a), JSON.parse(b));
      return null;
    } catch (e) {
      if (a === b) return null;
      return `${name}: ${a.slice(0, 60)} against ${b.slice(0, 60)}`;
    }
  }
  if (NUMERIC_ATTRS.has(name)) {
    const number = /-?\d+(?:\.\d+)?/g;
    const [skeletonA, skeletonB] = [a.replace(number, '#'), b.replace(number, '#')];
    const [numbersA, numbersB] = [a.match(number) ?? [], b.match(number) ?? []];
    const close = skeletonA === skeletonB && numbersA.length === numbersB.length
      && numbersA.every((x, i) => Math.abs(Number(x) - Number(numbersB[i])) <= TOLERANCE);
    return close ? null : `${name}: ${a} against ${b}`;
  }
  return a === b ? null : `${name}: ${JSON.stringify(a)} against ${JSON.stringify(b)}`;
}

/** Whether the texts of tokens `x` and `y` are the same: a data block as JSON, else exactly. */
function sameText(x, y) {
  if (!x.json && !y.json) return x.text === y.text;
  if (x.json !== y.json) return false;
  try {
    assert.deepStrictEqual(JSON.parse(x.text), JSON.parse(y.text));
    return true;
  } catch (e) {
    return x.text === y.text;
  }
}

/** What `token` is, for a message. */
const show = (token) => {
  if (token === undefined) return 'the end';
  if (token.text !== undefined) return `text ${JSON.stringify(token.text.slice(0, 60))}`;
  if (token.close !== undefined) return `</${token.close}>`;
  return `<${token.open}>`;
};

/**
 * Whether `ours` and `theirs` are the same fragment: `{ same, at }`, where `at` says where they
 * first differ (empty when they do not).
 */
export function sameHtml(ours, theirs) {
  const [a, b] = [tokens(ours), tokens(theirs)];
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const [x, y] = [a[i], b[i]];
    const where = `token ${i}`;
    const differs = x === undefined || y === undefined
      || (x.text !== undefined && y.text !== undefined
        ? !sameText(x, y)
        : (x.text ?? x.close ?? x.open) !== (y.text ?? y.close ?? y.open))
      || (x.text === undefined) !== (y.text === undefined)
      || (x.close === undefined) !== (y.close === undefined);
    if (differs) return { same: false, at: `${where}: ${show(x)} against ${show(y)}` };
    if (x.text !== undefined || x.close !== undefined) continue;
    if (x.self !== y.self) return { same: false, at: `${where}: <${x.open}> self-closing differs` };
    for (const name of new Set([...x.attrs.keys(), ...y.attrs.keys()])) {
      if (!x.attrs.has(name) || !y.attrs.has(name)) {
        const missing = x.attrs.has(name) ? 'theirs' : 'ours';
        return { same: false, at: `${where} <${x.open}>: ${name} missing from ${missing}` };
      }
      const why = sameValue(name, x.attrs.get(name), y.attrs.get(name));
      if (why) return { same: false, at: `${where} <${x.open}> ${why}` };
    }
  }
  return { same: true, at: '' };
}
