// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! SOFA files: the arrays read once through `sofar`'s `hdf` module, and the choice of the two
//! speakers' measurements out of them.
//!
//! Only the shape every impulse-response set in the catalogue has is read: `Data.IR` of
//! measurements by receivers (the two ears) by samples, one source position per measurement, so
//! a free-field set (one head, sources all around) and a room set (one speaker position a
//! measurement) go through the same choice.

use sofar::hdf::{self, DataObject};

use crate::error::ConvertError;
use crate::ir::{Angles, Direction, Irs};

/// How near a measurement's elevation and azimuth may be to a speaker's direction, in degrees,
/// unless a recipe says otherwise.
pub(crate) const DEFAULT_TOLERANCE: f64 = 10.0;

/// The most measurements a file may hold.
pub(crate) const MAX_MEASUREMENTS: usize = 1 << 20;

/// The most samples a response may hold.
pub(crate) const MAX_SAMPLES: usize = 1 << 20;

/// The longest delay applied, in samples: the length cap at the highest rate a chain runs at.
const MAX_SHIFT: f64 = 384_000.0 * crate::ir::MAX_SECONDS;

/// The most numbers any one dataset may hold (a 0.5 s response at 192 kHz is under 100000 samples
/// and no set has a million measurements: this bounds what is copied, not what is useful).
const MAX_ELEMENTS: usize = 1 << 24;

/// The distance a measurement is preferred at when several are in the same direction, in metres.
const PREFERRED_DISTANCE: f64 = 1.5;

/// The arrays of a SOFA file that the choice needs.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sofa {
    /// The sample rate in Hz.
    pub rate: f64,
    /// Measurements, receivers (ears), samples.
    pub shape: (usize, usize, usize),
    /// `Data.IR`, measurement-major.
    pub ir: Vec<f32>,
    /// Where each measurement's source was.
    pub positions: Vec<Direction>,
    /// `Data.Delay` in samples: one per receiver, or one per measurement and receiver.
    pub delay: Vec<f64>,
    /// Whether receiver 0 is the right ear.
    pub swap_ears: bool,
}

/// The checks every size and rate in `sofa` passes before it sizes anything.
fn validate(sofa: &Sofa) -> Result<(), ConvertError> {
    if !sofa.rate.is_finite() || !(8000.0..=384_000.0).contains(&sofa.rate) {
        return Err(ConvertError::Rate(sofa.rate as u32));
    }
    let (m, r, n) = sofa.shape;
    if m == 0 || n == 0 || m > MAX_MEASUREMENTS || n > MAX_SAMPLES || r != 2 {
        return Err(ConvertError::Layout(format!(
            "{m} measurements of {r} receivers and {n} samples"
        )));
    }
    let total = m.checked_mul(r).and_then(|v| v.checked_mul(n));
    if total != Some(sofa.ir.len()) || sofa.positions.len() != m {
        return Err(ConvertError::Layout(
            "the arrays do not fit the dimensions".into(),
        ));
    }
    if sofa.delay.iter().any(|d| !d.is_finite()) {
        return Err(ConvertError::Layout("a delay is not a number".into()));
    }
    Ok(())
}

