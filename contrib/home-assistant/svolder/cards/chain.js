// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later
// `svolder-chain-card`: the computer's sound chain as four keyed modules, numbered 1 to 4 with a
// coloured top edge: EQ, Loudness, Crossfeed, Speakers. Each has its switch, one setting, a tiny
// picture and a word of why. A control is writable only when its entity is (commands on); with
// commands off the module shows the same words without a way to change them.
import {
  SvolderCard, esc, numberOf, selectOf, switchOf, firstId, register,
} from './shared.js';

const PIC = {
  eq: '<svg viewBox="0 0 48 20" aria-hidden="true"><path d="M2 12 C10 12 12 4 20 4 S30 16 38 14 44 10 46 10"/></svg>',
  loud: '<svg viewBox="0 0 48 20" aria-hidden="true"><path d="M2 14 C8 14 8 10 14 6 L16 6 C22 8 32 12 46 12"/></svg>',
  xf: '<svg viewBox="0 0 48 20" aria-hidden="true"><path d="M4 4 C20 4 28 16 44 16 M4 16 C20 16 28 4 44 4"/></svg>',
  sp: '<svg viewBox="0 0 48 20" aria-hidden="true"><path d="M6 6 L14 6 L20 2 L20 18 L14 14 L6 14Z M30 6 L38 6 L44 2 L44 18 L38 14 L30 14Z"/></svg>',
};

const CSS = `
.mods { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 8px; }
@container (min-width: 560px) { .mods { grid-template-columns: repeat(4, minmax(0, 1fr)); } }
.mod { border: 1px solid var(--sv-line); border-top: 3px solid var(--sv-faint); border-radius: 4px; padding: 8px 9px;
  display: grid; gap: 6px; align-content: start; min-width: 0; }
.mod.on { border-top-color: var(--sv-on); }
.mod.clash { border-top-color: var(--sv-warn); }
.mod .top { display: flex; align-items: center; gap: 6px; }
.mod .no { color: var(--sv-dim); font-size: 10.5px; letter-spacing: .1em; }
.mod .nm { font-weight: 500; }
.mod .key { min-height: 30px; padding: 0 10px; font-size: 12px; justify-content: center; }
.mod .set { width: 100%; min-width: 0; min-height: 30px; border: 1px solid var(--sv-line); border-radius: 4px;
  background: var(--sv-field); padding: 0 6px; }
.mod input[type=range] { width: 100%; accent-color: var(--sv-on); }
.mod .val { font-size: 12px; }
.mod .val b { font-weight: 500; }
.mod svg { width: 100%; height: 20px; fill: none; stroke: var(--sv-faint); stroke-width: 1.5; vector-effect: non-scaling-stroke; }
.mod.on svg { stroke: var(--sv-on); }
.mod .why { color: var(--sv-dim); font-size: 10.5px; min-height: 1.4em; }
.mod.clash .why { color: var(--sv-warn); }
`;

export class SvolderChainCard extends SvolderCard {
  check(config) {
    if (!config.entities || typeof config.entities !== 'object') throw new Error('svolder-chain-card: entities is needed');
    return config;
  }

  static getStubConfig(hass) {
    const pick = (re) => firstId(hass, re);
    const entities = {
      eq: pick(/^(switch|binary_sensor)\.(?!.*(loudness|crossfeed|virtual)).*_eq$/),
      loudness: pick(/^(switch|binary_sensor)\..*_loudness$/),
      crossfeed: pick(/^(switch|binary_sensor)\..*_crossfeed$/),
      speakers: pick(/^(switch|binary_sensor)\..*_virtual_speakers$/),
    };
    for (const k of Object.keys(entities)) if (!entities[k]) delete entities[k];
    return { type: 'custom:svolder-chain-card', name: 'Chain', entities };
  }

  read() {
    const E = this._cfg.entities;
    const h = this._hass;
    const sw = (id) => switchOf(h, id);
    const path = E.path && h.states[E.path];
    const cfg = E.config && h.states[E.config];
    return {
      eq: { sw: sw(E.eq), preset: selectOf(h, E.eq_preset), pre: numberOf(h, E.preamp, { min: -24, max: 0, step: 0.1 }) },
      loud: { sw: sw(E.loudness), n: numberOf(h, E.loudness_strength, { min: 0, max: 127, step: 1 }) },
      xf: { sw: sw(E.crossfeed), sel: selectOf(h, E.crossfeed_preset) },
      sp: { sw: sw(E.speakers), sel: selectOf(h, E.speakers_set) },
      path: path ? path.state : null,
      config: cfg ? cfg.state : null,
    };
  }

