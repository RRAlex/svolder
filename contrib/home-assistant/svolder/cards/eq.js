// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later
// `svolder-eq-card`: a short plot of the device's EQ curve, a one-line header (name, preset, Flat,
// Preset...), the band boxes on one line under it (frequency over gain) and a readout line with the
// selected band's Hz, dB and Q as typed number fields. Dragging a point previews it and, on release,
// writes the band's frequency and gain number entities once each (only the one that moved); a typed
// field writes its own entity once, on Enter, a stepper or leaving the field.
import {
  SvolderCard, esc, fmt, snap, numberOf, selectOf, firstId, register,
} from './shared.js';

const H = 96; // the plot is H tall in its own units and as wide as the card (`this.W`)
const F0 = 20;
const F1 = 20000;
const SPAN = Math.log10(F1 / F0);
const SAMPLE_RATE = 48000;

const xOf = (f, W) => (Math.log10(Math.max(F0, Math.min(F1, f)) / F0) / SPAN) * W;
const fOf = (x, W) => F0 * 10 ** ((Math.max(0, Math.min(W, x)) / W) * SPAN);

/** The gain in dB of one peaking band at `f` (RBJ biquad). */
export function peaking(f, band) {
  const w0 = (2 * Math.PI * band.f) / SAMPLE_RATE;
  const A = 10 ** (band.g / 40);
  const alpha = Math.sin(w0) / (2 * Math.max(0.1, band.q));
  const b = [1 + alpha * A, -2 * Math.cos(w0), 1 - alpha * A];
  const a = [1 + alpha / A, -2 * Math.cos(w0), 1 - alpha / A];
  const w = (2 * Math.PI * f) / SAMPLE_RATE;
  const mag = (c) => {
    const re = c[0] + c[1] * Math.cos(w) + c[2] * Math.cos(2 * w);
    const im = -(c[1] * Math.sin(w) + c[2] * Math.sin(2 * w));
    return re * re + im * im;
  };
  return 10 * Math.log10(mag(b) / mag(a));
}

const hz = (f) => (f >= 1000 ? `${(f / 1000).toFixed(2).replace(/0$/, '')} kHz` : `${Math.round(f)} Hz`);
const db = (g) => `${g >= 0 ? '+' : ''}${fmt(g)} dB`.replace('+−', '−');

const CSS = `
.eqh { display: flex; align-items: center; gap: 8px 10px; margin: 0 0 8px; flex-wrap: wrap; }
.eqh .nm { font-weight: 500; font-size: 14px; }
.eqh .sub { white-space: nowrap; }
.plot { display: block; width: 100%; height: auto; touch-action: none; }
.plot .grid { stroke: var(--sv-line); stroke-width: 1; vector-effect: non-scaling-stroke; }
.plot .zero { stroke: var(--sv-faint); stroke-width: 1; stroke-dasharray: 3 3; vector-effect: non-scaling-stroke; }
.plot .curve { fill: none; stroke: var(--sv-on); stroke-width: 1.8; vector-effect: non-scaling-stroke; }
.plot.off .curve { stroke: var(--sv-faint); }
.plot .pt { fill: var(--ha-card-background, var(--card-background-color, #141414)); stroke: var(--sv-dim); stroke-width: 1.6;
  vector-effect: non-scaling-stroke; }
.plot .pt.sel { stroke: var(--sv-on); fill: var(--sv-on); }
.plot .hit { fill: transparent; cursor: grab; }
.plot.ro .hit { cursor: default; }
.read { margin: 6px 0 0; color: var(--sv-dim); font-size: 12px; display: flex; flex-wrap: wrap; gap: 4px 6px; align-items: center; }
.read b { color: var(--sv-text); font-weight: 500; }
.read .fld, .read .grp { display: inline-flex; align-items: center; gap: 4px; flex-wrap: nowrap; white-space: nowrap; }
.read .grp { gap: 6px; }
.read input { width: 8.4ch; min-height: 30px; padding: 0 6px; border: 1px solid var(--sv-line); border-radius: 4px;
  background: var(--sv-field); color: var(--sv-text); text-align: right; min-width: 0; appearance: textfield; -moz-appearance: textfield; }
.read input[data-fld=q] { width: 7.6ch; }
.read input::-webkit-inner-spin-button, .read input::-webkit-outer-spin-button { -webkit-appearance: none; margin: 0; }
.read .key.sm { padding: 0 8px; min-width: 30px; justify-content: center; }
.bands { display: grid; gap: 4px; margin: 8px 0 0; }
.band { border: 1px solid var(--sv-line); border-radius: 4px; padding: 5px 2px; font-size: 10.5px; text-align: center;
  color: var(--sv-dim); background: transparent; display: block; line-height: 1.35; min-width: 0; }
.band b { display: block; color: var(--sv-text); font-weight: 500; font-size: 11.5px; }
.band[aria-pressed=true] { border-color: var(--sv-on); }
.presets, .modes { margin: 0 0 8px; }
`;