/// Read the arrays of the SOFA file in `bytes`.
///
/// What this cannot bound: `sofar` reads every dataset whole before the code here sees it
/// (allocating up to 256 MiB a dataset) and inflates a compressed one with no total cap, so a
/// crafted file of under 64 MiB can cost a long time inside `sofar` that cannot be interrupted.
/// Callers hold the input to [`crate::MAX_SOURCE_BYTES`] first, and the catalogue's sets are
/// checked against their checksum before they get here.
pub(crate) fn parse(bytes: &[u8]) -> Result<Sofa, ConvertError> {
    if bytes.len() > crate::MAX_SOURCE_BYTES {
        return Err(ConvertError::TooLarge);
    }
    // `sofar` asserts on layouts it does not read (a panic in a debug build): a panic is a refusal.
    let parsed = std::panic::catch_unwind(|| hdf::parse_with_children(bytes))
        .map_err(|_| {
            ConvertError::NotSofa("the HDF5 container uses a layout that is not read".into())
        })?
        .map_err(|error| {
            ConvertError::NotSofa(format!("the HDF5 container is unreadable ({error})"))
        })?;
    let attribute = |name: &str| {
        parsed
            .root
            .parsed_attributes
            .iter()
            .find(|attr| attr.name == name)
            .and_then(|attr| attr.value.as_deref())
            .map(|value| value.trim_matches(['\0', ' ']).to_string())
    };
    if attribute("Conventions").as_deref() != Some("SOFA") {
        return Err(ConvertError::NotSofa(
            "it carries no SOFA conventions mark".into(),
        ));
    }
    match attribute("DataType").as_deref() {
        Some("FIR") => {}
        other => {
            return Err(ConvertError::NotFir(
                other.unwrap_or("no data type").to_string(),
            ));
        }
    }
    let child = |name: &str| -> Result<Option<DataObject>, ConvertError> {
        let read =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| parsed.get_child(name)))
                .map_err(|_| {
                    ConvertError::NotSofa(format!("{name} uses a layout that is not read"))
                })?;
        match read {
            None => Ok(None),
            Some(Ok(object)) => Ok(Some(object)),
            Some(Err(error)) => Err(ConvertError::NotSofa(format!(
                "{name} is unreadable ({error})"
            ))),
        }
    };
    let missing = |name: &str| ConvertError::NotSofa(format!("it has no {name}"));

    let ir_object = child("Data.IR")?.ok_or_else(|| missing("Data.IR"))?;
    let dims: Vec<usize> = ir_object
        .ds
        .dimension_size
        .iter()
        .map(|d| *d as usize)
        .collect();
    let (m, r, n) = match dims[..] {
        [m, r, n] => (m, r, n),
        [m, r, 1, n] => (m, r, n),
        [_, _, emitters, _] => {
            return Err(ConvertError::Layout(format!(
                "{emitters} emitters a measurement (a multi-speaker set); only one speaker a \
                 measurement is read"
            )));
        }
        _ => {
            return Err(ConvertError::Layout(format!(
                "Data.IR has {} dimensions, not measurements by receivers by samples",
                dims.len()
            )));
        }
    };
    if r != 2 {
        return Err(ConvertError::Layout(format!(
            "{r} receivers, not the two ears"
        )));
    }
    let elements = m.checked_mul(r * n).filter(|total| *total <= MAX_ELEMENTS);
    if elements.is_none()
        || elements.map(|total| total * ir_object.dt.size as usize) != Some(ir_object.data.len())
    {
        return Err(ConvertError::Layout(
            "Data.IR does not fit its dimensions".into(),
        ));
    }
    let ir: Vec<f32> = numbers(&ir_object)?.into_iter().map(|v| v as f32).collect();
    if m == 0 || n == 0 || m.checked_mul(r * n) != Some(ir.len()) {
        return Err(ConvertError::NotSofa("Data.IR is incomplete".into()));
    }

    let rate = child("Data.SamplingRate")?
        .map(|object| numbers(&object))
        .transpose()?
        .and_then(|values| values.first().copied())
        .filter(|rate| *rate > 0.0)
        .ok_or_else(|| missing("Data.SamplingRate"))?;

    let source = child("SourcePosition")?.ok_or_else(|| missing("SourcePosition"))?;
    let raw = numbers(&source)?;
    if raw.len() != m.saturating_mul(3) {
        return Err(ConvertError::Layout(format!(
            "{} source position values for {m} measurements",
            raw.len()
        )));
    }
    let positions = raw
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| direction(&coordinate_type(&source), *p))
        .collect();

    let delay = match child("Data.Delay")? {
        None => vec![0.0; r],
        Some(object) => numbers(&object)?,
    };
    if delay.len() != r && delay.len() != m.saturating_mul(r) {
        return Err(ConvertError::Layout(format!(
            "Data.Delay has {} values for {m} measurements and {r} receivers",
            delay.len()
        )));
    }

    // Receiver 0 is the left ear unless the file puts it at the negative side.
    let swap_ears = match child("ReceiverPosition")? {
        Some(object) => {
            let kind = coordinate_type(&object);
            let values = numbers(&object)?;
            if values.len() >= 6 {
                let side =
                    |at: usize| cartesian(&kind, [values[at], values[at + 1], values[at + 2]])[1];
                side(0) < side(3)
            } else {
                false
            }
        }
        None => false,
    };

    let sofa = Sofa {
        rate,
        shape: (m, r, n),
        ir,
        positions,
        delay,
        swap_ears,
    };
    validate(&sofa)?;
    Ok(sofa)
}

