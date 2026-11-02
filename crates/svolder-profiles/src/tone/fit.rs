// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fitting a headphone correction to the tone control's peaking bands.
//!
//! [`fit`] finds, for a device's bands, the frequencies, gains and Qs that come closest to a
//! [`Correction`]'s summed response, every value on one of the device's own raw steps and every
//! band inside its own window, so a recall writes exactly what the fit says.
//!
//! A correction that is already one peaking filter per band maps straight across. Anything else
//! runs a Levenberg-Marquardt least-squares fit from three deterministic starts, then snaps to
//! the device's steps and searches the steps next door. The solver has no bounds, so each band's
//! values reach it through a logistic that squeezes the whole real line onto the band's range.
//! Nothing is random: the same input gives the same bands on every run.

use levenberg_marquardt::{LeastSquaresProblem, LevenbergMarquardt};
use nalgebra::{DMatrix, DVector, Dyn, storage::Owned};

use crate::convert::{EqParam, frequency_text};
use crate::model::EqSpec;
use crate::tone::apo::{Correction, FilterKind};
use crate::tone::preset::PresetBand;
use crate::tone::response::{BandParams, Grid, PEAK_SHOWN_DB, correction_db, log_freqs, peak_db};

/// How many log-spaced points from 20 Hz to 20 kHz the target is sampled at.
const POINTS: usize = 256;
/// Up to this frequency the fit weighs every point fully; above it, [`HIGH_WEIGHT`].
const TRUSTED_HZ: f64 = 10_000.0;
/// The weight of a point above [`TRUSTED_HZ`], where a headphone measurement is least reliable.
const HIGH_WEIGHT: f64 = 0.2;
/// The Q the third start fixes every band at.
const FIXED_Q: f64 = 0.7;
/// The Q the centre start gives every band.
const CENTRE_Q: f64 = 1.0;
/// How far the free variable moves for one central difference of the Jacobian.
const DIFFERENCE: f64 = 1e-4;
/// The nearest the logistic's input is allowed to start to a band's range edge, as a fraction
/// of the range.
const EDGE_MARGIN: f64 = 1e-3;
/// Steps a band parameter is tried at either side of where it is, in the step search.
const STEP_TRIES: [i16; 6] = [1, -1, 2, -2, 4, -4];
/// The solver's patience: it stops after this many times the number of free variables in
/// evaluations. More only wanders along the logistic's flat edges; the step search finishes
/// the job.
const SOLVER_PATIENCE: usize = 10;
/// The most sweeps of the step search.
const MAX_SWEEPS: usize = 50;

/// The result of fitting a correction to a device.
#[derive(Debug, Clone, PartialEq)]
pub struct Fit {
    /// One band per band of the device, each value exactly on a raw step.
    pub bands: Vec<PresetBand>,
    /// The root-mean-square error between the fit and the correction in dB over 20 Hz to
    /// 10 kHz, unweighted.
    pub rms_db: f64,
    /// The largest error in dB over 20 Hz to 10 kHz, as a magnitude.
    pub worst_db: f64,
    /// Where that largest error is, in Hz.
    pub worst_hz: f64,
    /// How far the fitted bands boost at most, in dB: [`peak_db`] of the bands.
    pub peak_db: f64,
}

impl Fit {
    /// The fit as one line: `fit 0.9 dB RMS · worst 2.4 dB at 8.0 kHz`, with
    /// ` · lower volume 3.5 dB` after it when the bands boost by at least [`PEAK_SHOWN_DB`].
    pub fn line(&self) -> String {
        let at = match self.worst_hz < 1000.0 {
            true => format!("{} Hz", frequency_text(self.worst_hz)),
            false => format!("{:.1} kHz", self.worst_hz / 1000.0),
        };
        let mut line = format!(
            "fit {:.1} dB RMS · worst {:.1} dB at {at}",
            self.rms_db, self.worst_db
        );
        if self.peak_db >= PEAK_SHOWN_DB {
            line.push_str(&format!(" · lower volume {:.1} dB", self.peak_db));
        }
        line
    }
}

/// Fits `correction` to the bands of `eq`: peaking filters on the device's steps.
pub fn fit(eq: &EqSpec, correction: &Correction) -> Fit {
    let search = Search::new(eq, correction);
    let steps = search.direct().unwrap_or_else(|| search.best_of_starts());
    search.finish(&steps)
}

/// One band's raw steps: frequency, gain and Q.
type Steps = [u8; 3];

/// A band parameter's range and whether it is spread on a log scale.
#[derive(Debug, Clone, Copy)]
struct Axis {
    low: f64,
    high: f64,
    log: bool,
}

