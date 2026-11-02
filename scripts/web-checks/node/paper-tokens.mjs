/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Paper's tokens (themes/paper/style.css): every text colour reads at
// AA on its sheet in light and dark paper, the two dark blocks (the system's, under Auto, and the
// person's) set the same values, both papers dress every colour token of the frame, and each
// value is the planned one. The style sheet is read as text. Run: node paper-tokens.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const web = new URL('../../../bins/svolderd/web/', import.meta.url);
const paper = readFileSync(new URL('themes/paper/style.css', web), 'utf8');
const frame = readFileSync(new URL('core/frame.css', web), 'utf8');

/** The text between the braces of the first rule whose selector is `selector`, outside any
 * comment. */
function block(css, selector) {
  const bare = css.replace(/\/\*[\s\S]*?\*\//g, '');
  const at = bare.indexOf(`${selector} {`);
  assert.ok(at >= 0, `no rule for ${selector}`);
  const open = bare.indexOf('{', at);
  return bare.slice(open + 1, bare.indexOf('}', open));
}

/** The custom properties a block declares, `name -> value` with white space folded. */
function declared(text) {
  const out = new Map();
  for (const part of text.split(';')) {
    const found = /^\s*(--[\w-]+)\s*:\s*([\s\S]+?)\s*$/.exec(part);
    if (found) out.set(found[1], found[2].replace(/\s+/g, ' '));
  }
  return out;
}

const LIGHT = ':root[data-theme="paper"]';
const MEDIA = ':root[data-theme="paper"]:not([data-scheme="light"])';
const DARK = ':root[data-theme="paper"][data-scheme="dark"]';

const base = declared(block(paper, LIGHT));
const media = declared(block(paper, MEDIA));
const dark = declared(block(paper, DARK));

/** The tokens in force in the light paper and in the dark paper: the base block, then the dark
 * block over it. */
const light = base;
const night = new Map([...base, ...dark]);

/** [name, light, dark]: the planned values; a null dark value is the light one. */
const PLANNED = [
  ['--paper-sheet', '#f3efe6', '#1e1f22'],
  ['--paper-grid', 'rgba(60, 80, 120, .07)', 'rgba(236, 232, 223, .07)'],
  ['--paper-print', '#23201b', '#ece8df'],
  ['--paper-print-dim', '#6b6458', '#a8a398'],
  ['--paper-print-soft', '#4a443a', '#c9c3b8'],
  ['--paper-pencil', '#b3261e', '#f2786b'],
  ['--paper-ink', '#2b4a8a', '#8fb0f5'],
  ['--paper-hatch', 'rgba(179, 38, 30, .18)', 'rgba(242, 120, 107, .22)'],
  ['--paper-serif', '"Crimson Pro", "Iowan Old Style", "Palatino Linotype", Georgia, serif', null],
  ['--paper-hand', '"Caveat", "Segoe Print", cursive', null],
  ['--paper-mono', '"IBM Plex Mono", ui-monospace, monospace', null],
  ['--frame-btn-fg', '#23201b', '#ece8df'],
  ['--frame-btn-border', '#23201b', '#ece8df'],
  ['--frame-btn-fg-hover', '#b3261e', '#f2786b'],
  ['--frame-btn-border-hover', '#b3261e', '#f2786b'],
  ['--frame-btn-bg', 'transparent', null],
  ['--frame-btn-size', '30px', null],
  ['--frame-btn-radius', '50%', null],
  ['--frame-surface', '#faf7f0', '#26282b'],
  ['--frame-hover', '#ece6d9', '#2e3034'],
  ['--frame-ink', '#23201b', '#ece8df'],
  ['--frame-dim', '#6b6458', '#a8a398'],
  ['--frame-faint', '#6b6458', '#a8a398'],
  ['--frame-line', 'rgba(35, 32, 27, .18)', 'rgba(236, 232, 223, .18)'],
  ['--frame-line-strong', 'rgba(35, 32, 27, .4)', 'rgba(236, 232, 223, .4)'],
  ['--frame-accent', '#b3261e', '#f2786b'],
  ['--frame-on-accent', '#f3efe6', '#1e1f22'],
  ['--frame-scrim', 'rgba(35, 32, 27, .35)', 'rgba(0, 0, 0, .55)'],
  ['--frame-shadow', '0 8px 24px rgba(35, 32, 27, .18)', '0 8px 24px rgba(0, 0, 0, .5)'],
  ['--frame-toast-shadow', '0 8px 20px -8px rgba(35, 32, 27, .35)',
    '0 8px 20px -8px rgba(0, 0, 0, .6)'],
  ['--frame-radius', '4px', null],
  ['--frame-font', 'var(--paper-serif)', null],
  ['--frame-toast-lift', '16px', null],
];

/** `#rrggbb` as [r, g, b]. */
const rgb = (hex) => [1, 3, 5].map((at) => parseInt(hex.slice(at, at + 2), 16));

/** The WCAG relative luminance of a `#rrggbb` colour. */
function luminance(hex) {
  const [r, g, b] = rgb(hex).map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** The contrast ratio of two `#rrggbb` colours. */
function ratio(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

ok('every_text_token_reads_at_aa_on_its_sheet', () => {
  for (const [name, tokens] of [['light', light], ['dark', night]]) {
    const on = (fg, bg) => {
      const [f, b] = [tokens.get(fg), tokens.get(bg)];
      assert.match(f, /^#[0-9a-f]{6}$/, `${name} ${fg}`);
      assert.match(b, /^#[0-9a-f]{6}$/, `${name} ${bg}`);
      const r = ratio(f, b);
      assert.ok(r >= 4.5, `${name}: ${fg} on ${bg} is ${r.toFixed(2)}`);
    };
    for (const fg of ['--paper-print', '--paper-print-dim', '--paper-print-soft',
      '--paper-pencil', '--paper-ink']) on(fg, '--paper-sheet');
    for (const fg of ['--frame-ink', '--frame-dim', '--frame-accent']) {
      on(fg, '--frame-surface');
      on(fg, '--frame-hover');
    }
    on('--frame-on-accent', '--frame-accent');
  }
});

ok('the_two_dark_blocks_agree', () => {
  assert.ok(dark.size > 10, 'the dark block sets the dark paper');
  assert.deepEqual([...media].sort(), [...dark].sort());
  assert.match(block(paper, MEDIA), /color-scheme:\s*dark/);
  assert.match(block(paper, DARK), /color-scheme:\s*dark/);
  assert.match(block(paper, LIGHT), /color-scheme:\s*light/);
  assert.match(paper.replace(/\s+/g, ' '),
    /@media \(prefers-color-scheme: dark\) \{ :root\[data-theme="paper"\]:not\(\[data-scheme="light"\]\)/);
});

ok('both_papers_set_every_frame_colour_token', () => {
  const root = declared(block(frame, ':root'));
  // What is not a colour: sizes, radii, the font, the lift, the tap size.
  const plain = /^--frame-(btn-size|btn-radius|radius|tap|font|mono|toast-lift)$/;
  const colours = [...root.keys()].filter((name) => name.startsWith('--frame-') && !plain.test(name));
  assert.ok(colours.length >= 15, `${colours.length} colour tokens in the frame`);
  for (const name of colours) {
    assert.ok(light.has(name), `light paper sets ${name}`);
    assert.ok(night.has(name), `dark paper sets ${name}`);
  }
  // The dark block itself overrides each one that is a colour of the light paper.
  for (const name of colours.filter((c) => c !== '--frame-btn-bg')) {
    assert.ok(dark.has(name), `the dark block sets ${name}`);
  }
});

ok('the_tokens_hold_the_planned_values', () => {
  for (const [name, a, b] of PLANNED) {
    assert.equal(light.get(name), a, `light ${name}`);
    assert.equal(night.get(name), b ?? a, `dark ${name}`);
  }
});

console.log(`paper-tokens: ${n} passed`);
