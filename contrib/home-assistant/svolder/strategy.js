// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later
// The dashboard svolder builds: one view per computer, each device it serves first (faceplate and
// EQ card), then the scenes, the computer's chain and the "other" lists, then the diagnostics. Pure: it reads `hass` and returns a config, and never
// calls a service.
import { findComputers, slug } from './find.js';
import { computerSection, deviceSections, entitiesSection } from './layout.js';

/** One section from the cards of several: a column the page lays out as one stack. */
const stack = (sections) => (sections.length
  ? [{ type: 'grid', cards: sections.flatMap((x) => x.cards) }] : []);

/**
 * One computer's view config, in the approved layout's order: each device (name order) as one
 * stack of its faceplate and EQ card, then the scenes, the computer's chain and the "other" entities
 * as one stack (the second column beside a single device), then the diagnostics.
 */
export function buildView(computer, theme) {
  const sections = [];
  const others = [];
  const diags = [];
  const push = (list, section) => { if (section) list.push(section); };
  for (const dev of computer.devices) {
    const d = deviceSections(dev.title, dev.entities);
    sections.push(...stack(d.sections));
    push(others, entitiesSection(`${dev.title}: other`, 'mdi:dots-horizontal', d.rest));
    push(diags, entitiesSection(`${dev.title}: diagnostics`, 'mdi:stethoscope', d.diag));
  }
  let right = [];
  if (computer.device) {
    const c = computerSection(computer.title, computer.entities);
    right = c.sections;
    others.unshift(...[entitiesSection('Other', 'mdi:dots-horizontal', c.rest)].filter(Boolean));
    diags.unshift(...[entitiesSection('Diagnostics', 'mdi:stethoscope', c.diag)].filter(Boolean));
  }
  // the scenes, the chain and every "other" list share one column, so the short chain leaves no gap
  sections.push(...stack([...right, ...others]), ...diags);
  const view = {
    title: computer.title,
    path: slug(computer.title) || 'computer',
    icon: computer.device ? 'mdi:laptop' : 'mdi:knob',
    type: 'sections',
    max_columns: 2,
    dense_section_placement: true,
    sections,
  };
  if (theme) view.theme = 'svolder';
  return view;
}

/** The whole dashboard for `hass`: `{ title, views }`. No svolder anywhere: one empty view. */
export function buildDashboard(hass, config = {}) {
  const theme = Boolean(hass.themes && hass.themes.themes && hass.themes.themes.svolder);
  const views = findComputers(hass).map((c) => buildView(c, theme));
  const paths = new Set();
  for (const v of views) {
    while (paths.has(v.path)) v.path += '_';
    paths.add(v.path);
  }
  if (!views.length) {
    views.push({
      title: 'svolder', path: 'svolder', type: 'sections',
      sections: [{ type: 'grid', cards: [{
        type: 'markdown', content: 'No svolder computer or device found. Turn on `[mqtt]` in svolderd.',
      }] }],
    });
  }
  return { title: config.title || 'svolder', views };
}
