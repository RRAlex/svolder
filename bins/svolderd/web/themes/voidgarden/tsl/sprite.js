/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The instanced sprite: what a GL point with a point size gives. WebGPU has no point size above
// one pixel, so each instance is a quad on a `Sprite` object with `count = n`, its size read from
// a buffer at the instance's index. Two techniques behind one helper, whichever a size is in:
// pixels (`PointsNodeMaterial.sizeNode`, `sizeAttenuation` off: a GL point's size) or world
// units (`SpriteNodeMaterial.scaleNode`). Never a `Points` object: a point list is one pixel on
// every cell and its size node is ignored.
//
//   createSprites(T, S)   -> { make(options) }
//   make({ count, position, size, mode, colour, opacity, round, additive })
//        `position` a vec4 storage buffer (xyz), `size` a float storage buffer, `mode` 'px' or
//        'world', `colour` a node (default white), `opacity` a node or null, `round` the soft disc
//        of `spr` over the square; returns the `Sprite` object (its `material` is the node
//        material)

export function createSprites(T, S, lib) {
  function make({
    count, position, size, mode = 'px', colour = S.vec3(1, 1, 1), opacity = null, round = false,
    additive = true,
  }) {
    const i = S.instanceIndex;
    const material = mode === 'px' ? new T.PointsNodeMaterial() : new T.SpriteNodeMaterial();
    material.positionNode = position.element(i).xyz;
    if (mode === 'px') {
      material.sizeNode = size.element(i);
      material.sizeAttenuation = false;
    } else {
      material.scaleNode = S.vec2(size.element(i));
    }
    const shape = round ? lib.spr() : S.float(1);
    material.colorNode = S.vec4(colour, 1);
    material.opacityNode = opacity ? opacity.mul(shape) : shape;
    material.transparent = true;
    material.depthWrite = false;
    material.blending = additive ? T.AdditiveBlending : T.NormalBlending;
    const sprite = new T.Sprite(material);
    sprite.count = count;
    sprite.frustumCulled = false;
    return sprite;
  }
  return { make };
}