/// The coordinate type an array declares: `spherical` or `cartesian` (the default).
fn coordinate_type(object: &DataObject) -> String {
    object
        .parsed_attributes
        .iter()
        .find(|attr| attr.name == "Type")
        .and_then(|attr| attr.value.as_deref())
        .map_or_else(
            || "cartesian".to_string(),
            |v| v.trim_matches(['\0', ' ']).to_string(),
        )
}

/// The direction of a position given in `kind` coordinates.
fn direction(kind: &str, p: [f64; 3]) -> Direction {
    if kind == "spherical" {
        return Direction {
            az_deg: p[0].rem_euclid(360.0),
            el_deg: p[1],
            distance_m: p[2],
        };
    }
    let distance = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
    Direction {
        az_deg: p[1].atan2(p[0]).to_degrees().rem_euclid(360.0),
        el_deg: if distance > 0.0 {
            (p[2] / distance).asin().to_degrees()
        } else {
            0.0
        },
        distance_m: distance,
    }
}

/// A position in `kind` coordinates as x (front), y (left), z (up).
fn cartesian(kind: &str, p: [f64; 3]) -> [f64; 3] {
    if kind != "spherical" {
        return p;
    }
    let (az, el) = (p[0].to_radians(), p[1].to_radians());
    [
        p[2] * el.cos() * az.cos(),
        p[2] * el.cos() * az.sin(),
        p[2] * el.sin(),
    ]
}

/// The numbers of a dataset as `f64`, whatever its stored type.
fn numbers(object: &DataObject) -> Result<Vec<f64>, ConvertError> {
    if object.data.len() > MAX_ELEMENTS * 8 {
        return Err(ConvertError::Layout(format!(
            "{} is too large",
            object.name
        )));
    }
    let class = object.dt.class_and_version & 0x0F;
    let size = object.dt.size as usize;
    let big = object.dt.class_bit_field & 1 == 1;
    let unreadable = || {
        ConvertError::NotSofa(format!(
            "{} has a stored type that is not read",
            object.name
        ))
    };
    let read = |chunk: &[u8]| -> Option<f64> {
        macro_rules! num {
            ($t:ty) => {{
                let raw: [u8; std::mem::size_of::<$t>()] = chunk.try_into().ok()?;
                if big {
                    <$t>::from_be_bytes(raw)
                } else {
                    <$t>::from_le_bytes(raw)
                }
            }};
        }
        match (class, size) {
            (1, 4) => Some(f64::from(num!(f32))),
            (1, 8) => Some(num!(f64)),
            (0, 1) => Some(f64::from(chunk[0] as i8)),
            (0, 2) => Some(f64::from(num!(i16))),
            (0, 4) => Some(f64::from(num!(i32))),
            (0, 8) => Some(num!(i64) as f64),
            _ => None,
        }
    };
    if size == 0 || !matches!((class, size), (1, 4 | 8) | (0, 1 | 2 | 4 | 8)) {
        return Err(unreadable());
    }
    object
        .data
        .chunks_exact(size)
        .map(|chunk| read(chunk).ok_or_else(unreadable))
        .collect()
}

