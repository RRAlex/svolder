// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later
// Home Assistant dashboard strategy `custom:svolder`: one page per svolder computer, drawn with
// svolder's own cards (imported below). Add it as a dashboard resource (type module); README.md
// has the steps.
import { buildDashboard } from './svolder/strategy.js';
import './svolder/cards/index.js';

class SvolderDashboardStrategy extends HTMLElement {
  static async generate(config, hass) {
    return buildDashboard(hass, config);
  }
}

if (!customElements.get('ll-strategy-dashboard-svolder')) {
  customElements.define('ll-strategy-dashboard-svolder', SvolderDashboardStrategy);
}
// The entry's fields (type, strategyType, name, description) are those of Home Assistant 2026.10's
// custom strategy documentation; a frontend that ignores them still runs the strategy by name.
window.customStrategies = window.customStrategies || [];
if (!window.customStrategies.some((s) => s.type === 'svolder')) {
  window.customStrategies.push({
    type: 'svolder',
    strategyType: 'dashboard',
    name: 'svolder',
    description: 'One page per svolder computer, with the DACs and amps it serves.',
  });
}
