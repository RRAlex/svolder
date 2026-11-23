// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later
// `svolder-scenes-card`: one hairline row per scene (lamp, name, the parts every scene shares as
// chips in one aligned column; a part only some scenes have goes on a line under its row), a "Save
// current as..." field with Save, and the answer to what this card sent. Rows and chips come from
// the scene select's `scenes` attribute; `custom` is the state of no scene and is never a row.
import { SvolderCard, esc, selectOf, firstId, register } from './shared.js';

const CSS = `
.rows { display: grid; grid-template-columns: 14px max-content minmax(0, 1fr); column-gap: 10px; }
.row { display: grid; grid-template-columns: subgrid; grid-column: 1 / -1; align-items: center; row-gap: 3px;
  padding: 6px 8px; border: 0; border-top: 1px solid var(--sv-line); background: transparent; width: 100%; min-height: 38px;
  text-align: left; color: var(--sv-text); }
.row[disabled] { color: inherit; }
.row.on { box-shadow: inset 3px 0 0 var(--sv-on); }
.row .name { font-weight: 500; color: var(--sv-dim); }
.row.on .name { color: var(--sv-text); }
.row:not([disabled]):hover .name { color: var(--sv-text); }
.parts { display: flex; flex-wrap: wrap; gap: 4px; min-width: 0; }
.parts.extra { grid-column: 3; }
.part { font-size: 10px; letter-spacing: .06em; text-transform: uppercase; border: 1px solid var(--sv-line); border-radius: 3px;
  padding: 0 5px; color: var(--sv-dim); }
.part.off { border-style: dashed; color: var(--sv-dim); }
.saver { display: flex; flex-wrap: wrap; gap: 8px; margin: 10px 0 0; }
.saver input { flex: 1 1 10em; min-height: 34px; padding: 0 10px; border: 1px solid var(--sv-faint); border-radius: 4px;
  background: var(--sv-field); color: var(--sv-text); min-width: 0; }
.saver .key { border-color: var(--sv-on); }
.none { color: var(--sv-dim); }
`;

const sameName = (a, b) => String(a).trim().toLowerCase() === String(b).trim().toLowerCase();

export class SvolderScenesCard extends SvolderCard {
  check(config) {
    if (!config.entities || !config.entities.scene) throw new Error('svolder-scenes-card: entities.scene is needed');
    return config;
  }

  static getStubConfig(hass) {
    const scene = firstId(hass, /^(select|sensor)\..*_scene$/);
    const save = firstId(hass, /^text\..*_scene_save$/);
    return { type: 'custom:svolder-scenes-card', entities: { ...(scene ? { scene } : {}), ...(save ? { save } : {}) } };
  }

  read() {
    const h = this._hass;
    const E = this._cfg.entities;
    const st = h.states[E.scene];
    const sel = selectOf(h, E.scene);
    const a = (st && st.attributes) || {};
    const known = Array.isArray(a.scenes);
    let scenes = known ? a.scenes.map((s) => ({ name: s.name, parts: Array.isArray(s.parts) ? s.parts : [] }))
      : (sel ? sel.options : []).map((name) => ({ name, parts: [] }));
    scenes = scenes.filter((s) => !sameName(s.name, 'custom'));
    const every = known && scenes.length ? scenes[0].parts.map((p) => p.who).filter((w) => scenes.every((s) => s.parts.some((p) => p.who === w))) : [];
    const rows = scenes.map((s) => ({
      name: s.name, on: !!st && st.state === s.name,
      common: every.map((w) => s.parts.find((p) => p.who === w)),
      extra: s.parts.filter((p) => !every.includes(p.who)),
    }));
    const save = E.save && h.states[E.save];
    return {
      rows, reading: known && a.known === false, writable: !!(sel && sel.writable),
      canSave: !!(save && E.save.startsWith('text.')), saveId: E.save || null,
      answer: this.answer(st, save),
    };
  }

