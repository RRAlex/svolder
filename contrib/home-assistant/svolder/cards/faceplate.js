// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later
// `svolder-faceplate-card`: one DAC or amplifier. A header (name, firmware and rate, link), the
// volume dial beside the Mute and Phase invert keys and three readouts, the input and preset chips,
// and a one-line link strip. The dial writes ONE `number.set_value` when a gesture ends.
import {
  SvolderCard, esc, fmt, snap, numberOf, selectOf, switchOf, ageWord, firstId, register,
} from './shared.js';

const A0 = 135; // the dial's sweep starts here (degrees, clockwise from east) and runs 270
const SWEEP = 270;
const polar = (a, r) => [50 + r * Math.cos((a * Math.PI) / 180), 50 + r * Math.sin((a * Math.PI) / 180)];
const arc = (a0, a1, r) => {
  const [x0, y0] = polar(a0, r);
  const [x1, y1] = polar(a1, r);
  return `M${x0.toFixed(2)} ${y0.toFixed(2)}A${r} ${r} 0 ${a1 - a0 > 180 ? 1 : 0} 1 ${x1.toFixed(2)} ${y1.toFixed(2)}`;
};
const wrap = (d) => ((((d + 180) % 360) + 360) % 360) - 180;

/** The raw step (0 to 127) a dB value is: the daemon's own linear map over the entity's range. */
export const rawOf = (v, min, max) => Math.round(((v - min) / (max - min)) * 127);

const CSS = `
.fp { display: grid; grid-template-columns: minmax(0, 170px) minmax(0, 1fr); gap: 10px; align-items: center; margin: 0 0 10px; }
.dial { touch-action: none; cursor: grab; display: block; width: 100%; height: auto; }
.dial:focus-visible { outline: 2px solid var(--sv-on); border-radius: 50%; }
.dial.ro { cursor: default; }
.dial .trk { fill: none; stroke: var(--sv-line); stroke-width: 6; stroke-linecap: round; }
.dial .val { fill: none; stroke: var(--sv-on); stroke-width: 6; stroke-linecap: round; }
.dial .dot { fill: var(--sv-text); }
.dial .db { fill: var(--sv-text); font: 500 12.5px var(--sv-font); text-anchor: middle; }
.dial .raw { fill: var(--sv-dim); font: 8.5px var(--sv-font); text-anchor: middle; }
.ctl { display: grid; gap: 7px; align-content: center; }
.keys { display: flex; flex-wrap: wrap; gap: 6px; }
.rw b.bad { color: var(--sv-warn); }
.pr { display: grid; gap: 8px; margin: 0 0 10px; grid-template-columns: 1fr; }
@container (min-width: 460px) { .pr { grid-template-columns: auto 1fr; gap: 18px; } }
.bal { align-items: center; display: grid; grid-template-columns: auto minmax(0, 1fr) 3.5em; gap: 8px; }
.bal input { width: 100%; min-width: 0; accent-color: var(--sv-on); margin: 0; }
.bal b { text-align: right; }
.strip { display: flex; align-items: center; gap: 10px; border-top: 1px solid var(--sv-line); padding-top: 8px;
  color: var(--sv-dim); font-size: 11.5px; }
.strip b { color: var(--sv-text); font-weight: 500; }
`;

export class SvolderFaceplateCard extends SvolderCard {
  check(config) {
    if (!config.entities || typeof config.entities !== 'object') throw new Error('svolder-faceplate-card: entities is needed');
    return config;
  }

  static getStubConfig(hass) {
    const pick = (re) => firstId(hass, re);
    const entities = {
      volume: pick(/^number\..*_volume$/) || pick(/^sensor\..*_volume$/),
      mute: pick(/^(switch|binary_sensor)\..*_mute$/),
      phase: pick(/^(switch|binary_sensor)\..*_phase_invert$/),
      input: pick(/^(select|sensor)\..*_input$/),
      link: pick(/^binary_sensor\..*_link$/),
    };
    for (const k of Object.keys(entities)) if (!entities[k]) delete entities[k];
    return { type: 'custom:svolder-faceplate-card', name: 'DAC', entities };
  }

