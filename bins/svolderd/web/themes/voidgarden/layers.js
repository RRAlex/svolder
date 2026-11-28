/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's layer list: which layers draw, in which order, and the context they are built
// with. A layer is a factory `(ctx) => { name, group, object, build(seed), update(frame),
// dispose() }` (layers/*.js); this module makes the context (the vendored namespaces, the shared
// shading `lib` and `sprites`, the capability bag, the uniform block, the audio buffers, the scene
// and the camera), builds the layers in order under the scene, moves the camera and the uniforms
// from the rig each frame, and hides a layer the bisect switches turned off.
//
//   createLayers(gpu, { off, seed })   -> `{ ctx, list, build(seed), update(frame), setOff(names),
//                                          refit(), draw(), readStats(target), dispose() }`
//   ORDER                             the layer names, in the order they are built and drawn
//   BACKDROP                          the layers drawn at the budget's share of the resolution
//   frameStats(pixels, w, h)          the statistics of a small readback

import { createLib } from './tsl/lib.js';
import { createSprites } from './tsl/sprite.js';
import { beaconsLayer } from './layers/beacons.js';
import { floraLayer } from './layers/flora.js';
import { heroLayer } from './layers/hero.js';
import { landLayer } from './layers/land.js';
import { particlesLayer } from './layers/particles.js';
import { postLayer } from './layers/post.js';
import { riverLayer } from './layers/river.js';
import { skyLayer } from './layers/sky.js';
import { BACK_LAYER } from './tsl/post.js';

/** The night the scene is drawn on (the page's own ground, #030406). */
const GROUND = '#030406';

/** The factories, in drawing order: the post layer is last. */
const FACTORIES = [
  ['sky', skyLayer], ['land', landLayer], ['river', riverLayer], ['flora', floraLayer],
  ['beacons', beaconsLayer], ['hero', heroLayer], ['particles', particlesLayer],
  ['post', postLayer],
];

/** The layer names, in order. */
export const ORDER = FACTORIES.map(([name]) => name);

/** The backdrop: the layers drawn at the budget's share of the resolution (the post chain's back
 * pass); the hero, the beacons and the particles are drawn at the canvas's own. */
export const BACKDROP = Object.freeze(['sky', 'land', 'river', 'flora']);

/** Move `camera` to the rig's `cam`. */
function aimCamera(camera, cam) {
  camera.position.set(cam.x, cam.y, cam.z);
  camera.up.set(0, 1, 0);
  camera.lookAt(cam.lookX, cam.lookY, cam.lookZ);
  camera.rotateZ(cam.roll);
  if (Math.abs(camera.fov - cam.fov) > 0.01 || camera.aspect !== cam.aspect) {
    camera.fov = cam.fov;
    camera.aspect = cam.aspect;
    camera.updateProjectionMatrix();
  }
  camera.updateMatrixWorld();
}

/**
 * The statistics of a small readback (`pixels`: RGBA bytes, `w` by `h`): the mean of each channel,
 * the mean luminance, how many pixels are lit (luminance over 16), and a checksum of the
 * lit pixels' places.
 */
export function frameStats(pixels, w, h) {
  const sum = [0, 0, 0];
  let lit = 0;
  let luma = 0;
  let places = 0;
  for (let i = 0; i < w * h; i++) {
    const r = pixels[i * 4];
    const g = pixels[i * 4 + 1];
    const b = pixels[i * 4 + 2];
    sum[0] += r;
    sum[1] += g;
    sum[2] += b;
    const l = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    luma += l;
    if (l > 16) {
      lit++;
      places = (Math.imul(places, 31) + i) >>> 0;
    }
  }
  const n = w * h;
  return {
    w,
    h,
    mean: sum.map((v) => Number((v / n).toFixed(3))),
    luma: Number((luma / n).toFixed(3)),
    lit,
    places,
  };
}

/**
 * The layers on `gpu` (gpu.js), built for `seed`. `off` is the set of layer names the bisect
 * switches turned off (a hidden layer is not updated).
 */
export function createLayers(gpu, { off = new Set(), seed = 0 } = {}) {
  const { T, S } = gpu;
  const lib = createLib(T, S, gpu.tier);
  const sprites = createSprites(T, S, lib);
  const scene = new T.Scene();
  scene.background = new T.Color(GROUND);
  const camera = new T.PerspectiveCamera(55, 1.6, 0.05, 400);
  // the camera sees both the backdrop's layer and the rest's (a plain render draws everything)
  camera.layers.enable(BACK_LAYER);
  const ctx = {
    T, S, gpu, lib, sprites, caps: gpu.caps, scene, camera, u: lib.uniformBlock(),
    audio: lib.audio(), terrain: null,
    render: null,
  };
  const factories = FACTORIES.map(([, factory]) => factory);
  const list = factories.map((factory) => factory(ctx));
  for (const layer of list) {
    if (!layer.object) continue;
    scene.add(layer.object);
    if (BACKDROP.includes(layer.name)) layer.object.traverse((o) => o.layers.set(BACK_LAYER));
  }
  let hidden = new Set(off);
  let seeded = null;

  const api = {
    ctx,
    list,
    get seed() { return seeded; },

    /** Place everything the seed places. */
    build(next) {
      seeded = next;
      // a new world starts with no hero placed: the hero publishes it again in its first update, so
      // no layer reads the place of another world's last frame
      ctx.hero = null;
      for (const layer of list) layer.build(next);
    },

    /** The bisect switches: the names of the layers to leave out. */
    setOff(names) {
      hidden = new Set(names);
      for (const layer of list) if (layer.object) layer.object.visible = !hidden.has(layer.name);
    },

    /** One frame: the camera and the uniforms from the rig (`frame.cam`, `frame.u`), then each
     * layer. */
    update(frame) {
      aimCamera(camera, frame.cam);
      lib.setUniforms(ctx.u, frame.u, frame.palette ?? null);
      for (const layer of list) {
        if (!hidden.has(layer.name)) layer.update({ ...frame, camera, caps: gpu.caps });
      }
    },

    /** The canvas has a new size: the layers that read it (the post chain's size uniforms and its
     * history) take it before the next draw. */
    refit() {
      for (const layer of list) if (!hidden.has(layer.name)) layer.refit?.();
    },

    /** Draw the scene to the canvas (the post layer's chain, when it has one). */
    draw() {
      if (ctx.render) ctx.render();
      else gpu.render(scene, camera);
    },

    /** Render the scene as it stands into a small target and read its statistics. */
    async readStats(target) {
      const was = camera.aspect;
      camera.aspect = target.width / target.height;
      camera.updateProjectionMatrix();
      const pixels = await gpu.read(scene, camera, target);
      camera.aspect = was;
      camera.updateProjectionMatrix();
      return frameStats(pixels, target.width, target.height);
    },

    dispose() {
      for (const layer of list) layer.dispose();
    },
  };
  api.build(seed);
  // the switches the page was opened with hide their layers from the first frame
  api.setOff(off);
  return api;
}