/// The angular distance between two directions, in degrees.
fn angle_between(a: &Direction, az_deg: f64, el_deg: f64) -> f64 {
    let unit = |az: f64, el: f64| {
        let (az, el) = (az.to_radians(), el.to_radians());
        [el.cos() * az.cos(), el.cos() * az.sin(), el.sin()]
    };
    let (u, v) = (unit(a.az_deg, a.el_deg), unit(az_deg, el_deg));
    let dot = (u[0] * v[0] + u[1] * v[1] + u[2] * v[2]).clamp(-1.0, 1.0);
    dot.acos().to_degrees()
}

/// The measurement nearest `az_deg` on the horizon: the nearest direction, and among those
/// within half a degree of it the one nearest 1.5 m.
fn nearest(sofa: &Sofa, az_deg: f64, tolerance: f64) -> Result<usize, ConvertError> {
    let distances: Vec<f64> = sofa
        .positions
        .iter()
        .map(|position| angle_between(position, az_deg, 0.0))
        .collect();
    let best = distances.iter().copied().fold(f64::INFINITY, f64::min);
    let by_distance = |i: &usize| (sofa.positions[*i].distance_m - PREFERRED_DISTANCE).abs();
    let pick = (0..distances.len())
        .filter(|i| distances[*i] <= best + 0.5)
        .min_by(|a, b| by_distance(a).total_cmp(&by_distance(b)))
        .ok_or(ConvertError::Silent)?;
    if best > tolerance {
        let found = (0..distances.len())
            .min_by(|a, b| distances[*a].total_cmp(&distances[*b]))
            .map_or(0.0, |i| sofa.positions[i].az_deg);
        return Err(ConvertError::NoMeasurement {
            wanted: az_deg.rem_euclid(360.0),
            found,
            tolerance,
        });
    }
    Ok(pick)
}

/// The four responses of the two speakers at `left_az` and `right_az` degrees on the horizon:
/// each speaker's measurement gives the left ear's and the right ear's response. `Data.Delay`
/// is applied as whole-sample shifts.
pub(crate) fn pick(
    sofa: &Sofa,
    left_az: f64,
    right_az: f64,
    tolerance: f64,
) -> Result<(Irs, Angles), ConvertError> {
    validate(sofa)?;
    let (_, receivers, n) = sofa.shape;
    let left = nearest(sofa, left_az, tolerance)?;
    let right = nearest(sofa, right_az, tolerance)?;
    let rate = sofa.rate.round();
    let ear = |measurement: usize, ear: usize| -> Result<Vec<f32>, ConvertError> {
        let receiver = if sofa.swap_ears { 1 - ear } else { ear };
        let layout = || ConvertError::Layout("the arrays do not fit the dimensions".into());
        let index = measurement
            .checked_mul(receivers)
            .and_then(|v| v.checked_add(receiver))
            .ok_or_else(layout)?;
        let start = index.checked_mul(n).ok_or_else(layout)?;
        let end = start.checked_add(n).ok_or_else(layout)?;
        let at = if sofa.delay.len() == receivers {
            receiver
        } else {
            index
        };
        // A negative delay is not applied: the response keeps its own start.
        let shift = sofa
            .delay
            .get(at)
            .copied()
            .ok_or_else(layout)?
            .round()
            .max(0.0);
        if shift > MAX_SHIFT {
            return Err(ConvertError::Layout(format!("a delay of {shift} samples")));
        }
        let mut taps = vec![0.0; shift as usize];
        taps.extend_from_slice(sofa.ir.get(start..end).ok_or_else(layout)?);
        Ok(taps)
    };
    let irs = Irs {
        rate: rate as u32,
        ch: [ear(left, 0)?, ear(left, 1)?, ear(right, 0)?, ear(right, 1)?],
    };
    let angles = Angles {
        left: sofa.positions[left],
        right: sofa.positions[right],
    };
    Ok((irs, angles))
}

#[cfg(test)]
mod tests;
