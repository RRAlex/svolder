// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

// The indicator's view data: one `svolder panel --json` line in, what the bar and the popup draw
// out. Nothing here talks to the shell, St or Clutter, so `gjs -m` tests run it anywhere. The
// Path strip's tones and words, the signal's bars, weak and old, and every choice's
// CLI arguments arrive judged in the line; this module only turns them into colours, strings and
// markup.

import GLib from 'gi://GLib';

export const PANEL_VERSION = 1;
export const MINUS = '−';

export const COLORS = {
    green: '#8ff0a4',
    yellow: '#f8e45c',
    grey: '#8a8a8a',
    white: '#ffffff',
    stripGrey: '#666666',
    dim: '#9a9a9a',
};

// The sample-rate tint of the DAC box body, one colour per rate; any other rate stays plain.
const B17_BY_RATE = new Map([
    [44100, '#cfcfcf'],
    [48000, '#ffffff'],
    [88200, '#b8d8ff'],
    [96000, '#86b4ff'],
    [176400, '#a58aff'],
    [192000, '#d476f0'],
    [352800, '#f27aa8'],
    [384000, '#ff8c72'],
]);

// The legend of the eight swatches; the docs read it as their source.
export const B17_LEGEND = [...B17_BY_RATE].map(([hz, hex]) => ({
    rate_hz: hz,
    rate_text: String(hz / 1000),
    hex,
}));

export function tintFor(rateHz) {
    return B17_BY_RATE.get(rateHz) ?? null;
}

export function escapeMarkup(text) {
    return GLib.markup_escape_text(String(text), -1);
}

// Pango lengths are in 1/1024 pt; the page's CSS px are 3/4 pt, and the stylesheet gives the text
// its size in pt too.
const pangoUnits = px => Math.round(px * 0.75 * 1024);

// `text` followed by a small unit in the same font, lowered below the baseline: the unit is
// `scale` of the surrounding text's size (a percentage, so it follows the text scale), 0.9
// opaque, one px after the last character. `basePx` is the surrounding font size in CSS px.
export function unitMarkup(text, unit, basePx, scale, lowerEm) {
    const rise = pangoUnits(basePx * lowerEm);
    const plain = escapeMarkup(text.slice(0, -1));
    const last = escapeMarkup(text.slice(-1));
    const gap = text === '' ? '' : `${plain}<span letter_spacing="${pangoUnits(1)}">${last}</span>`;
    return `${gap}<span size="${Math.round(scale * 100)}%" rise="${-rise}" alpha="90%">${escapeMarkup(unit)}</span>`;
}

// The unit of the bar's figure, 0.72 em and lowered 0.22 em.
const barUnit = (text, unit) => unitMarkup(text, unit, 12, 0.72, 0.22);
// The popup's units are 0.7 em of the row's text and lowered 0.22 em.
export const popupUnit = (text, unit, basePx = 14) => unitMarkup(text, unit, basePx, 0.7, 0.22);

// The off-only words with every ` dB` after a number drawn as the small lowered unit.
export function offMarkup(off) {
    const parts = String(off).split(/(?<=\d) dB(?![A-Za-z])/);
    return parts.map(p => escapeMarkup(p)).join(popupUnit('', 'dB', 12));
}

// The slider's figure while it is moved: `−24.1` and the small unit, as markup.
export function dbText(db) {
    return popupUnit(number1(db), 'dB', 14);
}

const number1 = db => {
    const t = Math.abs(db).toFixed(1);
    return db < 0 && t !== '0.0' ? `${MINUS}${t}` : t;
};

export function ageText(heardAtMs, nowMs) {
    const s = Math.floor((nowMs - heardAtMs) / 1000);
    if (!(s >= 5))
        return 'just now';
    if (s < 60)
        return `${s} s ago`;
    if (s < 3600)
        return `${Math.floor(s / 60)} min ago`;
    if (s < 86400)
        return `${Math.floor(s / 3600)} h ago`;
    return `${Math.floor(s / 86400)} d ago`;
}

const isAway = view => !view?.device || !view.device.present;

// How the bar's signal glyph is drawn: how many bars are filled, their colour and their opacity.
function signalGlyph(view) {
    const sig = view?.device?.signal;
    if (isAway(view) || !sig)
        return {filled: 0, color: COLORS.grey, filledOpacity: 1, emptyOpacity: 0.3};
    return {
        filled: sig.bars,
        color: sig.weak ? COLORS.yellow : COLORS.white,
        filledOpacity: sig.old ? 0.5 : 1,
        emptyOpacity: sig.old ? 0.15 : 0.3,
    };
}

