/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The smoke's music notes: three glyphs (a quaver, a beamed pair of quavers, a crotchet) drawn
// with canvas paths into one strip of three square cells. The red channel holds the glyph, the
// green channel a soft halo around it; smoke.js uploads the strip once and stamps from it.

/** A head's radii (of the cell), its tilt (radians), a stem's width and its place right of the
 * head's centre (of the cell). */
const HEAD_RX = 0.135;
const HEAD_RY = 0.095;
const HEAD_TILT = -0.4;
const STEM_W = 0.04;
const STEM_X = 0.115;
/** Where each glyph's pieces are, in fractions of a cell (x from the left, y from the top): the
 * heads' centres `[x, y]`, each stem standing on its head's right side and rising to its `tops`
 * entry, and whether the glyph has a flag or a beam. */
const GLYPHS = [
  { heads: [[0.36, 0.72]], tops: [0.2], flag: true },
  { heads: [[0.29, 0.72], [0.6, 0.64]], tops: [0.2, 0.15], beam: true },
  { heads: [[0.42, 0.72]], tops: [0.18] },
];

/** The path of glyph `g` in the cell whose left edge is `ox` (canvas pixels) and side `cell`:
 * heads as tilted ellipses, stems as thin rectangles, a flag as a closed curve, a beam as a
 * polygon. One path, so it is filled and stroked as it stands. */
function glyphPath(ctx, g, ox, cell) {
  const px = (x) => ox + x * cell;
  const py = (y) => y * cell;
  ctx.beginPath();
  const stems = g.heads.map(([hx, hy], i) => {
    ctx.moveTo(px(hx) + HEAD_RX * cell * Math.cos(HEAD_TILT),
      py(hy) + HEAD_RX * cell * Math.sin(HEAD_TILT));
    ctx.ellipse(px(hx), py(hy), HEAD_RX * cell, HEAD_RY * cell, HEAD_TILT, 0, 2 * Math.PI);
    const sx = hx + STEM_X;
    ctx.rect(px(sx), py(g.tops[i]), STEM_W * cell, (hy - g.tops[i]) * cell);
    return { x: sx, top: g.tops[i] };
  });
  if (g.flag) {
    const { x, top } = stems[0];
    const sx = px(x + STEM_W);
    const ty = py(top);
    ctx.moveTo(sx, ty);
    ctx.bezierCurveTo(sx + 0.06 * cell, ty + 0.1 * cell, sx + 0.26 * cell, ty + 0.2 * cell,
      sx + 0.18 * cell, ty + 0.42 * cell);
    ctx.bezierCurveTo(sx + 0.22 * cell, ty + 0.24 * cell, sx + 0.06 * cell, ty + 0.2 * cell,
      sx, ty + 0.2 * cell);
    ctx.closePath();
  }
  if (g.beam) {
    const [a, b] = stems;
    const beam = 0.09 * cell;
    ctx.moveTo(px(a.x), py(a.top));
    ctx.lineTo(px(b.x + STEM_W), py(b.top));
    ctx.lineTo(px(b.x + STEM_W), py(b.top) + beam);
    ctx.lineTo(px(a.x), py(a.top) + beam);
    ctx.closePath();
  }
}

/**
 * Draw the three glyphs side by side into a `3 * cell` by `cell` 2D context: quaver, beamed pair,
 * crotchet. The strip is opaque black; each glyph is red (its mask) over a green halo (its shape
 * dilated by a stroke of `halo.width` and blurred by a shadow of `halo.blur`, both shares of the
 * cell); nothing leaves its cell.
 */
export function drawAtlas(ctx, cell, halo) {
  ctx.globalCompositeOperation = 'source-over';
  ctx.fillStyle = '#000';
  ctx.fillRect(0, 0, 3 * cell, cell);
  GLYPHS.forEach((g, i) => {
    const ox = i * cell;
    ctx.save();
    ctx.beginPath();
    ctx.rect(ox, 0, cell, cell);
    ctx.clip();
    glyphPath(ctx, g, ox, cell);
    // the halo: the black shape adds nothing, its green shadow adds the glow around it
    ctx.globalCompositeOperation = 'lighter';
    ctx.shadowColor = 'rgb(0, 255, 0)';
    ctx.shadowBlur = halo.blur * cell;
    ctx.fillStyle = '#000';
    ctx.strokeStyle = '#000';
    ctx.lineWidth = halo.width * cell;
    ctx.lineJoin = 'round';
    ctx.fill();
    ctx.stroke();
    ctx.shadowBlur = 0;
    ctx.shadowColor = 'rgba(0, 0, 0, 0)';
    // the glyph itself: red only
    ctx.fillStyle = 'rgb(255, 0, 0)';
    ctx.fill();
    ctx.restore();
  });
}