impl Axis {
    /// The axis between two ends in either order; a NaN end is ignored, so `value` and `clamp`
    /// never see a reversed range.
    fn new(a: f64, b: f64, log: bool) -> Axis {
        Axis {
            low: a.min(b),
            high: a.max(b),
            log,
        }
    }

    /// The value the free variable `x` stands for, always inside the range.
    fn value(self, x: f64) -> f64 {
        let t = 1.0 / (1.0 + (-x).exp());
        let value = match self.log {
            true => (self.low.ln() + (self.high.ln() - self.low.ln()) * t).exp(),
            false => self.low + (self.high - self.low) * t,
        };
        value.max(self.low).min(self.high)
    }

    /// The free variable that stands for `value`, kept off the range's very edges where the
    /// logistic is flat.
    fn free(self, value: f64) -> f64 {
        let t = match self.log {
            true => (value.ln() - self.low.ln()) / (self.high.ln() - self.low.ln()),
            false => (value - self.low) / (self.high - self.low),
        };
        let t = t.clamp(EDGE_MARGIN, 1.0 - EDGE_MARGIN);
        (t / (1.0 - t)).ln()
    }

    fn clamp(self, value: f64) -> f64 {
        value.max(self.low).min(self.high)
    }
}

/// The three axes of one band: frequency, gain and Q.
type Axes = [Axis; 3];

/// The target, the weights and the device's ranges: everything a candidate is judged by.
struct Search<'a> {
    eq: &'a EqSpec,
    correction: &'a Correction,
    /// The log-spaced frequencies the fit is judged at.
    freqs: Vec<f64>,
    /// The trigonometry of those frequencies, worked out once.
    grid: Grid,
    /// The correction's summed response at each of them, in dB.
    target: Vec<f64>,
    /// How much each point's squared error counts.
    weight: Vec<f64>,
    /// Every band's three axes, from its own window and the device's ranges.
    axes: Vec<Axes>,
}

impl<'a> Search<'a> {
    fn new(eq: &'a EqSpec, correction: &'a Correction) -> Self {
        let freqs = log_freqs(POINTS);
        let target = correction_db(correction, &freqs);
        let weight = freqs
            .iter()
            .map(|&f| if f <= TRUSTED_HZ { 1.0 } else { HIGH_WEIGHT })
            .collect();
        let top = EqSpec::STEP_MAX;
        let axes = (0..eq.bands.len())
            .map(|band| {
                let (low, high) = (eq.frequency(band, 0), eq.frequency(band, top));
                [
                    Axis::new(low.unwrap_or(1.0), high.unwrap_or(1.0), true),
                    Axis::new(eq.gain_db(0), eq.gain_db(top), false),
                    Axis::new(eq.q(0), eq.q(top), true),
                ]
            })
            .collect();
        Search {
            eq,
            correction,
            grid: Grid::new(&freqs),
            freqs,
            target,
            weight,
            axes,
        }
    }

    /// One band's response at the fit's frequencies.
    fn curve(&self, band: &BandParams) -> Vec<f64> {
        self.grid.band_db(band)
    }

    /// The band `band` has at raw `steps`.
    fn band_at(&self, band: usize, steps: Steps) -> BandParams {
        BandParams {
            f0: self
                .eq
                .frequency(band, steps[0])
                .unwrap_or(self.axes[band][0].low),
            gain_db: self.eq.gain_db(steps[1]),
            q: self.eq.q(steps[2]),
        }
    }

    fn bands_at(&self, steps: &[Steps]) -> Vec<BandParams> {
        steps
            .iter()
            .enumerate()
            .map(|(band, &s)| self.band_at(band, s))
            .collect()
    }

    /// The weighted squared error of a summed response.
    fn cost(&self, total: &[f64]) -> f64 {
        (0..total.len())
            .map(|i| self.weight[i] * (total[i] - self.target[i]).powi(2))
            .sum()
    }

    /// The summed response of `bands` at the fit's frequencies.
    fn total(&self, bands: &[BandParams]) -> Vec<f64> {
        let mut total = vec![0.0; self.freqs.len()];
        for band in bands {
            for (sum, db) in total.iter_mut().zip(self.grid.band_db(band)) {
                *sum += db;
            }
        }
        total
    }

    /// The raw steps nearest to `bands`.
    fn snap(&self, bands: &[BandParams]) -> Vec<Steps> {
        bands
            .iter()
            .enumerate()
            .map(|(band, b)| {
                let step = |param, value| self.eq.step(band, param, value).unwrap_or(64);
                [
                    step(EqParam::Frequency, b.f0),
                    step(EqParam::Gain, b.gain_db),
                    step(EqParam::Q, b.q),
                ]
            })
            .collect()
    }