// The DAC box: the body's outline tint and the LED.
function boxGlyph(view) {
    if (isAway(view))
        return {outline: null, led: 'ring', ledColor: COLORS.grey};
    const outline = tintFor(view.device.rate_hz);
    // The LED is the Path row's own tone: good green, warn yellow, plain (no verdict) a ring.
    const tone = view.path?.tone;
    if (tone === 'good')
        return {outline, led: 'dot', ledColor: COLORS.green};
    if (tone === 'warn')
        return {outline, led: 'dot', ledColor: COLORS.yellow};
    return {outline, led: 'ring', ledColor: COLORS.grey};
}

export function barView(view) {
    const away = isAway(view);
    const vol = view?.device?.volume;
    let figure = {kind: 'none', markup: ''};
    if (!away && vol?.muted === true)
        figure = {kind: 'muted', markup: ''};
    else if (!away && vol?.bar_text != null)
        figure = {kind: 'text', markup: barUnit(vol.bar_text, 'dB')};
    let name = 'svolder';
    if (away)
        name = 'svolder: the DAC is away';
    else if (figure.kind === 'muted')
        name = `svolder: ${view.device.name}, muted`;
    else if (figure.kind === 'text')
        name = `svolder: ${view.device.name}, ${vol.bar_text} dB`;
    else
        name = `svolder: ${view.device.name}`;
    return {away, box: boxGlyph(view), signal: signalGlyph(view), figure, accessibleName: name};
}

const choiceRows = choices => (choices?.choices ?? []).map(c => ({
    name: c.name,
    run: c.run,
    on: c.name === choices.on,
}));

export function sliderFraction(volume) {
    if (volume?.db == null || volume.min_db == null || volume.max_db == null || volume.max_db <= volume.min_db)
        return null;
    const f = (volume.db - volume.min_db) / (volume.max_db - volume.min_db);
    return Math.min(1, Math.max(0, f));
}

// The dB a slider position stands for, to one decimal (the CLI's own step is finer than a hand).
export function dbFromFraction(fraction, volume) {
    const db = volume.min_db + Math.min(1, Math.max(0, fraction)) * (volume.max_db - volume.min_db);
    const rounded = Math.round(db * 10) / 10;
    return rounded === 0 ? 0 : rounded;
}

// The header's signal part: bars, the dBm figure and, for an old reading, its age.
function headerSignal(view, nowMs) {
    const glyph = signalGlyph(view);
    const sig = view.device.signal;
    // The header's bars are grey; yellow still says weak.
    if (glyph.color === COLORS.white)
        glyph.color = COLORS.dim;
    if (isAway(view))
        return {glyph, markup: escapeMarkup('away'), dim: true};
    if (!sig)
        return {glyph, markup: '', dim: true};
    let markup = popupUnit(sig.dbm_text, 'dBm', 13);
    if (sig.old)
        markup += escapeMarkup(` · ${ageText(sig.heard_at_ms, nowMs)}`);
    return {glyph, markup, dim: true};
}

// A refused or older shape than this module reads is a failure line, never a guess.
export function versionFailure(view) {
    if (view?.v !== PANEL_VERSION)
        return {headline: 'this svolder speaks another panel version', detail: 'update svolder or the extension'};
    return null;
}

export function popupView(view, nowMs, failure = null) {
    if (!view || failure) {
        return {
            failure: failure ?? {headline: 'waiting for svolder', detail: ''},
            note: null,
            header: null,
            volume: null,
            input: null,
            preset: null,
            computerEq: null,
            path: null,
            scenes: null,
        };
    }
    const dev = view.device;
    const away = isAway(view);
    const box = boxGlyph(view);
    const header = dev ? {
        name: dev.name,
        nameMarkup: `<b>${escapeMarkup(dev.name)}</b>`,
        box,
        rate: dev.rate_text != null && !away ? {markup: popupUnit(dev.rate_text, 'kHz', 13)} : null,
        signal: headerSignal(view, nowMs),
    } : {
        name: 'svolder',
        nameMarkup: '<b>svolder</b>',
        box,
        rate: null,
        signal: {glyph: signalGlyph(view), markup: escapeMarkup('away'), dim: true},
    };

    let volume = null;
    const vol = dev?.volume;
    if (!away && vol) {
        volume = {
            muted: vol.muted === true,
            canMute: vol.muted != null,
            fraction: sliderFraction(vol),
            min_db: vol.min_db,
            max_db: vol.max_db,
            textMarkup: vol.text != null ? popupUnit(vol.text, 'dB', 14) : '',
        };
    }

    const input = !away && dev?.input ? {
        label: 'Input', value: dev.input.on ?? '', choices: choiceRows(dev.input),
    } : null;
    const preset = !away && dev?.preset ? {
        label: 'DAC preset', value: dev.preset.on ?? '', choices: choiceRows(dev.preset),
    } : null;
    // The switch is "the computer changes the sound" when svolder sends one; an older svolder's
    // line has none, and the switch is the EQ's own, as it always was.
    const computerEq = view.computer_eq ? {
        label: 'Computer EQ',
        on: view.computer_eq.switch?.on ?? view.computer_eq.on,
        switchName: view.computer_eq.switch?.name ?? 'Computer EQ on or off',
        value: [view.computer_eq.preset, view.computer_eq.level].filter(Boolean).join(' \u00b7 '),
        choices: choiceRows({on: view.computer_eq.preset, choices: view.computer_eq.choices}),
    } : null;

    let path = null;
    if (!away && view.path) {
        const color = {good: COLORS.green, warn: COLORS.yellow, unknown: COLORS.stripGrey};
        path = {
            label: view.path.label,
            segments: view.path.segments.map(s => ({
                hop: s.hop,
                color: color[s.tone] ?? COLORS.stripGrey,
                tone: s.tone,
                words: s.words ?? '',
            })),
            off: view.path.off,
            offMarkup: view.path.off ? offMarkup(view.path.off) : '',
            // An older svolder's line has no rest line: its off words, in yellow, are what it drew.
            rest: view.path.rest ?? (view.path.off ? {text: view.path.off, tone: 'warn'} : {text: '', tone: 'plain'}),
        };
    }

    const scenes = {on: view.scenes.on, choices: choiceRows(view.scenes)};
    const note = dev ? null : 'no device yet: svolder setup';
    return {failure: null, note, header, volume, input, preset, computerEq, path, scenes};
}

