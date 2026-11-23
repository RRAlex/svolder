// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later
// Finds svolder's computers and devices in what the Home Assistant frontend already holds:
// `hass.devices` (id, identifiers, name, name_by_user, via_device_id, disabled_by) and
// `hass.entities` (entity id, name, device_id, entity_category, hidden; a hidden entity is left
// out, as Home Assistant's own generated dashboards do). Nothing here calls a service or a
// websocket.

const COMPUTER = /^svolder_computer(?:_([a-z0-9_]+))?$/;
// The unit id is the sixteen UPPER-case hex digits svolderd publishes; the computer id is a lower-case host id.
const DEVICE = /^svolder_[0-9A-Fa-f]{16}$/;

/** The MQTT identifier strings of one registry device. */
function mqttIds(device) {
  const ids = [];
  for (const pair of device.identifiers || []) {
    if (Array.isArray(pair) && pair[0] === 'mqtt' && typeof pair[1] === 'string') ids.push(pair[1]);
  }
  return ids;
}

const nameOf = (device) => device.name_by_user || device.name || '';

/** `text` as an entity id's object part: lower case, `[a-z0-9_]`. */
export function slug(text) {
  return String(text).toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_+|_+$/g, '');
}

/** The entities of every registry device, `{ [device_id]: [entry, ...] }`, in entity id order. */
function entitiesByDevice(hass) {
  const by = {};
  for (const id of Object.keys(hass.entities || {}).sort()) {
    const e = hass.entities[id];
    if (!e || !e.device_id || e.hidden) continue;
    const entry = { ...e, entity_id: e.entity_id || id };
    (by[e.device_id] = by[e.device_id] || []).push(entry);
  }
  return by;
}

/**
 * Every computer with the devices it serves:
 * `[{ id, title, device, entities, devices: [{ id, title, device, entities }] }]`, views in name
 * order. A device with no computer goes to the only computer, else to a last group titled
 * "Devices" (`id: null`, no entities of its own).
 */
export function findComputers(hass) {
  const by = entitiesByDevice(hass);
  const computers = [];
  const devices = [];
  for (const [rid, device] of Object.entries(hass.devices || {})) {
    if (device.disabled_by) continue;
    const ids = mqttIds(device);
    const registryId = device.id || rid;
    const cid = ids.find((i) => COMPUTER.test(i));
    if (cid) {
      computers.push({
        id: cid, registryId, device, title: nameOf(device) || 'This computer',
        entities: by[registryId] || [], devices: [],
      });
      continue;
    }
    const did = ids.find((i) => DEVICE.test(i));
    if (did) {
      devices.push({
        id: did, registryId, device, title: nameOf(device) || did, entities: by[registryId] || [],
      });
    }
  }
  const byName = (a, b) => a.title.localeCompare(b.title) || a.id.localeCompare(b.id);
  computers.sort(byName);
  devices.sort(byName);
  const orphans = [];
  for (const dev of devices) {
    const home = computers.find((c) => c.registryId === dev.device.via_device_id);
    if (home) home.devices.push(dev);
    else if (computers.length === 1) computers[0].devices.push(dev);
    else orphans.push(dev);
  }
  if (orphans.length) {
    computers.push({ id: null, registryId: null, device: null, title: 'Devices', entities: [], devices: orphans });
  }
  return computers;
}

/** The domain of an entity id. */
export const domainOf = (entityId) => entityId.slice(0, entityId.indexOf('.'));
const objectOf = (entityId) => entityId.slice(entityId.indexOf('.') + 1);

/**
 * Takes the entity a role names out of `pool` (an array, mutated): `domains` its domain, `slugs`
 * the role's name as the discovery spells it, `owner` the device's name. The registry name is
 * tried first (it survives a renamed entity id), then the entity id exactly `<owner>_<role>`
 * (or `<role>` alone). Returns the entry or null.
 */
export function take(pool, domains, slugs, owner = '') {
  const list = Array.isArray(slugs) ? slugs : [slugs];
  const dom = Array.isArray(domains) ? domains : [domains];
  const inDomain = pool.filter((e) => dom.includes(domainOf(e.entity_id)));
  const own = slug(owner);
  for (const s of list) {
    let hit = inDomain.find((e) => e.name && slug(e.name) === s);
    if (!hit) hit = inDomain.find((e) => [s, `${own}_${s}`].includes(objectOf(e.entity_id)));
    if (hit) {
      pool.splice(pool.indexOf(hit), 1);
      return hit;
    }
  }
  return null;
}