  read() {
    const E = this._cfg.entities;
    const h = this._hass;
    const text = (id) => (id && h.states[id] ? h.states[id] : null);
    const vol = numberOf(h, E.volume, { min: -96.33, max: 0, step: 0.334 });
    const link = switchOf(h, E.link);
    const sig = text(E.signal);
    const rate = text(E.sample_rate);
    const fw = text(E.firmware);
    const withUnit = (st) => (st ? `${st.state}${st.attributes && st.attributes.unit_of_measurement
      && !String(st.state).includes(st.attributes.unit_of_measurement) ? ` ${st.attributes.unit_of_measurement}` : ''}` : '');
    return {
      name: this._cfg.name || (E.volume && h.states[E.volume] && (h.states[E.volume].attributes || {}).device_name) || 'Device',
      fw: fw ? fw.state : '', rate: withUnit(rate),
      link: link && (link.known ? (link.on ? 'connected' : 'not connected') : 'unavailable'),
      linkOn: !!(link && link.on),
      vol: vol && { ...vol, value: Number.isFinite(vol.value) ? vol.value : null },
      mute: switchOf(h, E.mute), phase: switchOf(h, E.phase),
      spdif: switchOf(h, E.spdif), usb: switchOf(h, E.usb), usb1: switchOf(h, E.usb1), usb2: switchOf(h, E.usb2),
      balance: numberOf(h, E.balance, { min: -127, max: 127, step: 1 }),
      input: selectOf(h, E.input), preset: selectOf(h, E.preset),
      sig: sig ? { text: withUnit(sig), age: ageWord(sig) } : null,
    };
  }

  /** The link age ticks on its own: it is written into the strip, never a reason to rebuild the card. */
  signature(m) { return JSON.stringify({ ...m, sig: m.sig && { text: m.sig.text } }); }

  patch(m) {
    const age = this.shadowRoot.querySelector('.strip .age');
    if (age && m.sig && age.textContent !== m.sig.age) age.textContent = m.sig.age;
  }