// The line beside the Path strip: the segment's own words while the pointer is on segment
// `hovered` (an index, or null), else the rest line the panel gives. `tone` names the colour and
// `color` is it: good green, warn yellow, plain neutral; `markup` is ready for a label.
const toneColor = {good: COLORS.green, warn: COLORS.yellow};
// Every line starts with an invisible mark of the dB unit's size and rise, so a line with the unit
// and one without are the same height at any text scale (it leads, as an ellipsis cuts the end).
const lineHeightMark = () => popupUnit('', '\u200b', 12);
export function stripLine(path, hovered = null) {
    const seg = hovered === null ? undefined : path?.segments[hovered];
    if (seg && seg.words !== '') {
        const tone = seg.tone in toneColor ? seg.tone : 'plain';
        return {visible: true, tone, color: toneColor[tone] ?? COLORS.dim,
            markup: lineHeightMark() + escapeMarkup(seg.words)};
    }
    const rest = path?.rest;
    if (!rest || rest.text === '')
        return {visible: false, tone: 'plain', color: COLORS.dim, markup: `${lineHeightMark()}\u00a0`};
    const markup = rest.tone === 'warn' ? offMarkup(rest.text) : escapeMarkup(rest.text);
    return {visible: true, tone: rest.tone, color: toneColor[rest.tone] ?? COLORS.dim, markup: lineHeightMark() + markup};
}

// A line's words as the label draws them: a part (the words between two ` \u00b7 `) never breaks
// inside, so a line that wraps does so only at the separators, the dot staying with the part
// before it; a hyphen takes a word joiner after it, so `32-bit` stays whole. Only the text
// outside the markup's tags changes.
export function nobreak(markup) {
    return markup.split(/(<[^>]*>)/).map(chunk => chunk.startsWith('<')
        ? chunk
        : chunk.replaceAll(' \u00b7 ', '\u00a0\u00b7\u0001').replaceAll(' ', '\u00a0').replaceAll('\u0001', ' ')
            .replaceAll('-', '-\u2060')).join('');
}

// Every line the Path row can draw in this state, as label markup: the rest line and each
// segment's words (hovered), the ones with something to say. The indicator measures them and sizes
// the line for the widest, so hovering never changes the menu's width. A segment's words are drawn
// plain and the rest line's dB unit small, so the widest cannot be told by counting characters.
// `keep` is the markup the line is already sized for, kept as a candidate while the menu is open so
// the line never shrinks under the pointer (it only grows, on new words).
export function lineCandidates(path, keep = null) {
    const hovers = [null, ...(path?.segments ?? []).map((_, i) => i)];
    const found = hovers.map(hovered => stripLine(path, hovered)).filter(line => line.visible).map(line => line.markup);
    return keep !== null && keep !== undefined && !found.includes(keep) ? [...found, keep] : found;
}

export function parseLine(line) {
    let view;
    try {
        view = JSON.parse(line);
    } catch (_e) {
        return {view: null, error: 'unreadable line'};
    }
    if (typeof view !== 'object' || view === null)
        return {view: null, error: 'unreadable line'};
    const bad = versionFailure(view);
    if (bad)
        return {view: null, error: bad.headline, failure: bad};
    return {view, error: null};
}

// `#rrggbb` as three 0..1 numbers for cairo.
export function rgb(hex) {
    const n = parseInt(hex.slice(1), 16);
    return [(n >> 16 & 255) / 255, (n >> 8 & 255) / 255, (n & 255) / 255];
}
