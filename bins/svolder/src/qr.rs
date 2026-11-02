// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A QR code as text: half blocks, each character two modules tall, inside the four-module quiet
//! zone the standard asks for. The character draws the dark modules and its background the light
//! ones, so a caller paints it black on white whatever the terminal's theme: `svolder web --qr`
//! with explicit colour escapes, the TUI's Web box with a style.

use qrcode::{Color, EcLevel, QrCode};

/// The quiet zone round the code, in modules.
const QUIET: usize = 4;

/// `text` as a QR code at error correction level M, one string per two rows of modules, quiet
/// zone included: `█` both dark, `▀` the top one, `▄` the bottom one, a space neither.
pub fn half_blocks(text: &str) -> Result<Vec<String>, String> {
    let code = QrCode::with_error_correction_level(text.as_bytes(), EcLevel::M)
        .map_err(|e| format!("the link does not fit a QR code: {e}"))?;
    let width = code.width();
    let colors = code.to_colors();
    let size = width + 2 * QUIET;
    let dark = |x: usize, y: usize| {
        (QUIET..QUIET + width).contains(&x)
            && (QUIET..QUIET + width).contains(&y)
            && colors[(y - QUIET) * width + (x - QUIET)] == Color::Dark
    };
    Ok((0..size.div_ceil(2))
        .map(|row| {
            (0..size)
                .map(|x| match (dark(x, 2 * row), dark(x, 2 * row + 1)) {
                    (true, true) => '█',
                    (true, false) => '▀',
                    (false, true) => '▄',
                    (false, false) => ' ',
                })
                .collect()
        })
        .collect())
}

/// `lines` painted black on white by explicit 24-bit colours, each reset at its end.
pub fn painted(lines: &[String]) -> Vec<String> {
    lines
        .iter()
        .map(|line| format!("\x1b[38;2;0;0;0;48;2;255;255;255m{line}\x1b[0m"))
        .collect()
}

#[cfg(test)]
mod tests;