export class SvolderEqCard extends SvolderCard {
  check(config) {
    const E = config.entities;
    if (!E || !Array.isArray(E.bands) || !E.bands.length) throw new Error('svolder-eq-card: entities.bands is needed');
    return config;
  }

  static getStubConfig(hass) {
    const bands = [];
    for (let n = 1; n <= 5; n += 1) {
      const gain = firstId(hass, new RegExp(`^(number|sensor)\\..*_band_${n}_gain$`));
      const frequency = firstId(hass, new RegExp(`^(number|sensor)\\..*_band_${n}_frequency$`));
      if (gain && frequency) {
        bands.push({ n, gain, frequency, q: firstId(hass, new RegExp(`^(number|sensor)\\..*_band_${n}_q$`)) });
      }
    }
    return { type: 'custom:svolder-eq-card', name: 'EQ', entities: { bands } };
  }

  read() {
    const E = this._cfg.entities;
    const h = this._hass;
    const bands = E.bands.map((b, i) => {
      const f = numberOf(h, b.frequency, { min: 20, max: 20000, step: 1 });
      const g = numberOf(h, b.gain, { min: -12, max: 12, step: 0.1 });
      const q = numberOf(h, b.q, { min: 0.1, max: 10, step: 0.1 });
      return { n: b.n || i + 1, f, g, q };
    }).filter((b) => b.f && b.g);
    const mode = selectOf(h, E.mode);
    const strength = numberOf(h, E.strength, { min: 0, max: 127, step: 1 });
    const pre = numberOf(h, E.preamp, { min: -24, max: 0, step: 0.1 });
    return {
      name: this._cfg.name || 'EQ', bands, preset: selectOf(h, E.preset),
      mode, modeWord: mode && mode.state !== 'eq' && mode.state !== 'unavailable' ? mode.state : '',
      loud: strength && Number.isFinite(strength.value)
        ? (strength.value > 0 ? `${Math.round((strength.value / (strength.max || 127)) * 100)}%` : 'off') : null,
      pre: pre && Number.isFinite(pre.value) ? `${fmt(pre.value)} dB` : null,
      sel: Math.min(this._sel || 0, Math.max(0, bands.length - 1)), open: !!this._open,
    };
  }

  connectedCallback() {
    super.connectedCallback();
    if (typeof ResizeObserver === 'undefined') return;
    this._ro = new ResizeObserver(() => {
      if (Math.abs(this.width() - this.W) > 8) { this.stale(); this.refresh(); }
    });
    this._ro.observe(this);
  }

  disconnectedCallback() {
    super.disconnectedCallback();
    this._kb = null;
    this._edit = null;
    if (this._ro) this._ro.disconnect();
  }

  /** The plot's width in its own units: the card's width less its padding (300 where nothing is laid out). */
  width() {
    const w = this.getBoundingClientRect().width - 30;
    return w > 120 ? Math.round(w) : 300;
  }

