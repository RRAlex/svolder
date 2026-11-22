// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later
// What every svolder card shares: the base element, its stylesheet and a few formatters. A card
// reads `hass.states` of the entities its config names, paints a shadow root, and writes only
// through `hass.callService` on those same entities. Nothing here imports Home Assistant.

/** `text` safe inside markup and attribute values. */
export const esc = (text) => String(text ?? '').replace(/[&<>"']/g, (c) => (
  { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));

/** The domain of an entity id. */
export const domainOf = (id) => String(id).split('.')[0];

/** A number as text with a real minus sign and `digits` decimals. */
export const fmt = (n, digits = 1) => `${n < 0 ? '−' : ''}${Math.abs(n).toFixed(digits)}`;

/** `n` held to `lo..hi` and snapped to `step` counted from `lo`. */
export function snap(n, lo, hi, step) {
  let v = Math.min(hi, Math.max(lo, n));
  if (step > 0) v = Math.min(hi, lo + Math.round((v - lo) / step) * step);
  return Math.round(v * 1000) / 1000;
}

const num = (v) => (v === '' || v === null || v === undefined ? NaN : Number(v));

/**
 * A number entity as the card shows it: `{ id, value, min, max, step, unit, writable }`, value NaN
 * while it is unavailable. `writable` only for a real `number` entity: with commands off the same
 * control is a sensor, and the card draws it without a way to move it.
 */
export function numberOf(hass, id, fallback = {}) {
  const st = id && hass.states[id];
  if (!st) return null;
  const a = st.attributes || {};
  return {
    id, value: num(st.state), unit: a.unit_of_measurement || '',
    bounded: Number.isFinite(num(a.min)) && Number.isFinite(num(a.max)),
    min: Number.isFinite(num(a.min)) ? num(a.min) : fallback.min ?? 0,
    max: Number.isFinite(num(a.max)) ? num(a.max) : fallback.max ?? 127,
    step: num(a.step) > 0 ? num(a.step) : fallback.step ?? 1,
    writable: domainOf(id) === 'number',
  };
}

/** A select (or the sensor it becomes) as `{ id, state, options, writable }`. */
export function selectOf(hass, id) {
  const st = id && hass.states[id];
  if (!st) return null;
  const options = Array.isArray(st.attributes && st.attributes.options) ? st.attributes.options : [];
  return { id, state: st.state, options, writable: domainOf(id) === 'select' && options.length > 0 };
}

/** A switch (or the binary sensor it becomes) as `{ id, on, known, writable }`. */
export function switchOf(hass, id) {
  const st = id && hass.states[id];
  if (!st) return null;
  return { id, on: st.state === 'on', known: st.state === 'on' || st.state === 'off', writable: domainOf(id) === 'switch' };
}

/** The age of a state as a word: "12 s", "30 min", "3 h", "2 d". */
export function ageWord(st, now = Date.now()) {
  const t = Date.parse(st && (st.last_updated || st.last_changed));
  if (!Number.isFinite(t)) return '';
  const s = Math.max(0, Math.round((now - t) / 1000));
  if (s < 90) return `${s} s`;
  if (s < 5400) return `${Math.round(s / 60)} min`;
  if (s < 129600) return `${Math.round(s / 3600)} h`;
  return `${Math.round(s / 86400)} d`;
}

/** The stylesheet: colours from Home Assistant's theme, the svolder theme's values as fallbacks. */
export const CSS = `
:host { display: block; --sv-text: var(--primary-text-color, #f2f0ea); --sv-dim: var(--secondary-text-color, #7a7a7a);
  --sv-faint: var(--disabled-text-color, #5a5a5a); --sv-line: var(--divider-color, #1e1e1e);
  --sv-on: var(--state-active-color, var(--primary-color, #e0341c)); --sv-ok: var(--success-color, #6fbf73);
  --sv-warn: var(--warning-color, #d9a441); --sv-bad: var(--error-color, #ff5a4a);
  --sv-field: var(--input-fill-color, #1e1e1e);
  --sv-font: "JetBrains Mono", "DejaVu Sans Mono", "Liberation Mono", ui-monospace, monospace; }
* { box-sizing: border-box; }
.card { container-type: inline-size; background: var(--ha-card-background, var(--card-background-color, #141414));
  border: var(--ha-card-border-width, 1px) solid var(--ha-card-border-color, var(--sv-line));
  border-radius: var(--ha-card-border-radius, 6px); padding: 14px; color: var(--sv-text);
  font: 13px/1.4 var(--sv-font); min-width: 0; }
.head { display: flex; align-items: center; flex-wrap: wrap; gap: 4px 10px; margin: 0 0 10px; }
.head .nm { font-weight: 500; font-size: 14px; }
.sub, .dim { color: var(--sv-dim); font-size: 11.5px; }
.sp { margin-left: auto; }
.lamp { display: inline-block; width: 8px; height: 8px; border-radius: 50%; border: 1.5px solid var(--sv-faint);
  flex: none; vertical-align: middle; }
.lamp.on { background: var(--sv-on); border-color: var(--sv-on); }
.lamp.ok { background: var(--sv-ok); border-color: var(--sv-ok); }
.lamp.warn { background: var(--sv-warn); border-color: var(--sv-warn); }
.link { display: inline-flex; align-items: center; gap: 6px; color: var(--sv-dim); font-size: 11.5px; }
button, select, input { font: inherit; color: inherit; }
button { cursor: pointer; }
button:disabled, select:disabled { cursor: default; }
.key, .chip { min-height: 36px; padding: 0 12px; border: 1px solid var(--sv-line); border-radius: 4px;
  background: transparent; color: var(--sv-dim); display: inline-flex; align-items: center; gap: 7px; }
.key { color: var(--sv-text); }
.key.sm { min-height: 30px; padding: 0 10px; font-size: 12px; }
.key[aria-pressed=true], .chip[aria-pressed=true] { border-color: var(--sv-on); box-shadow: inset 0 0 0 1px var(--sv-on);
  color: var(--sv-text); }
.chip[aria-pressed=true]::before { content: ""; width: 8px; height: 8px; border-radius: 50%; background: var(--sv-on); flex: none; }
.key:disabled:not([aria-pressed=true]), .chip:disabled:not([aria-pressed=true]) { color: var(--sv-faint); }
.chip:not(:disabled):hover { color: var(--sv-text); }
:focus-visible { outline: 2px solid var(--sv-on); outline-offset: 1px; }
.chips { display: flex; flex-wrap: wrap; gap: 6px; }
.leg { font-size: 10.5px; letter-spacing: .14em; text-transform: uppercase; color: var(--sv-dim); margin: 0 0 6px; }
.rw { display: flex; justify-content: space-between; gap: 8px; color: var(--sv-dim); font-size: 11.5px;
  border-top: 1px solid var(--sv-line); padding-top: 5px; }
.rw b, .foot b { color: var(--sv-text); font-weight: 500; }
.foot { display: flex; align-items: center; justify-content: space-between; gap: 10px; border-top: 1px solid var(--sv-line);
  padding-top: 8px; margin-top: 10px; color: var(--sv-dim); font-size: 11.5px; }
.na { color: var(--sv-faint); }
.say { margin: 10px 0 0; color: var(--sv-dim); font-size: 12px; min-height: 1.4em; }
.say b { color: var(--sv-text); font-weight: 500; }
.say .skip { color: var(--sv-warn); }
.say .bad { color: var(--sv-bad); }
`;

/** The base of every card: config, hass, and a paint that waits while a gesture is under way. */
export class SvolderCard extends HTMLElement {
  constructor() {
    super();
    this.attachShadow({ mode: 'open' });
    this._cfg = null;
    this._hass = null;
    this._sig = null;
    this._held = 0;
    this._dirty = false;
  }

  /** A view Home Assistant keeps cached may detach the card mid-gesture: nothing stays held. */
  disconnectedCallback() {
    this._held = 0;
    this._dirty = false;
    this._sig = null;
  }

  connectedCallback() { this.refresh(); }

  /** What decides a repaint: the model as text. A card leaves out what it patches in place. */
  signature(model) { return JSON.stringify(model); }

  /** Called with the model when it changed only in what `signature` leaves out. */
  patch() {}

  /** Subclasses return the config to keep, or throw `Error` for one they cannot draw. */
  check(config) { return config; }

  setConfig(config) {
    if (!config || typeof config !== 'object') throw new Error('svolder card: a config is needed');
    this._cfg = this.check(config) || config;
    this._sig = null;
    this.refresh();
  }

  set hass(hass) {
    this._hass = hass;
    this.refresh();
  }

  get hass() { return this._hass; }

  /** Paints again when what is shown changed, and not during a gesture. */
  refresh() {
    if (!this._cfg || !this._hass) return;
    this.themed();
    if (this._held) { this._dirty = true; return; }
    const model = this.read();
    const sig = this.signature(model);
    if (sig === this._sig) { this.patch(model); return; }
    this._sig = sig;
    this.paint(model);
  }

  /** Home Assistant switched theme or mode: the accent is worked out again, with no repaint. */
  themed() {
    const t = this._hass.themes;
    const key = t ? `${t.theme}|${t.darkMode}` : '';
    if (key === this._theme) return;
    const first = this._theme === undefined;
    this._theme = key;
    if (first) return;
    const again = () => {
      const card = this.shadowRoot.querySelector('.card');
      if (card) accent(card);
    };
    again();
    // the theme's variables land on the page a moment after `hass.themes` changes
    if (typeof requestAnimationFrame === 'function') requestAnimationFrame(again);
  }

  /** A gesture starts: no repaint (it would drop the element under the pointer). */
  hold() { this._held += 1; }

  /** The gesture ended; a repaint that waited runs now. */
  release() {
    this._held = Math.max(0, this._held - 1);
    if (!this._held && this._dirty) { this._dirty = false; this.refresh(); }
  }

  /** Forces the next refresh to paint (what is shown may be a preview the daemon did not take). */
  stale() { this._sig = null; }

  /** One service call on one entity of this card. */
  call(domain, service, data) {
    if (this._hass && this._hass.callService) this._hass.callService(domain, service, data);
  }

  /** Paints `html` into the shadow root under the shared stylesheet; the focused control keeps focus. */
  draw(html, extraCss = '') {
    return this.keepFocus(() => {
      this.shadowRoot.innerHTML = `<style>${CSS}${extraCss}</style><div class="card">${html}</div>`;
      const card = this.shadowRoot.querySelector('.card');
      accent(card);
      return card;
    });
  }

  /** Runs `fn` (which rebuilds markup) and gives focus back to the same control, by a stable key. */
  keepFocus(fn) {
    const root = this.shadowRoot;
    const at = root.activeElement;
    const key = at ? focusKey(at) : null;
    const sel = at && typeof at.selectionStart === 'number' ? [at.selectionStart, at.selectionEnd] : null;
    const out = fn();
    if (key) {
      const again = root.querySelector(key);
      if (again && again !== root.activeElement) {
        again.focus({ preventScroll: true });
        if (sel && typeof again.setSelectionRange === 'function') again.setSelectionRange(sel[0], sel[1]);
      }
    }
    return out;
  }

  getCardSize() { return 4; }

  getGridOptions() { return { columns: 12, min_columns: 6, rows: 'auto' }; }
}

/** A selector that finds `el` again after the markup is rebuilt: its tag, class and data-*, under its group. */
export function focusKey(el) {
  const q = (v) => String(v).replace(/["\\]/g, '\\$&');
  let key = el.localName + (el.classList.length ? `.${CSS_ESC(el.classList[0])}` : '');
  for (const [k, v] of Object.entries(el.dataset)) key += `[data-${k.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}="${q(v)}"]`;
  const group = el.closest('[data-group]');
  if (group && group !== el) key = `[data-group="${q(group.dataset.group)}"] ${key}`;
  return key;
}
const CSS_ESC = (v) => String(v).replace(/[^a-zA-Z0-9_-]/g, '\\$&');

/** Parses `rgb(...)` / `rgba(...)`: `[r, g, b, a]` or null. */
function parseRgb(css) {
  const m = /rgba?\(([^)]+)\)/.exec(css || '');
  if (!m) return null;
  const n = m[1].replace('/', ' ').split(/[\s,]+/).filter(Boolean).map(Number);
  return n.length >= 3 && n.every(Number.isFinite) ? [n[0], n[1], n[2], n.length > 3 ? n[3] : 1] : null;
}
const lum = (c) => {
  const f = (v) => { const x = v / 255; return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4; };
  return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]);
};
const ratio = (a, b) => { const [hi, lo] = [lum(a), lum(b)].sort((x, y) => y - x); return (hi + 0.05) / (lo + 0.05); };