    /// A band at the centre of its window with no gain, as a spare band stands.
    fn flat_band(&self, band: usize) -> BandParams {
        BandParams {
            f0: self
                .eq
                .frequency(band, 64)
                .unwrap_or(self.axes[band][0].low),
            gain_db: 0.0,
            q: self.eq.q_center,
        }
    }

    /// One filter per band, when the correction is exactly that: no more filters than bands,
    /// all peaking, each in a different band's window, with a gain and a Q the band reaches.
    fn direct(&self) -> Option<Vec<Steps>> {
        let bands = self.axes.len();
        if self.correction.filters.len() > bands
            || self
                .correction
                .filters
                .iter()
                .any(|f| f.kind != FilterKind::Peaking)
        {
            return None;
        }
        let mut sorted: Vec<_> = self.correction.filters.iter().collect();
        sorted.sort_by(|a, b| a.f.total_cmp(&b.f));
        let mut chosen: Vec<BandParams> = (0..bands).map(|b| self.flat_band(b)).collect();
        let mut last = None;
        for filter in sorted {
            let band = self
                .axes
                .iter()
                .position(|[f, _, _]| (f.low..=f.high).contains(&filter.f))?;
            let [_, gain, q] = self.axes[band];
            let reaches =
                (gain.low..=gain.high).contains(&filter.g) && (q.low..=q.high).contains(&filter.q);
            if last.is_some_and(|before| band <= before) || !reaches {
                return None;
            }
            last = Some(band);
            chosen[band] = BandParams {
                f0: filter.f,
                gain_db: filter.g,
                q: filter.q,
            };
        }
        Some(self.snap(&chosen))
    }

    /// The best of the three starts, each solved and snapped to the device's steps.
    fn best_of_starts(&self) -> Vec<Steps> {
        if self.axes.is_empty() {
            return Vec::new();
        }
        let starts = [
            self.greedy(None),
            self.centres(),
            self.greedy(Some(FIXED_Q)),
        ];
        let mut best: Option<(Vec<Steps>, f64, f64)> = None;
        for start in starts {
            let steps = self.solve(&start);
            let bands = self.bands_at(&steps);
            let cost = self.cost(&self.total(&bands));
            let peak = peak_db(&bands);
            let wins = match &best {
                None => true,
                Some((_, held_cost, held_peak)) => {
                    match (cost - held_cost).abs() <= 1e-9 * (1.0 + held_cost) {
                        true => peak < *held_peak,
                        false => cost < *held_cost,
                    }
                }
            };
            if wins {
                best = Some((steps, cost, peak));
            }
        }
        best.map(|(steps, _, _)| steps).unwrap_or_default()
    }

    /// Every band on its centre, the correction's gain there, Q 1.
    fn centres(&self) -> Vec<BandParams> {
        (0..self.axes.len())
            .map(|band| {
                let [_, gain, q] = self.axes[band];
                let centre = self.flat_band(band).f0;
                let wanted = correction_db(self.correction, &[centre])[0];
                BandParams {
                    f0: centre,
                    gain_db: gain.clamp(wanted),
                    q: q.clamp(CENTRE_Q),
                }
            })
            .collect()
    }

    /// Bands placed one at a time on the largest weighted error left inside a free band's
    /// window, with the gain the error has there and the Q of the bump's half-height width,
    /// or `fixed_q` when given.
    fn greedy(&self, fixed_q: Option<f64>) -> Vec<BandParams> {
        let bands = self.axes.len();
        let mut left: Vec<f64> = self.target.clone();
        let mut placed: Vec<Option<BandParams>> = vec![None; bands];
        for _ in 0..bands {
            let mut pick: Option<(usize, usize, f64)> = None;
            for band in (0..bands).filter(|&b| placed[b].is_none()) {
                let window = self.axes[band][0];
                for (i, &f) in self.freqs.iter().enumerate() {
                    let size = self.weight[i] * left[i].abs();
                    if (window.low..=window.high).contains(&f)
                        && pick.is_none_or(|(_, _, best)| size > best)
                    {
                        pick = Some((band, i, size));
                    }
                }
            }
            let (band, bump) = match pick {
                Some((band, i, size)) if size > 1e-9 => (band, Some(i)),
                Some((band, _, _)) => (band, None),
                None => (placed.iter().position(Option::is_none).unwrap_or(0), None),
            };
            let [_, gain, q] = self.axes[band];
            let chosen = match bump {
                None => self.flat_band(band),
                Some(i) => BandParams {
                    f0: self.freqs[i],
                    gain_db: gain.clamp(left[i]),
                    q: q.clamp(fixed_q.unwrap_or_else(|| self.bump_q(&left, i))),
                },
            };
            let curve = self.curve(&chosen);
            for (l, c) in left.iter_mut().zip(curve) {
                *l -= c;
            }
            placed[band] = Some(chosen);
        }
        placed.into_iter().flatten().collect()
    }