  paint(m) {
    this._m = m;
    this.W = this.width();
    const writable = m.bands.some((b) => b.f.writable && b.g.writable);
    const opts = m.preset ? m.preset.options : [];
    const flat = opts.find((o) => /^flat$/i.test(o));
    const key = (label, attr) => `<button type="button" class="key sm" ${attr}${m.preset && m.preset.writable ? '' : ' disabled'}>${label}</button>`;
    const cols = Math.min(5, Math.max(1, m.bands.length));
    const root = this.draw(`
      <div class="eqh"><span class="nm">${esc(m.name)}</span>${m.preset ? `<span class="sub">${esc(m.preset.state)}</span>` : ''}<span class="sp"></span>${
        flat ? key('Flat', `data-flat="${esc(flat)}"`) : ''}${opts.length ? key('Preset…', `data-open aria-expanded="${m.open}"`) : ''}</div>
      ${m.mode ? `<div class="modes"><div class="leg">Mode</div><div class="chips" data-group="mode">${(m.mode.options.length ? m.mode.options : [m.mode.state]).map((o) => `<button type="button" class="chip" data-mode="${esc(o)}" aria-pressed="${
        o === m.mode.state}"${m.mode.writable ? '' : ' disabled'}>${esc(o)}</button>`).join('')}</div></div>` : ''}
      ${m.open && opts.length ? `<div class="chips presets">${opts.map((o) => `<button type="button" class="chip" data-preset="${esc(o)}" aria-pressed="${
        o === m.preset.state}"${m.preset.writable ? '' : ' disabled'}>${esc(o)}</button>`).join('')}</div>` : ''}
      <svg class="plot${m.modeWord ? ' off' : ''}${writable ? '' : ' ro'}" viewBox="0 0 ${this.W} ${H}" role="img" aria-label="EQ curve"></svg>
      <div class="read" aria-live="polite"></div>
      <div class="bands" style="grid-template-columns:repeat(${cols},minmax(0,1fr))"></div>
      <div class="foot"><span>${m.pre ? `Preamp <b>${esc(m.pre)}</b>` : ''}</span><span>${m.loud ? `Loudness <b>${esc(m.loud)}</b>` : ''}</span></div>`, CSS);
    this.root = root;
    this.live = m.bands.map((b) => ({ f: b.f.value, g: b.g.value }));
    this.drawPlot();
    this.bindDrag(root.querySelector('.plot'));
    this.bindKeys(root);
    for (const b of root.querySelectorAll('[data-flat]')) b.addEventListener('click', () => this.pick(b.dataset.flat));
    for (const b of root.querySelectorAll('[data-preset]')) b.addEventListener('click', () => this.pick(b.dataset.preset));
    for (const b of root.querySelectorAll('[data-mode]')) {
      b.addEventListener('click', () => {
        const md = this._m.mode;
        if (md && md.writable && b.dataset.mode !== md.state) this.call('select', 'select_option', { entity_id: md.id, option: b.dataset.mode });
      });
    }
    const open = root.querySelector('[data-open]');
    if (open) open.addEventListener('click', () => { this._open = !this._open; this.stale(); this.refresh(); });
  }

  pick(option) {
    const p = this._m.preset;
    if (p && p.writable && option !== p.state) this.call('select', 'select_option', { entity_id: p.id, option });
  }

  /** Draws the curve, the points, the band boxes and the readout from `this.live` (previews included). */
  drawPlot() {
    this._redrawing = true;
    try { this.keepFocus(() => this.plotMarkup()); } finally { this._redrawing = false; }
    const at = this.shadowRoot.activeElement;
    if (this._edit && !(at && at.dataset && at.dataset.fld)) this.endEdit();
  }