  /** What this card sent, answered once the entity's own record names it and differs from the record before the send. */
  answer(st, save) {
    const sent = this._sent;
    if (!sent) return null;
    const a = (st && st.attributes) || {};
    if (sent.kind === 'recall') {
      const o = a.outcome;
      const fresh = o && sameName(o.name, sent.name) && JSON.stringify(o) !== sent.before;
      if (!fresh) return null;
      if (o.error) return { kind: 'recall', name: o.name, error: String(o.error), issues: [] };
      const bad = (o.parts || []).filter((p) => p.status && p.status !== 'done');
      const words = (p) => (p.notes ? [].concat(p.notes).join('; ') : '');
      return { kind: 'recall', name: o.name, error: '', issues: bad.map((p) => `${p.who} ${p.status === 'left' ? 'skipped' : p.status}${words(p) ? `: ${words(p)}` : ''}`) };
    }
    const s = a.saved;
    const fresh = s && sameName(s.name, sent.name) && JSON.stringify(s) !== sent.before;
    if (!fresh) return null;
    return { kind: 'save', name: s.name, error: s.error || '' };
  }

  paint(m) {
    this._rows = m.rows.length;
    const chip = (p) => `<span class="part${p.present ? '' : ' off'}">${esc(p.present ? p.who : `${p.who} · not connected`)}</span>`;
    const rows = m.rows.map((r, i) => `<${m.writable ? 'button type="button"' : 'div'} class="row${r.on ? ' on' : ''}" data-i="${i}"${r.on ? ' aria-current="true"' : ''}><i class="lamp${r.on ? ' on' : ''}"></i><span class="name">${esc(r.name)}</span><span class="parts">${
      r.common.filter(Boolean).map(chip).join('')}</span>${r.extra.length ? `<span class="parts extra">${r.extra.map(chip).join('')}</span>` : ''}</${m.writable ? 'button' : 'div'}>`);
    const ans = m.answer;
    const say = ans ? (ans.kind === 'recall' && ans.error
      ? `<span class="bad">${esc(ans.name)} not recalled: ${esc(ans.error)}</span>`
      : ans.kind === 'recall'
      ? `<b>${esc(ans.name)} recalled.</b> ${ans.issues.length ? `<span class="skip">${esc(ans.issues.join('. '))}.</span>` : ''}`
      : (ans.error ? `<span class="bad">${esc(ans.name)} not saved: ${esc(ans.error)}</span>` : `<b>${esc(ans.name)} saved.</b>`)) : '';
    const root = this.draw(`
      <div class="head"><span class="nm">Scenes</span><span class="sub">computer + devices</span></div>
      ${m.reading ? '<div class="none">reading…</div>' : ''}
      ${rows.length ? `<div class="rows">${rows.join('')}</div>` : (m.reading ? '' : '<div class="none">No scenes saved yet.</div>')}
      ${m.canSave ? `<div class="saver"><input type="text" placeholder="Save current as…" aria-label="Scene name" maxlength="40" value="${esc(this._draft || '')}"><button type="button" class="key" data-save>Save</button></div>` : ''}
      <p class="say" aria-live="polite">${say}</p>`, CSS);
    const E = this._cfg.entities;
    for (const b of root.querySelectorAll('button.row')) {
      b.addEventListener('click', () => {
        const name = m.rows[Number(b.dataset.i)].name;
        this.remember('recall', name);
        this.call('select', 'select_option', { entity_id: E.scene, option: name });
      });
    }
    const input = root.querySelector('.saver input');
    if (input) {
      input.addEventListener('input', () => { this._draft = input.value; });
      const save = () => {
        const name = input.value.trim();
        if (!name) return;
        this.remember('save', name);
        this.call('text', 'set_value', { entity_id: m.saveId, value: name });
        this._draft = '';
        input.value = '';
      };
      root.querySelector('[data-save]').addEventListener('click', save);
      input.addEventListener('keydown', (e) => { if (e.key === 'Enter') save(); });
    }
  }

  /** Notes what was sent and the entity's record before it, so an old answer is not taken for the new one. */
  remember(kind, name) {
    const st = this._hass.states[this._cfg.entities.scene];
    const a = (st && st.attributes) || {};
    this._sent = { kind, name, before: JSON.stringify(kind === 'recall' ? a.outcome : a.saved) };
    this.stale();
  }

  getCardSize() { return 3 + (this._rows ?? 3); }
}

register('svolder-scenes-card', SvolderScenesCard, 'svolder scenes', 'The computer\'s scenes: recall one, save the current sound as a new one.');
