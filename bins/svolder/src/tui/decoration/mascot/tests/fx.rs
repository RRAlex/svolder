// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The entrance and exit animations: each one ends in the landed pose, stays inside the grid,
//! paints only letters every costume colours, and differs from the landed pose on the way.

use std::time::Duration;

use super::super::fx::{Entrance, Fx, apply};
use super::super::looks::{self, Costume, Paint, Season};
use super::super::tricks::{Beat, State};
use super::super::{BOX_H, BOX_W};

type Grid = Vec<Vec<Option<char>>>;

const COSTUMES: [Costume; 5] = [
    Costume::Amber,
    Costume::Ice,
    Costume::Red,
    Costume::Neon,
    Costume::God,
];

const SEASONS: [Season; 18] = [
    Season::NewYear,
    Season::Groundhog,
    Season::Pi,
    Season::Alien,
    Season::Towel,
    Season::Midsummer,
    Season::SaintJean,
    Season::Pride,
    Season::Canada,
    Season::Independence,
    Season::Bastille,
    Season::CatInfinity,
    Season::Judgment,
    Season::Pirate,
    Season::Truth,
    Season::Halloween,
    Season::Remembrance,
    Season::Yule,
];

/// The resting squirrel of `costume` and `season` in a grid `width` wide.
fn landed(costume: Costume, season: Option<Season>, width: usize) -> Grid {
    let dress = looks::dress(costume, season, Duration::ZERO);
    let beat = Beat {
        since: Duration::MAX,
        count: 0,
    };
    State::Rest
        .pose(Duration::ZERO, beat, &dress)
        .pixels_wide(width)
}

fn fx(kind: Entrance, out: bool, progress: f64) -> Fx {
    Fx {
        kind,
        seed: 7,
        out,
        progress,
    }
}

#[test]
fn every_entrance_ends_in_the_landed_pose_in_every_costume_and_season() {
    for width in [BOX_W, BOX_W + 30] {
        for costume in COSTUMES {
            let seasons = std::iter::once(None).chain(SEASONS.iter().copied().map(Some));
            for season in seasons {
                let pose = landed(costume, season, width);
                for kind in Entrance::ALL {
                    assert_eq!(
                        apply(&fx(kind, false, 1.0), pose.clone()),
                        pose,
                        "{kind:?} in {costume:?} {season:?} at {width}"
                    );
                }
            }
        }
    }
}

#[test]
fn every_exit_ends_empty() {
    for width in [BOX_W, BOX_W + 30] {
        let pose = landed(Costume::Amber, None, width);
        for kind in Entrance::ALL {
            let gone = apply(&fx(kind, true, 1.0), pose.clone());
            assert!(
                gone.iter().flatten().all(Option::is_none),
                "{kind:?} leaves cells at {width}"
            );
        }
    }
}

#[test]
fn every_entrance_starts_without_the_squirrel_and_differs_on_the_way() {
    let pose = landed(Costume::Amber, None, BOX_W);
    for kind in Entrance::ALL {
        let start = apply(&fx(kind, false, 0.0), pose.clone());
        let squirrel = |grid: &Grid| {
            grid.iter()
                .zip(&pose)
                .flat_map(|(a, b)| a.iter().zip(b))
                .filter(|(a, b)| a.is_some() && a == b)
                .count()
        };
        assert_eq!(squirrel(&start), 0, "{kind:?} shows the squirrel at 0");
        let moved =
            (1..20).any(|n| apply(&fx(kind, false, f64::from(n) / 20.0), pose.clone()) != pose);
        assert!(moved, "{kind:?} never differs from the landed pose");
    }
}

#[test]
fn frames_keep_the_grid_and_paint_only_known_letters() {
    for width in [BOX_W, BOX_W + 30] {
        let pose = landed(Costume::Amber, None, width);
        for kind in Entrance::ALL {
            for out in [false, true] {
                for n in 0..=40 {
                    let frame = apply(&fx(kind, out, f64::from(n) / 40.0), pose.clone());
                    assert_eq!(frame.len(), BOX_H);
                    for row in &frame {
                        assert_eq!(row.len(), width, "{kind:?} out={out} at {n}");
                    }
                    for costume in COSTUMES {
                        let paint = Paint {
                            costume,
                            accent: None,
                            season: None,
                        };
                        for letter in frame.iter().flatten().flatten() {
                            let _ = paint.rgb(*letter, 0, 0, Duration::ZERO);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn effects_stay_inside_the_box_columns() {
    let width = BOX_W + 30;
    let pose = landed(Costume::Amber, None, width);
    for kind in Entrance::ALL {
        for out in [false, true] {
            for n in 0..=40 {
                let frame = apply(&fx(kind, out, f64::from(n) / 40.0), pose.clone());
                if kind == Entrance::Longship {
                    continue;
                }
                for row in &frame {
                    assert!(
                        row[..width - BOX_W].iter().all(Option::is_none),
                        "{kind:?} out={out} at {n} strays left of the box"
                    );
                }
            }
        }
    }
}

#[test]
fn the_seed_changes_the_noise_but_not_the_ending() {
    let pose = landed(Costume::Amber, None, BOX_W);
    let a = apply(
        &Fx {
            seed: 1,
            ..fx(Entrance::Sparkle, false, 0.4)
        },
        pose.clone(),
    );
    let b = apply(
        &Fx {
            seed: 2,
            ..fx(Entrance::Sparkle, false, 0.4)
        },
        pose.clone(),
    );
    assert_ne!(a, b);
}

#[test]
fn the_squirrel_never_overlaps_the_sinking_longship() {
    let pose = landed(Costume::Amber, None, BOX_W);
    let blank: Grid = vec![vec![None; BOX_W]; BOX_H];
    for n in 70..80 {
        let t = f64::from(n) / 100.0;
        assert_eq!(
            apply(&fx(Entrance::Longship, false, t), pose.clone()),
            apply(&fx(Entrance::Longship, false, t), blank.clone()),
            "at {t}"
        );
    }
}