  plotMarkup() {
    const m = this._m;
    const root = this.root;
    const gMax = Math.max(6, ...m.bands.map((b) => Math.max(Math.abs(b.g.min), Math.abs(b.g.max))).map((v) => Math.min(v, 24)));
    const yOf = (g) => H / 2 - (Math.max(-gMax, Math.min(gMax, g)) / gMax) * (H / 2 - 4);
    const W = this.W;
    const bands = m.bands.map((b, i) => ({ f: this.live[i].f, g: this.live[i].g, q: b.q && Number.isFinite(b.q.value) ? b.q.value : 1 }))
      .filter((b) => Number.isFinite(b.f) && Number.isFinite(b.g));
    this.scale = { gMax, yOf };
    let pts = '';
    for (let i = 0; i <= 120; i += 1) {
      const f = fOf((i / 120) * W, W);
      pts += `${((i / 120) * W).toFixed(1)},${yOf(bands.reduce((s, b) => s + peaking(f, b), 0)).toFixed(1)} `;
    }
    const grid = [100, 1000, 10000].map((f) => `<line class="grid" x1="${xOf(f, W)}" x2="${xOf(f, W)}" y1="0" y2="${H}"/>`).join('');
    const dots = m.bands.map((b, i) => `<circle class="pt${i === m.sel ? ' sel' : ''}" data-i="${i}" cx="${xOf(this.live[i].f, W).toFixed(1)}" cy="${
      yOf(this.live[i].g).toFixed(1)}" r="4.5"/><circle class="hit" data-i="${i}" cx="${xOf(this.live[i].f, W).toFixed(1)}" cy="${
      yOf(this.live[i].g).toFixed(1)}" r="13"/>`).join('');
    root.querySelector('.plot').innerHTML = `${grid}<line class="zero" x1="0" x2="${W}" y1="${H / 2}" y2="${H / 2}"/><polyline class="curve" points="${pts.trim()}"/>${dots}`;
    root.querySelector('.bands').innerHTML = m.bands.map((b, i) => `<button type="button" class="band" data-i="${i}" aria-pressed="${i === m.sel}" aria-label="Band ${esc(b.n)}, ${
      esc(hz(this.live[i].f))}, ${esc(db(this.live[i].g))}"><b>${esc(hz(this.live[i].f))}</b>${esc(db(this.live[i].g))}</button>`).join('');
    const b = m.bands[m.sel];
    const q = b && b.q;
    const live = this.live[m.sel];
    // a writable value is a number field with its unit after it; one that is not (commands off) is text
    const field = (key, ent, value, shown, unit, text) => (ent && ent.writable && Number.isFinite(value)
      ? `<label class="fld"><input type="number" data-fld="${key}" data-i="${m.sel}" min="${ent.min}" max="${ent.max}" step="${
        key === 'q' ? 'any' : ent.step}" value="${shown}" aria-label="${unit} of band ${esc(b.n)}">${unit}</label>`
      : `<span><b>${Number.isFinite(value) ? esc(text) : '—'}</b></span>`);
    root.querySelector('.read').innerHTML = b ? `<span>Band ${esc(b.n)}:</span> ${
      field('f', b.f, live.f, this.shown('f', live.f), 'Hz', hz(live.f))} ${
      field('g', b.g, live.g, this.shown('g', live.g), 'dB', db(live.g))} ${
      q && Number.isFinite(q.value) ? `<span class="grp">${field('q', q, q.value, this.shown('q', q.value), 'Q', `Q ${q.value.toFixed(2)}`)} ${
        q.writable ? `<button type="button" class="key sm" data-q="-1" aria-label="Lower Q">−</button> <button type="button" class="key sm" data-q="1" aria-label="Raise Q">+</button>` : ''}</span>` : ''} ${
      m.modeWord ? `<span class="skip">Bands apply in eq mode (now ${esc(m.modeWord)}).</span>` : ''}` : '';
    for (const el of root.querySelectorAll('.band')) {
      el.addEventListener('click', () => { this._sel = Number(el.dataset.i); this.stale(); this.refresh(); });
      el.addEventListener('keydown', (e) => this.key(e, Number(el.dataset.i)));
    }
    for (const el of root.querySelectorAll('[data-q]')) {
      el.addEventListener('click', () => {
        const qq = this._m.bands[this._m.sel].q;
        if (!qq.writable) return;
        const v = this.stepOf('q', qq, Number(el.dataset.q), NaN);
        if (v !== qq.value) this.call('number', 'set_value', { entity_id: qq.id, value: v });
      });
    }
    for (const el of root.querySelectorAll('input[data-fld]')) this.bindField(el);
  }

  /** `n` as the entity takes it: Hz to the step counted from 0 (the entity's minimum may be fractional),
   * the rest counted from the minimum, all held to min..max. */
  entityValue(key, ent, n) {
    if (key !== 'f') return snap(n, ent.min, ent.max, ent.step);
    return Math.min(ent.max, Math.max(ent.min, Math.round(n / ent.step) * ent.step));
  }