    /// The Q of the bump in `error` around point `i`: the point's frequency over the width at
    /// half its height, the width running out to the grid's edge where the bump does not fall
    /// that far.
    fn bump_q(&self, error: &[f64], i: usize) -> f64 {
        let peak = error[i].abs();
        let half = peak / 2.0;
        let inside = |j: usize| error[j] * error[i] > 0.0 && error[j].abs() >= half;
        let log_f = |j: usize| self.freqs[j].ln();
        let crossing = |inner: usize, outer: usize| {
            let (a, b) = (error[inner].abs(), error[outer].abs());
            let t = match (a - b).abs() < 1e-12 {
                true => 0.0,
                false => ((a - half) / (a - b)).clamp(0.0, 1.0),
            };
            (log_f(inner) + t * (log_f(outer) - log_f(inner))).exp()
        };
        let mut low = i;
        while low > 0 && inside(low - 1) {
            low -= 1;
        }
        let f_low = if low == 0 {
            self.freqs[0]
        } else {
            crossing(low, low - 1)
        };
        let mut high = i;
        while high + 1 < self.freqs.len() && inside(high + 1) {
            high += 1;
        }
        let f_high = if high + 1 == self.freqs.len() {
            self.freqs[high]
        } else {
            crossing(high, high + 1)
        };
        let width = f_high - f_low;
        match width > 0.0 {
            true => self.freqs[i] / width,
            false => f64::INFINITY,
        }
    }

    /// Runs the solver from `start`, snaps the result to the device's steps and searches the
    /// steps around it.
    fn solve(&self, start: &[BandParams]) -> Vec<Steps> {
        let x = DVector::from_iterator(
            start.len() * 3,
            start.iter().enumerate().flat_map(|(band, b)| {
                let [f, g, q] = self.axes[band];
                [f.free(b.f0), g.free(b.gain_db), q.free(b.q)]
            }),
        );
        let (problem, _report) = LevenbergMarquardt::new()
            .with_patience(SOLVER_PATIENCE)
            .minimize(Problem::new(self, x));
        let solved = self.bands_of(&problem.x);
        let bands = match solved
            .iter()
            .all(|b| b.f0.is_finite() && b.gain_db.is_finite() && b.q.is_finite())
        {
            true => solved,
            false => start.to_vec(),
        };
        let mut steps = self.snap(&bands);
        self.search_steps(&mut steps);
        steps
    }

    /// The bands the free variables `x` stand for.
    fn bands_of(&self, x: &DVector<f64>) -> Vec<BandParams> {
        self.axes
            .iter()
            .enumerate()
            .map(|(band, [f, g, q])| BandParams {
                f0: f.value(x[band * 3]),
                gain_db: g.value(x[band * 3 + 1]),
                q: q.value(x[band * 3 + 2]),
            })
            .collect()
    }

    /// Moves each step of each band by 1, 2 or 4 either way when that lowers the error,
    /// sweeping until a sweep changes nothing or [`MAX_SWEEPS`] have run.
    fn search_steps(&self, steps: &mut [Steps]) {
        let mut curves: Vec<Vec<f64>> =
            self.bands_at(steps).iter().map(|b| self.curve(b)).collect();
        let mut total: Vec<f64> = (0..self.freqs.len())
            .map(|i| curves.iter().map(|c| c[i]).sum())
            .collect();
        let mut cost = self.cost(&total);
        for _ in 0..MAX_SWEEPS {
            let mut improved = false;
            for band in 0..steps.len() {
                for param in 0..3 {
                    let mut best: Option<(u8, Vec<f64>, f64)> = None;
                    let mut held = cost;
                    for delta in STEP_TRIES {
                        let moved = i16::from(steps[band][param]) + delta;
                        if !(0..=i16::from(EqSpec::STEP_MAX)).contains(&moved) {
                            continue;
                        }
                        let mut trial = steps[band];
                        trial[param] = moved as u8;
                        let curve = self.curve(&self.band_at(band, trial));
                        let trial_cost: f64 = (0..total.len())
                            .map(|i| {
                                let sum = total[i] - curves[band][i] + curve[i];
                                self.weight[i] * (sum - self.target[i]).powi(2)
                            })
                            .sum();
                        if trial_cost < held - 1e-12 * (1.0 + held) {
                            held = trial_cost;
                            best = Some((trial[param], curve, trial_cost));
                        }
                    }
                    if let Some((step, curve, new_cost)) = best {
                        for i in 0..total.len() {
                            total[i] += curve[i] - curves[band][i];
                        }
                        curves[band] = curve;
                        steps[band][param] = step;
                        cost = new_cost;
                        improved = true;
                    }
                }
            }
            if !improved {
                break;
            }
        }
    }

