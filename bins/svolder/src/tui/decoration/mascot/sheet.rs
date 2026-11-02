// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sprite sheet: layers of one letter per pixel, parsed once from the asset.
//!
//! A block starts with `== name dx dy`, the layer's top-left in pixels from the squirrel's own;
//! its rows follow until a blank line. `.` is see-through and `#` lines are comments.

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// One layer of the sheet.
#[derive(Debug)]
pub(crate) struct Layer {
    /// Pixels right of the squirrel's top-left the layer starts.
    pub dx: i32,
    /// Pixels below the squirrel's top-left the layer starts.
    pub dy: i32,
    /// The rows, one letter a pixel.
    pub rows: Vec<&'static str>,
}

/// The sheet's source text.
const SHEET: &str = include_str!("../../../../assets/mascot/sprites.txt");

/// Every layer, by name.
fn sheet() -> &'static BTreeMap<&'static str, Layer> {
    static PARSED: OnceLock<BTreeMap<&'static str, Layer>> = OnceLock::new();
    PARSED.get_or_init(|| parse(SHEET))
}

/// `text` read as a sheet.
fn parse(text: &'static str) -> BTreeMap<&'static str, Layer> {
    let mut layers = BTreeMap::new();
    let mut current: Option<&'static str> = None;
    for line in text.lines() {
        if line.starts_with('#') {
            continue;
        }
        if let Some(head) = line.strip_prefix("== ") {
            let mut parts = head.split_whitespace();
            let name = parts.next().expect("a layer needs a name");
            let mut offset = || {
                parts
                    .next()
                    .and_then(|n| n.parse::<i32>().ok())
                    .unwrap_or_else(|| panic!("layer {name} needs two offsets"))
            };
            let (dx, dy) = (offset(), offset());
            layers.insert(
                name,
                Layer {
                    dx,
                    dy,
                    rows: Vec::new(),
                },
            );
            current = Some(name);
        } else if line.trim().is_empty() {
            current = None;
        } else if let Some(name) = current {
            layers
                .get_mut(name)
                .expect("the open layer exists")
                .rows
                .push(line.trim());
        }
    }
    layers
}

/// The layer called `name`. Panics on a name the sheet does not hold: the names the code uses
/// are all proved to exist by a test.
pub(crate) fn layer(name: &str) -> &'static Layer {
    sheet()
        .get(name)
        .unwrap_or_else(|| panic!("the sprite sheet has no layer {name}"))
}

/// Every layer's name, in order.
#[cfg(test)]
pub(crate) fn names() -> impl Iterator<Item = &'static str> {
    sheet().keys().copied()
}