  paint(m) {
    const word = (sw, yes, no) => (sw ? (sw.known ? (sw.on ? yes : no) : '—') : null);
    const rows = [['Sample rate', m.rate || null], ['S/PDIF', word(m.spdif, 'locked', 'unlocked')],
      ['USB', word(m.usb, 'connected', 'not connected')], ['USB 1', word(m.usb1, 'connected', 'not connected')], ['USB 2', word(m.usb2, 'connected', 'not connected')]]
      .filter(([, v]) => v);
    const key = (label, sw, field) => (sw ? `<button type="button" class="key" data-key="${field}" aria-pressed="${sw.on}"${
      sw.writable && sw.known ? '' : ' disabled'}>${esc(label)}</button>` : '');
    const chips = (label, sel, field) => (sel ? `<div><div class="leg">${esc(label)}</div><div class="chips" data-group="${field}">${
      (sel.options.length ? sel.options : [sel.state]).map((o) => `<button type="button" class="chip" data-option="${esc(o)}" aria-pressed="${
        o === sel.state}"${sel.writable ? '' : ' disabled'}>${esc(o)}</button>`).join('')}</div></div>` : '');
    const dial = m.vol ? this.dialSvg(m.vol) : '';
    const sub = [m.fw ? `fw ${m.fw}` : '', m.rate].filter(Boolean).join(' · ');
    const root = this.draw(`
      <div class="head"><span class="nm">${esc(m.name)}</span><span class="sub">${esc(sub)}</span>${m.link ? `<span class="link sp"><i class="lamp ${
        m.linkOn ? 'ok' : 'warn'}"></i>${esc(m.link)}</span>` : ''}</div>
      <div class="fp">${dial}<div class="ctl"><div class="keys">${key('Mute', m.mute, 'mute')}${key('Phase invert', m.phase, 'phase')}</div>${
        rows.map(([k, v]) => `<div class="rw"><span>${esc(k)}</span><b>${esc(v)}</b></div>`).join('')}${this.balanceRow(m.balance)}</div></div>
      ${m.input || m.preset ? `<div class="pr">${chips('Input', m.input, 'input')}${chips('Preset', m.preset, 'preset')}</div>` : ''}
      ${m.sig ? `<div class="strip"><span>Link <b>${esc(m.sig.text)}</b></span><span class="sp age">${esc(m.sig.age)}</span></div>` : ''}`, CSS);
    this.bind(root, m);
  }

  balanceRow(b) {
    if (!b) return '';
    const have = Number.isFinite(b.value);
    // a sensor, or an entity naming no range, has no honest thumb position: a readout line instead
    if (!b.writable || !b.bounded) return `<div class="rw"><span>Balance</span><b>${have ? esc(fmt(b.value, 0).replace('−', '-')) : '—'}</b></div>`;
    return `<div class="rw bal"><span>Balance</span><input type="range" data-balance aria-label="Balance" min="${b.min}" max="${b.max}" step="${b.step}" value="${
      have ? b.value : (b.min + b.max) / 2}"${b.writable && have ? '' : ' disabled'}><b>${have ? esc(fmt(b.value, 0).replace('−', '-')) : '—'}</b></div>`;
  }

  dialSvg(v) {
    const have = v.value !== null;
    // a volume sensor that names no range gets no arc and no raw step: the range would be a guess
    const frac = have && v.bounded ? (v.value - v.min) / (v.max - v.min) : 0;
    const a1 = A0 + SWEEP * Math.min(1, Math.max(0, frac));
    const [dx, dy] = polar(a1, 40);
    const label = have ? `${fmt(v.value)} dB` : '—';
    return `<svg class="dial${v.writable && have ? '' : ' ro'}" viewBox="0 0 100 100" role="slider" aria-label="Volume" tabindex="0"
      aria-valuemin="${v.min}" aria-valuemax="${v.max}" aria-valuenow="${have ? v.value : ''}" aria-valuetext="${esc(label)}"${
  v.writable ? '' : ' aria-readonly="true"'}><path class="trk" d="${arc(A0, A0 + SWEEP, 40)}"/><path class="val" d="${
  have && frac > 0 ? arc(A0, a1, 40) : ''}"/><circle class="dot" cx="${dx.toFixed(2)}" cy="${dy.toFixed(2)}" r="${have && v.bounded ? 3.2 : 0}"/>
      <text class="db" x="50" y="53">${esc(label)}</text><text class="raw" x="50" y="66">${
  have && v.bounded ? `raw ${rawOf(v.value, v.min, v.max)}/127` : ''}</text></svg>`;
  }

  bind(root, m) {
    for (const b of root.querySelectorAll('[data-key]')) {
      b.addEventListener('click', () => {
        const sw = m[b.dataset.key];
        if (!sw.writable || !sw.known) return;
        this.call('switch', sw.on ? 'turn_off' : 'turn_on', { entity_id: sw.id });
      });
    }
    for (const b of root.querySelectorAll('[data-option]')) {
      b.addEventListener('click', () => {
        const sel = m[b.parentElement.dataset.group];
        if (sel.writable && b.dataset.option !== sel.state) this.call('select', 'select_option', { entity_id: sel.id, option: b.dataset.option });
      });
    }
    const bal = root.querySelector('[data-balance]');
    if (bal && m.balance && m.balance.writable) {
      this.bindBalance(bal, m.balance);
    }
    const dial = root.querySelector('.dial');
    if (dial && m.vol && m.vol.writable && m.vol.value !== null) this.bindDial(dial, m.vol);
  }

  /** The slider previews while it moves and writes ONE value when it is let go. */
  bindBalance(input, bal) {
    const readout = input.parentElement.querySelector('b');
    let held = false;
    // the hold ends one tick after the gesture does, so a `change` that follows pointerup still writes first
    const end = () => setTimeout(() => { if (held) { held = false; this.release(); } }, 0);
    input.addEventListener('input', () => {
      if (!held) { held = true; this.hold(); }
      readout.textContent = String(Math.round(Number(input.value)));
    });
    input.addEventListener('change', () => {
      const value = Number(input.value);
      if (value !== bal.value) { this.stale(); this.call('number', 'set_value', { entity_id: bal.id, value }); }
      end();
    });
    for (const type of ['pointerup', 'pointercancel', 'keyup', 'blur']) input.addEventListener(type, end);
  }

  /** Drag turns the dial relative to where the pointer went down (a tap changes nothing); the one write comes on release. */
  bindDial(dial, vol) {
    const rawStep = (vol.max - vol.min) / 127;
    const step = Math.max(vol.step, rawStep > 0 ? Math.min(rawStep, vol.step * 4) : vol.step);
    let start = null;
    let now = vol.value;
    const show = () => {
      const frac = (now - vol.min) / (vol.max - vol.min);
      const a1 = A0 + SWEEP * frac;
      const [dx, dy] = polar(a1, 40);
      dial.querySelector('.val').setAttribute('d', frac > 0 ? arc(A0, a1, 40) : '');
      dial.querySelector('.dot').setAttribute('cx', dx.toFixed(2));
      dial.querySelector('.dot').setAttribute('cy', dy.toFixed(2));
      dial.querySelector('.db').textContent = `${fmt(now)} dB`;
      dial.querySelector('.raw').textContent = `raw ${rawOf(now, vol.min, vol.max)}/127`;
      dial.setAttribute('aria-valuenow', now);
      dial.setAttribute('aria-valuetext', `${fmt(now)} dB`);
    };
    const angle = (e) => {
      const r = dial.getBoundingClientRect();
      return (Math.atan2(e.clientY - (r.top + r.height / 2), e.clientX - (r.left + r.width / 2)) * 180) / Math.PI;
    };
    const finish = () => {
      if (!start) return;
      const moved = now !== start.value;
      start = null;
      if (moved) {
        this.stale();
        this.call('number', 'set_value', { entity_id: vol.id, value: now });
      }
      this.release();
    };
    dial.addEventListener('pointerdown', (e) => {
      if (start) return;
      this.hold();
      now = vol.value;
      start = { a: angle(e), value: vol.value, last: angle(e), turned: 0 };
      if (dial.setPointerCapture && e.pointerId !== undefined) { try { dial.setPointerCapture(e.pointerId); } catch { /* none */ } }
      e.preventDefault();
    });
    dial.addEventListener('pointermove', (e) => {
      if (!start) return;
      const a = angle(e);
      start.turned += wrap(a - start.last);
      start.last = a;
      now = snap(start.value + (start.turned / SWEEP) * (vol.max - vol.min), vol.min, vol.max, step);
      show();
    });
    dial.addEventListener('pointerup', finish);
    dial.addEventListener('pointercancel', finish);
    dial.addEventListener('keydown', (e) => {
      // End (the loudest value) is left out on purpose: no single key press should write 0 dB
      const d = { ArrowUp: 1, ArrowRight: 1, ArrowDown: -1, ArrowLeft: -1, PageUp: 10, PageDown: -10, Home: -1e6 }[e.key];
      if (!d) return;
      if (!start) { this.hold(); now = vol.value; start = { value: vol.value }; }
      now = snap(now + d * step, vol.min, vol.max, step);
      show();
      e.preventDefault();
    });
    dial.addEventListener('keyup', (e) => { if (/^(Arrow|Page|Home)/.test(e.key)) finish(); });
    dial.addEventListener('blur', finish);
  }

  getCardSize() { return 6; }
}

register('svolder-faceplate-card', SvolderFaceplateCard, 'svolder faceplate', 'One DAC or amplifier: volume dial, keys, inputs, presets, link.');