    /// The fit for `steps`, with its errors measured against the correction.
    fn finish(&self, steps: &[Steps]) -> Fit {
        let bands = self.bands_at(steps);
        let total = self.total(&bands);
        let (mut sum, mut count) = (0.0, 0usize);
        let (mut worst_db, mut worst_hz) = (0.0f64, self.freqs[0]);
        for (i, &f) in self.freqs.iter().enumerate() {
            if f > TRUSTED_HZ {
                break;
            }
            let error = (total[i] - self.target[i]).abs();
            sum += error * error;
            count += 1;
            if error > worst_db {
                (worst_db, worst_hz) = (error, f);
            }
        }
        Fit {
            peak_db: peak_db(&bands),
            rms_db: (sum / count.max(1) as f64).sqrt(),
            worst_db,
            worst_hz,
            bands: bands
                .iter()
                .map(|b| PresetBand {
                    f: b.f0,
                    g: b.gain_db,
                    q: b.q,
                })
                .collect(),
        }
    }
}

/// The least-squares problem the solver runs: free variables in, weighted errors out.
struct Problem<'a> {
    search: &'a Search<'a>,
    x: DVector<f64>,
    /// Each band's response at the current `x`.
    curves: Vec<Vec<f64>>,
}

impl<'a> Problem<'a> {
    fn new(search: &'a Search<'a>, x: DVector<f64>) -> Self {
        let mut problem = Problem {
            search,
            x: x.clone(),
            curves: Vec::new(),
        };
        problem.set_params(&x);
        problem
    }

    /// Band `band`'s response with its three free variables taken from `x`.
    fn band_curve(&self, band: usize, x: &DVector<f64>) -> Vec<f64> {
        let [f, g, q] = self.search.axes[band];
        self.search.curve(&BandParams {
            f0: f.value(x[band * 3]),
            gain_db: g.value(x[band * 3 + 1]),
            q: q.value(x[band * 3 + 2]),
        })
    }

    fn root_weight(&self, i: usize) -> f64 {
        self.search.weight[i].sqrt()
    }
}

impl LeastSquaresProblem<f64, Dyn, Dyn> for Problem<'_> {
    type ParameterStorage = Owned<f64, Dyn>;
    type ResidualStorage = Owned<f64, Dyn>;
    type JacobianStorage = Owned<f64, Dyn, Dyn>;

    fn set_params(&mut self, x: &DVector<f64>) {
        self.x.copy_from(x);
        self.curves = (0..self.search.axes.len())
            .map(|band| self.band_curve(band, x))
            .collect();
    }

    fn params(&self) -> DVector<f64> {
        self.x.clone()
    }

    fn residuals(&self) -> Option<DVector<f64>> {
        let residuals = DVector::from_iterator(
            self.search.freqs.len(),
            (0..self.search.freqs.len()).map(|i| {
                let total: f64 = self.curves.iter().map(|c| c[i]).sum();
                self.root_weight(i) * (total - self.search.target[i])
            }),
        );
        residuals.iter().all(|r| r.is_finite()).then_some(residuals)
    }

    /// Central differences: each variable moves [`DIFFERENCE`] either way and only its own
    /// band's response is worked out again.
    fn jacobian(&self) -> Option<DMatrix<f64>> {
        let points = self.search.freqs.len();
        let mut jacobian = DMatrix::zeros(points, self.x.len());
        for column in 0..self.x.len() {
            let band = column / 3;
            let mut ahead = self.x.clone();
            ahead[column] += DIFFERENCE;
            let mut behind = self.x.clone();
            behind[column] -= DIFFERENCE;
            let (up, down) = (
                self.band_curve(band, &ahead),
                self.band_curve(band, &behind),
            );
            for i in 0..points {
                jacobian[(i, column)] =
                    self.root_weight(i) * (up[i] - down[i]) / (2.0 * DIFFERENCE);
            }
        }
        jacobian.iter().all(|v| v.is_finite()).then_some(jacobian)
    }
}

#[cfg(test)]
mod tests;
