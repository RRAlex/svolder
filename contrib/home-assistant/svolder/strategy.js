// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later
// The dashboard svolder builds: one view per computer, the computer's section first, then one
// section per device that computer serves. Pure: it reads `hass` and returns a config, and never
// calls a service.
import { findComputers, slug } from './find.js';
import { computerSection, deviceSections, entitiesSection } from './layout.js';

/** One computer's view config. */
export function buildView(computer, theme) {
  const sections = [];
  if (computer.device) {
    const c = computerSection(computer.title, computer.entities);
    sections.push(...c.sections);
    for (const extra of [
      entitiesSection('Other', 'mdi:dots-horizontal', c.rest),
      entitiesSection('Diagnostics', 'mdi:stethoscope', c.diag),
    ]) if (extra) sections.push(extra);
  }
  for (const dev of computer.devices) {
    const d = deviceSections(dev.title, dev.entities);
    sections.push(...d.sections);
    for (const extra of [
      entitiesSection(`${dev.title}: other`, 'mdi:dots-horizontal', d.rest),
      entitiesSection(`${dev.title}: diagnostics`, 'mdi:stethoscope', d.diag),
    ]) if (extra) sections.push(extra);
  }
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