/**
 * The accent marks (dial arc, curve, lamp, pressed ring) need 3:1 against the card. A theme whose
 * active colour is paler than that (Home Assistant's default amber on white) gets it blended toward
 * the text colour until it holds. Nothing is touched where it already does, or where nothing is laid out.
 */
export function accent(card) {
  if (typeof getComputedStyle !== 'function') return;
  card.style.setProperty('--sv-on', '');
  const probe = document.createElement('i');
  probe.style.cssText = 'position:absolute;visibility:hidden;color:var(--sv-on)';
  card.appendChild(probe);
  const cs = getComputedStyle(card);
  const on = parseRgb(getComputedStyle(probe).color);
  const bg = parseRgb(cs.backgroundColor);
  const text = parseRgb(cs.color);
  probe.remove();
  if (!on || !bg || !text || bg[3] < 0.99 || ratio(on, bg) >= 3) return;
  for (let t = 0.1; t <= 1.001; t += 0.05) {
    const mix = on.slice(0, 3).map((v, i) => Math.round(v * (1 - t) + text[i] * t));
    if (ratio(mix, bg) >= 3.05 || t > 0.99) { card.style.setProperty('--sv-on', `rgb(${mix.join(',')})`); return; }
  }
}

/** The first entity id in `hass.states` whose id matches `re`, or undefined. */
export function firstId(hass, re) {
  return Object.keys((hass && hass.states) || {}).sort().find((id) => re.test(id));
}

/** Defines `tag` once and lists it for Home Assistant's card picker. */
export function register(tag, cls, name, description) {
  if (!customElements.get(tag)) customElements.define(tag, cls);
  if (typeof window !== 'undefined') {
    window.customCards = window.customCards || [];
    if (!window.customCards.some((c) => c.type === tag)) {
      window.customCards.push({ type: tag, name, description, preview: false });
    }
  }
}