  paint(m) {
    const pct = (n) => (Number.isFinite(n.value) ? `${Math.round((n.value / (n.max || 127)) * 100)}%` : '—');
    const mods = [];
    const both = m.xf.sw && m.sp.sw && m.xf.sw.on && m.sp.sw.on;
    const have = (n) => n && Number.isFinite(n.value);
    const eqSetting = m.eq.pre
      ? (m.eq.pre.writable ? this.range('pre', m.eq.pre, have(m.eq.pre) ? `${m.eq.pre.value} dB` : '\u2014')
        : `<span class="val">Preamp <b>${have(m.eq.pre) ? `${esc(m.eq.pre.value)} dB` : '\u2014'}</b></span>`)
      : (m.eq.preset ? `<span class="val">Preset <b>${esc(m.eq.preset.state)}</b></span>` : '');
    const loudSetting = m.loud.n ? (m.loud.n.writable ? this.range('strength', m.loud.n, pct(m.loud.n))
      : `<span class="val">Strength <b>${esc(pct(m.loud.n))}</b></span>`) : '';
    const sel = (key, s) => (s ? (s.writable ? `<select class="set" data-sel="${key}" aria-label="${key === 'xf' ? 'Crossfeed level' : 'Speakers'}">${
      s.options.includes(s.state) ? '' : `<option value="" disabled selected>${esc(s.state)}</option>`}${s.options.map((o) => `<option value="${esc(o)}"${o === s.state ? ' selected' : ''}>${esc(o)}</option>`).join('')}</select>`
      : `<span class="val"><b>${esc(s.state)}</b></span>`) : '');
    const add = (no, name, pic, sw, setting, why, clash) => {
      if (!sw && !setting) return;
      mods.push(`<div class="mod${sw && sw.on ? ' on' : ''}${clash ? ' clash' : ''}" data-mod="${no}"><div class="top"><span class="no">${no}</span><span class="nm">${esc(name)}</span></div>${
        sw ? `<button type="button" class="key" data-sw="${no}" aria-label="${esc(name)}" aria-pressed="${sw.on}"${sw.writable && sw.known ? '' : ' disabled'}>${sw.known ? (sw.on ? 'On' : 'Off') : '—'}</button>` : ''}${
        setting}${pic}<div class="why">${esc(why)}</div></div>`);
    };
    add(1, 'EQ', PIC.eq, m.eq.sw, eqSetting, m.eq.pre ? 'applies to the bands' : 'the bands of the device');
    add(2, 'Loudness', PIC.loud, m.loud.sw, loudSetting, 'follows the volume');
    add(3, 'Crossfeed', PIC.xf, m.xf.sw, sel('xf', m.xf.sel), both ? 'on with speakers: pick one' : 'exclusive with speakers', both);
    add(4, 'Speakers', PIC.sp, m.sp.sw, sel('sp', m.sp.sel), both ? 'on with crossfeed: pick one' : 'exclusive with crossfeed', both);
    const root = this.draw(`<div class="mods">${mods.join('')}</div>${m.path !== null || m.config !== null
      ? `<div class="foot"><span>${m.path !== null ? `Path <b>${esc(m.path)}</b>` : ''}</span><span>${m.config !== null ? `Config <b>${esc(m.config)}</b>` : ''}</span></div>` : ''}`, CSS);
    const src = { 1: m.eq.sw, 2: m.loud.sw, 3: m.xf.sw, 4: m.sp.sw };
    for (const b of root.querySelectorAll('[data-sw]')) {
      b.addEventListener('click', () => {
        const sw = src[b.dataset.sw];
        if (!sw.writable || !sw.known) return;
        this.call('switch', sw.on ? 'turn_off' : 'turn_on', { entity_id: sw.id });
      });
    }
    for (const s of root.querySelectorAll('[data-sel]')) {
      s.addEventListener('change', () => {
        const sel = s.dataset.sel === 'xf' ? m.xf.sel : m.sp.sel;
        this.call('select', 'select_option', { entity_id: sel.id, option: s.value });
      });
    }
    for (const r of root.querySelectorAll('input[type=range]')) {
      const n = r.dataset.range === 'pre' ? m.eq.pre : m.loud.n;
      let holding = false;
      const hold = () => { if (!holding) { holding = true; this.hold(); } };
      const drop = () => { if (holding) { holding = false; this.release(); } };
      r.addEventListener('pointerdown', hold);
      r.addEventListener('pointerup', drop);
      r.addEventListener('lostpointercapture', drop);
      r.addEventListener('input', () => {
        const label = r.parentElement.querySelector('b');
        label.textContent = r.dataset.range === 'pre' ? `${r.value} dB` : `${Math.round((Number(r.value) / (n.max || 127)) * 100)}%`;
      });
      // one write when the gesture ends (the range's change event)
      r.addEventListener('change', () => {
        this.stale();
        this.call('number', 'set_value', { entity_id: n.id, value: Number(r.value) });
        drop();
      });
      r.addEventListener('pointercancel', drop);
    }
  }

  range(key, n, label) {
    const ok = Number.isFinite(n.value);
    const v = ok ? n.value : n.min;
    return `<div class="val"><span>${key === 'pre' ? 'Preamp' : 'Strength'}</span> <b>${esc(label)}</b><input type="range" data-range="${key}" min="${n.min}" max="${n.max}" step="${n.step}" value="${v}"${ok ? '' : ' disabled'} aria-label="${key === 'pre' ? 'EQ preamp' : 'Loudness strength'}"></div>`;
  }

  getCardSize() { return 4; }
}

register('svolder-chain-card', SvolderChainCard, 'svolder chain', "The computer's chain: EQ, loudness, crossfeed, speakers.");
