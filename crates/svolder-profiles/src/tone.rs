// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tone control's maths: named curves, the response a set of bands makes, the fit of a
//! headphone correction onto them, and the text format such a correction arrives in.
//!
//! All of it works in physical units (Hz, dB and Q) and converts through a device's own
//! [`EqSpec`](crate::EqSpec), so a front end draws, recalls or fits with the same numbers.
//!
//! | Module | What it holds |
//! |---|---|
//! | [`preset`] | Named curves, a device's current curve, the writes that recall one |
//! | [`response`] | The frequency response of a set of bands, and of a correction |
//! | [`fit`] | A correction fitted to a device's bands on its own raw steps |
//! | [`apo`] | Equalizer APO parametric text, as AutoEq and squig.link write it |

pub mod apo;
pub mod fit;
pub mod preset;
pub mod response;