  /** The value one arrow step from `from` (NaN: the entity's own value). Q moves by at least 12 %, since
   * the entity's own step (0.01) is finer than the device's (one raw step is about 0.11 above the
   * centre); Hz by 3 % as the band boxes do; dB by the entity step. */
  stepOf(key, ent, dir, from) {
    const base = Number.isFinite(from) ? from : ent.value;
    if (key === 'q') return snap(dir > 0 ? base * 1.12 : base / 1.12, ent.min, ent.max, ent.step);
    if (key === 'f') return this.entityValue('f', ent, base + dir * Math.max(ent.step, Math.round(base * 0.03)));
    return snap(base + dir * ent.step, ent.min, ent.max, ent.step);
  }

  /** One typed field: focus holds the card (an incoming state does not repaint it). Enter or leaving
   * the field writes a changed value once; the arrow keys step the field and write once on key up (a
   * held key is one write). Escape and an empty entry put the last value back with no write. */
  bindField(el) {
    const settle = () => {
      if (this._redrawing || !el.isConnected) return;
      this.commitField(el);
      this.endEdit();
    };
    el.addEventListener('focus', () => {
      if (this._edit) return;
      this._edit = { sent: {} };
      this.hold();
    });
    el.addEventListener('change', () => { if (!this._arrow) this.commitField(el); });
    el.addEventListener('blur', settle);
    el.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        e.preventDefault();
        settle();
        el.blur();
      } else if (e.key === 'Escape') {
        e.preventDefault();
        el.value = el.defaultValue;
        settle();
        el.blur();
      } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
        e.preventDefault();
        const ent = this.fresh(el);
        if (!ent) return;
        const key = el.dataset.fld;
        const raw = el.value.trim();
        const v = this.stepOf(key, ent, e.key === 'ArrowUp' ? 1 : -1, raw === '' ? NaN : Number(raw));
        el.value = this.shown(key, v);
        this._arrow = true;
      }
    });
    el.addEventListener('keyup', (e) => {
      if (this._arrow && /^Arrow(Up|Down)$/.test(e.key)) {
        this._arrow = false;
        this.commitField(el);
      }
    });
  }

  /** The text a field shows for `v`: whole Hz, dB with one decimal, Q with two. */
  shown(key, v) {
    return key === 'q' ? v.toFixed(2) : key === 'g' ? v.toFixed(1) : String(Math.round(v));
  }

  /** The field's entity as the state has it now (not as it was when the card last painted), or null. */
  fresh(el) {
    const b = this._m.bands[Number(el.dataset.i)];
    const ent = b && b[el.dataset.fld];
    if (!ent || !ent.writable) return null;
    const now = numberOf(this._hass, ent.id, ent);
    return now && Number.isFinite(now.value) ? now : null;
  }

  /** Writes the field's entity when its text was changed and the value it makes differs from the last
   * one written during this focus (else the state's). */
  commitField(el) {
    if (el.value === el.defaultValue) return;
    const ent = this.fresh(el);
    const raw = el.value.trim();
    const typed = raw === '' ? NaN : Number(raw);
    if (!ent || !Number.isFinite(typed)) { el.value = el.defaultValue; return; }
    const key = el.dataset.fld;
    const v = this.entityValue(key, ent, typed);
    const sent = this._edit ? this._edit.sent : {};
    if (v !== (ent.id in sent ? sent[ent.id] : ent.value)) {
      sent[ent.id] = v;
      this.call('number', 'set_value', { entity_id: ent.id, value: v });
      // the device may round it back: the next state paints what it has
      this.stale();
    }
    el.value = this.shown(key, v);
    el.defaultValue = el.value;
  }

  /** The field is left: the card paints again (a state that came meanwhile shows now). */
  endEdit() {
    if (!this._edit) return;
    this._edit = null;
    this._arrow = false;
    this.release();
  }

  /** Sends what moved of band `i` (frequency and gain, each only if it changed) and ends the gesture. */
  commit(i, start) {
    const b = this._m.bands[i];
    const now = this.live[i];
    this.stale();
    if (now.f !== start.f && b.f.writable) this.call('number', 'set_value', { entity_id: b.f.id, value: now.f });
    if (now.g !== start.g && b.g.writable) this.call('number', 'set_value', { entity_id: b.g.id, value: now.g });
    this.release();
  }

  /** Arrow keys on a band box: left and right move the frequency, up and down the gain; one write on key up. */
  key(e, i) {
    const m = this._m;
    const b = m.bands[i];
    const d = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, 1], ArrowDown: [0, -1] }[e.key];
    if (!d || !b.f.writable || !b.g.writable) return;
    e.preventDefault();
    if (!this._kb) {
      this.hold();
      this._kb = { i, start: { ...this.live[i] } };
    } else if (this._kb.i !== i) return;
    this._sel = i;
    m.sel = i;
    const cur = this.live[i];
    this.live[i] = {
      f: d[0] ? snap(cur.f + d[0] * Math.max(b.f.step, Math.round(cur.f * 0.03)), b.f.min, b.f.max, b.f.step) : cur.f,
      g: d[1] ? snap(cur.g + d[1] * b.g.step, b.g.min, b.g.max, b.g.step) : cur.g,
    };
    this.drawPlot();
  }

  /** Pointer handling on the plot, bound once per paint; the points are redrawn under it during a drag. */
  bindDrag(svg) {
    const m = this._m;
    if (svg.classList.contains('ro')) return;
    let drag = null;
    const at = (e) => {
      const r = svg.getBoundingClientRect();
      return [((e.clientX - r.left) / (r.width || 1)) * this.W, ((e.clientY - r.top) / (r.height || 1)) * H];
    };
    const finish = () => {
      if (!drag) return;
      const { i, start } = drag;
      drag = null;
      this.commit(i, start);
    };
    svg.addEventListener('pointerdown', (e) => {
      const el = e.target;
      if (drag || !el.classList || !el.classList.contains('hit')) return;
      const i = Number(el.dataset.i);
      if (!m.bands[i].f.writable || !m.bands[i].g.writable) return;
      // the drag holds the card first, so leaving a typed field (which writes its text) cannot
      // repaint the plot under the pointer
      this.hold();
      const typing = this.shadowRoot.activeElement;
      if (typing && typing.dataset && typing.dataset.fld) typing.blur();
      this._sel = i;
      const [px, py] = at(e);
      drag = { i, start: { ...this.live[i] }, dx: px - xOf(this.live[i].f, this.W), dy: py - this.scale.yOf(this.live[i].g) };
      if (svg.setPointerCapture && e.pointerId !== undefined) { try { svg.setPointerCapture(e.pointerId); } catch { /* none */ } }
      e.preventDefault();
    });
    svg.addEventListener('pointermove', (e) => {
      if (!drag) return;
      const b = m.bands[drag.i];
      const [px, py] = at(e);
      const x = px - drag.dx;
      const y = py - drag.dy;
      const { gMax } = this.scale;
      this.live[drag.i] = {
        // at the plot's edge the frequency snaps to the band's own limit: harmless, the entity clamps it too
        f: x === xOf(drag.start.f, this.W) ? drag.start.f : snap(fOf(x, this.W), b.f.min, b.f.max, b.f.step),
        g: snap(((H / 2 - y) / (H / 2 - 4)) * gMax, b.g.min, b.g.max, b.g.step),
      };
      m.sel = drag.i;
      this.drawPlot();
    });
    svg.addEventListener('pointerup', finish);
    svg.addEventListener('pointercancel', finish);
  }

  /** The key's gesture ends on the arrow's key up, wherever in the card the focus is. */
  bindKeys(root) {
    const end = () => {
      if (!this._kb) return;
      const { i, start } = this._kb;
      this._kb = null;
      this.commit(i, start);
    };
    root.addEventListener('keyup', (e) => { if (/^Arrow/.test(e.key)) end(); });
    // focus leaving the card with an arrow held ends the gesture too (not the redraw's own focus move)
    root.addEventListener('focusout', (e) => {
      if (this._redrawing || (e.relatedTarget && root.contains(e.relatedTarget))) return;
      end();
    });
  }

  getCardSize() { return 5; }
}

register('svolder-eq-card', SvolderEqCard, 'svolder EQ', 'The EQ curve of one device: drag a point to move a band.');
