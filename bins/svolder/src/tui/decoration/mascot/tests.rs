// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sprite sheet, the costumes and the seasons.

use std::time::Duration;

use super::looks::{self, Costume, Paint, Season};
use super::{BOX_H, BOX_W, ORIGIN, sheet};

mod brain;
mod codes;
mod draw;

#[test]
fn every_block_parses_and_stays_inside_the_box() {
    for name in sheet::names() {
        let layer = sheet::layer(name);
        assert!(!layer.rows.is_empty(), "{name} has no rows");
        for (y, row) in layer.rows.iter().enumerate() {
            for (x, ch) in row.chars().enumerate() {
                if ch == '.' {
                    continue;
                }
                let bx = ORIGIN.0 + layer.dx + x as i32;
                let by = ORIGIN.1 + layer.dy + y as i32;
                // The ship rows in from outside the box, so only it may reach past the edges
                // sideways; nothing reaches above the headroom or below the feet.
                assert!(
                    (0..BOX_H as i32).contains(&by),
                    "{name} ({x},{y}) leaves the box"
                );
                if !name.starts_with("ship")
                    && !name.starts_with("oar")
                    && !name.starts_with("wake")
                {
                    assert!(
                        (0..BOX_W as i32).contains(&bx),
                        "{name} ({x},{y}) leaves the box"
                    );
                }
            }
        }
    }
}

#[test]
fn every_letter_has_a_colour_in_every_costume() {
    for costume in [
        Costume::Amber,
        Costume::Ice,
        Costume::Red,
        Costume::Neon,
        Costume::God,
    ] {
        let paint = Paint {
            costume,
            accent: None,
            season: None,
        };
        for name in sheet::names() {
            for row in &sheet::layer(name).rows {
                for ch in row.chars().filter(|ch| *ch != '.') {
                    // rgb panics on a letter it does not know
                    let _ = paint.rgb(ch, 0, 0, Duration::ZERO);
                }
            }
        }
    }
}

#[test]
fn the_seasons_fall_on_their_dates() {
    let cases = [
        ((12, 29), None),
        ((12, 30), Some(Season::NewYear)),
        ((1, 2), Some(Season::NewYear)),
        ((1, 3), None),
        ((2, 2), Some(Season::Groundhog)),
        ((3, 14), Some(Season::Pi)),
        ((4, 26), Some(Season::Alien)),
        ((5, 25), Some(Season::Towel)),
        ((6, 18), None),
        ((6, 19), Some(Season::Midsummer)),
        ((6, 23), Some(Season::Midsummer)),
        ((6, 24), Some(Season::SaintJean)),
        ((6, 25), Some(Season::Midsummer)),
        ((6, 26), None),
        ((6, 28), Some(Season::Pride)),
        ((7, 1), Some(Season::Canada)),
        ((7, 4), Some(Season::Independence)),
        ((7, 14), Some(Season::Bastille)),
        ((8, 8), Some(Season::CatInfinity)),
        ((8, 29), Some(Season::Judgment)),
        ((9, 19), Some(Season::Pirate)),
        ((9, 30), Some(Season::Truth)),
        ((10, 23), None),
        ((10, 24), Some(Season::Halloween)),
        ((10, 31), Some(Season::Halloween)),
        ((11, 11), Some(Season::Remembrance)),
        ((12, 12), None),
        ((12, 13), Some(Season::Yule)),
        ((12, 26), Some(Season::Yule)),
        ((12, 27), None),
    ];
    for ((month, day), want) in cases {
        assert_eq!(Season::on(month, day), want, "{month}/{day}");
    }
    assert!(Season::Truth.quiet() && Season::Remembrance.quiet() && !Season::Yule.quiet());
}

#[test]
fn a_season_hat_replaces_the_helmet_and_its_prop_the_horn() {
    let plain = looks::dress(Costume::Amber, None, Duration::ZERO);
    assert_eq!(
        (plain.hat, plain.carry.as_slice()),
        (Some("helmet"), &["horn"][..])
    );
    let france = looks::dress(Costume::Amber, Some(Season::Bastille), Duration::ZERO);
    assert_eq!(
        (france.hat, france.carry.as_slice()),
        (Some("beret"), &["baguette"][..])
    );
    let ice = looks::dress(Costume::Ice, None, Duration::ZERO);
    assert!(ice.wear.contains(&"scarf") && ice.wear.iter().any(|w| w.starts_with("breath")));
}

#[test]
fn the_costumes_take_the_palette_and_pride_paints_the_tail_by_row() {
    assert_eq!(Costume::of_palette("ice"), Costume::Ice);
    assert_eq!(Costume::of_palette("no such"), Costume::Amber);
    let pride = Paint {
        costume: Costume::Amber,
        accent: None,
        season: Some(Season::Pride),
    };
    assert_ne!(
        pride.rgb('T', 20, 2, Duration::ZERO),
        pride.rgb('T', 20, 10, Duration::ZERO)
    );
    let tinted = Paint {
        costume: Costume::Amber,
        accent: Some((0x00, 0x80, 0xff)),
        season: None,
    };
    let plain = Paint {
        costume: Costume::Amber,
        accent: None,
        season: None,
    };
    assert_ne!(
        tinted.rgb('F', 0, 0, Duration::ZERO),
        plain.rgb('F', 0, 0, Duration::ZERO)
    );
}

#[test]
fn pride_paints_the_tail_in_six_bands() {
    let pride = Paint {
        costume: Costume::Amber,
        accent: None,
        season: Some(Season::Pride),
    };
    let band = |y| pride.rgb('T', 20, y, Duration::ZERO);
    let hex = |v: u32| ((v >> 16) as u8, (v >> 8) as u8, v as u8);
    assert_eq!(band(4), hex(0xe40303));
    assert_eq!(band(5), hex(0xff8c00));
    assert_eq!(band(6), hex(0xffed00));
    assert_eq!(band(7), hex(0x008026));
    assert_eq!(band(8), hex(0x004dff));
    assert_eq!(band(9), hex(0x004dff));
    assert_eq!(band(10), hex(0x750787));
    assert_eq!(band(11), hex(0x750787));
}

#[test]
fn the_season_props_move() {
    let at = |m, d, ms| looks::dress(Costume::Amber, Season::on(m, d), Duration::from_millis(ms));
    let pi0 = at(3, 14, 0);
    let pi1 = at(3, 14, 250);
    assert!(pi0.carry.contains(&"steam1") && pi1.carry.contains(&"steam2"));
    for d in [&pi0, &pi1] {
        assert!(d.carry.contains(&"pi_sign") && d.carry.contains(&"pie"));
    }
    assert!(at(6, 24, 0).carry.contains(&"poutine"));
    assert!(at(6, 24, 750).carry.contains(&"steam4"));
    assert_eq!(at(5, 25, 0).carry, ["towel"]);
    assert_eq!(at(5, 25, 500).carry, ["towel2"]);
    assert_eq!(at(6, 20, 400).hat, Some("crown2"));
    assert_eq!(at(7, 1, 600).carry, ["maple2"]);
    let bastille = at(7, 14, 500);
    assert_eq!(bastille.carry, ["baguette2"]);
    assert_eq!(bastille.hat, Some("beret"));
    assert_eq!(at(8, 8, 450).carry, ["infinity2"]);
    assert_eq!(at(8, 29, 0).wear, ["shades"]);
    assert_eq!(at(8, 29, 1500).wear, ["shades2"]);
    assert_eq!(at(9, 19, 0).wear, ["eyepatch"]);
    assert_eq!(at(9, 19, 1300).wear, ["eyepatch2"]);
}

#[test]
fn every_layer_the_looks_name_is_on_the_sheet() {
    let names: Vec<&str> = sheet::names().collect();
    for costume in [
        Costume::Amber,
        Costume::Ice,
        Costume::Red,
        Costume::Neon,
        Costume::God,
    ] {
        for month in 1..=12u8 {
            for day in 1..=31u8 {
                for ms in (0..3000).step_by(100) {
                    let d =
                        looks::dress(costume, Season::on(month, day), Duration::from_millis(ms));
                    let all = d.hat.into_iter().chain(d.eyes).chain(d.carry).chain(d.wear);
                    for layer in all {
                        assert!(names.contains(&layer), "{layer} is not on the sheet");
                    }
                }
            }
        }
    }
}

mod tricks {
    use std::time::Duration;

    use super::super::looks::{self, Costume, Dress, Season};
    use super::super::tricks::{Beat, Dance, Pose, State, Trick};
    use super::super::{BOX_H, BOX_W, sheet};
    use crate::tui::test_support::ms;

    const TRICKS: [Trick; 12] = [
        Trick::Climb,
        Trick::Slide,
        Trick::BigSwing,
        Trick::Wake,
        Trick::Messenger,
        Trick::Palette,
        Trick::Hop,
        Trick::Stretch,
        Trick::Arrive,
        Trick::Depart,
        Trick::RowOut,
        Trick::RowIn,
    ];

    fn amber() -> Dress {
        looks::dress(Costume::Amber, None, Duration::ZERO)
    }

    fn has(pose: &Pose, layer: &str) -> bool {
        pose.parts.iter().any(|p| p.layer == layer)
    }

    fn part<'a>(pose: &'a Pose, layer: &str) -> &'a super::super::tricks::Part {
        pose.parts
            .iter()
            .find(|p| p.layer == layer)
            .unwrap_or_else(|| panic!("no {layer} in the pose"))
    }

    fn beat(since: u64, count: u64) -> Beat {
        Beat {
            since: ms(since),
            count,
        }
    }

    #[test]
    fn rest_wears_the_dress_blinks_and_flicks() {
        let dress = amber();
        let rest = State::Rest.pose(ms(0), beat(0, 0), &dress);
        for layer in ["body", "head", "ears", "helmet", "horn"] {
            assert!(has(&rest, layer), "{layer}");
        }
        let blink = State::Rest.pose(ms(2700), beat(0, 0), &dress);
        assert!(has(&blink, "eyes_closed"));
        let flick = State::Rest.pose(ms(4700), beat(0, 0), &dress);
        assert!(has(&flick, "body_flick") && !has(&flick, "body"));
        let again = State::Rest.pose(ms(6600 + 2700), beat(0, 0), &dress);
        assert!(has(&again, "eyes_closed"));
    }

    #[test]
    fn a_season_eye_layer_shows_at_rest_and_a_blink_closes_it() {
        let dress = looks::dress(Costume::Amber, Season::on(2, 2), Duration::ZERO);
        let rest = State::Rest.pose(ms(0), beat(0, 0), &dress);
        assert!(has(&rest, "eyes_left"));
        let blink = State::Rest.pose(ms(2700), beat(0, 0), &dress);
        assert!(has(&blink, "eyes_closed") && !has(&blink, "eyes_left"));
    }

    #[test]
    fn the_messenger_runs_off_and_comes_back_with_the_scroll() {
        let dress = amber();
        let t = Trick::Messenger;
        assert_eq!(t.duration(), ms(33 * 30 + 400 + 12 * 180));
        assert!(t.pose(ms(33), &dress).flip);
        let back = t.pose(ms(33 * 15 + 400 + 33 * 7), &dress);
        assert!(!back.flip && has(&back, "scroll1"));
        let gone = t.pose(ms(33 * 15 + 100), &dress);
        assert!(gone.parts.is_empty());
        let reading = t.pose(ms(33 * 30 + 400 + 190), &dress);
        assert!(has(&reading, "scroll2"));
    }

    #[test]
    fn the_climb_drops_the_carry_and_lifts_the_head_in_the_middle() {
        let dress = amber();
        for at in [0, 300, 700] {
            let pose = Trick::Climb.pose(ms(at), &dress);
            assert!(!has(&pose, "horn"), "{at}");
            assert!(has(&pose, "arms_up"), "{at}");
        }
        let mid = Trick::Climb.pose(ms(300), &dress);
        assert_eq!(part(&mid, "head").dy, -1);
        assert_eq!(part(&mid, "helmet").dy, -1);
        assert_eq!(part(&mid, "body").dy, 0);
        assert_eq!(part(&Trick::Climb.pose(ms(0), &dress), "head").dy, 0);
    }

    #[test]
    fn the_hop_lifts_the_whole_pose_for_its_first_140_ms() {
        let dress = amber();
        assert_eq!(Trick::Hop.pose(ms(0), &dress).dy, -2);
        assert_eq!(Trick::Hop.pose(ms(139), &dress).dy, -2);
        assert_eq!(Trick::Hop.pose(ms(140), &dress).dy, 0);
        assert_eq!(Trick::Hop.pose(ms(260), &dress).dy, -2);
        assert_eq!(Trick::Hop.duration(), ms(700));
    }

    #[test]
    fn a_one_shot_starts_reacting_at_zero_and_ends_on_its_last_frame() {
        let dress = amber();
        assert!(has(&Trick::Slide.pose(ms(0), &dress), "eyes_down"));
        assert!(has(&Trick::BigSwing.pose(ms(0), &dress), "mouth_open"));
        assert!(has(&Trick::Wake.pose(ms(0), &dress), "exclaim"));
        assert!(has(&Trick::Palette.pose(ms(0), &dress), "palette1"));
        assert!(has(&Trick::Stretch.pose(ms(0), &dress), "arms_up"));
        assert!(has(&Trick::Hop.pose(ms(0), &dress), "eyes_happy"));
        let swing = Trick::BigSwing.pose(ms(600), &dress);
        assert_eq!(part(&swing, "helmet").dx, 1);
        assert_eq!(part(&swing, "helmet").dy, -2);
        let past = Trick::Wake.pose(ms(60_000), &dress);
        assert_eq!(past, Trick::Wake.pose(ms(660 - 1), &dress));
    }

    #[test]
    fn the_big_swing_blows_back_whatever_hat_it_wears() {
        let dress = looks::dress(Costume::Amber, Season::on(7, 14), Duration::ZERO);
        let pose = Trick::BigSwing.pose(ms(0), &dress);
        assert_eq!((part(&pose, "beret").dx, part(&pose, "beret").dy), (1, -1));
    }

    #[test]
    fn the_arrival_rows_in_then_hops_and_the_departure_hops_then_rows_out() {
        let dress = amber();
        let arrive = Trick::Arrive.pose(ms(0), &dress);
        assert!(arrive.wide && has(&arrive, "ship") && arrive.dx == -28);
        let end = Trick::Arrive.duration();
        assert_eq!(end, ms(99 * 15 + 700));
        assert!(has(&Trick::Arrive.pose(ms(99 * 15), &dress), "eyes_happy"));
        assert_eq!(Trick::Arrive.pose(ms(99 * 14), &dress).dx, 0);
        let depart = Trick::Depart.pose(ms(0), &dress);
        assert!(depart.wide && has(&depart, "eyes_happy"));
        assert_eq!(Trick::Depart.pose(ms(700 + 99 * 14), &dress).dx, -28);
        let out = Trick::RowOut.pose(ms(0), &dress);
        assert!(!out.wide && has(&out, "ship") && out.dx == 0);
        assert_eq!(Trick::RowIn.pose(ms(0), &dress).dx, 28);
    }

    #[test]
    fn rowing_advances_a_pixel_per_250_ms_over_a_55_px_loop() {
        let dress = amber();
        let at = |t: u64| State::Rowing.pose(ms(t), beat(0, 0), &dress);
        assert_eq!((at(0).dx - at(250).dx).abs(), 1);
        assert_eq!(at(0).dx, at(55 * 250).dx);
        assert_ne!(at(0).dx, at(54 * 250).dx);
        assert!(!has(&at(0), "helmet") && has(&at(0), "ship_head"));
        assert!(!has(&at(0), "horn") && !has(&at(0), "body"));
    }

    #[test]
    fn the_other_states_hold_their_looks() {
        let dress = amber();
        let s = |state: State, t: u64| state.pose(ms(t), beat(0, 0), &dress);
        assert!(has(&s(State::Muted, 0), "paws_ears") && has(&s(State::Muted, 0), "zz_small"));
        assert!(!has(&s(State::Muted, 1800), "zz_small"));
        assert!(has(&s(State::Curled, 0), "curl") && !has(&s(State::Curled, 0), "helmet"));
        assert!(has(&s(State::Lantern, 0), "lantern1") && !has(&s(State::Lantern, 0), "horn"));
        assert!(has(&s(State::Lantern, 400), "lantern2"));
        assert!(has(&s(State::Pointing, 0), "exclaim") && has(&s(State::Pointing, 0), "point"));
        assert!(!has(&s(State::Pointing, 600), "exclaim") && !has(&s(State::Pointing, 0), "horn"));
    }

    #[test]
    fn a_headbang_nods_two_pixels_down_on_the_beat() {
        let dress = amber();
        let state = State::Music(Dance::Headbang);
        let hit = state.pose(ms(0), beat(100, 3), &dress);
        assert_eq!(part(&hit, "head").dy, 2);
        let up = state.pose(ms(0), beat(250, 3), &dress);
        assert_eq!(part(&up, "head").dy, 0);
        assert!(has(&up, "waves") && !has(&hit, "waves"));
    }

    #[test]
    fn a_bob_lowers_the_head_and_the_horn_with_it_on_the_beat() {
        let dress = amber();
        let state = State::Music(Dance::Bob);
        let hit = state.pose(ms(0), beat(100, 3), &dress);
        assert_eq!(part(&hit, "head").dy, 1);
        assert_eq!(part(&hit, "horn").dy, 1);
        let up = state.pose(ms(0), beat(300, 3), &dress);
        assert_eq!(part(&up, "head").dy, 0);
        assert_eq!(part(&up, "horn").dy, 0);
    }

    #[test]
    fn every_dance_hits_for_at_least_a_quarter_second() {
        let dress = amber();
        for dance in [
            Dance::Bob,
            Dance::Headbang,
            Dance::Bounce,
            Dance::Sway,
            Dance::Nod,
        ] {
            let state = State::Music(dance);
            let still = state.pose(ms(0), beat(499, 2), &dress);
            for since in (0..250).step_by(10) {
                assert_ne!(
                    state.pose(ms(0), beat(since, 2), &dress),
                    still,
                    "{dance:?} at {since} ms"
                );
            }
        }
    }

    #[test]
    fn a_sway_goes_left_on_odd_beats_and_right_on_even_ones() {
        let dress = amber();
        let state = State::Music(Dance::Sway);
        let odd = state.pose(ms(0), beat(0, 3), &dress);
        let even = state.pose(ms(0), beat(0, 4), &dress);
        assert_eq!(part(&odd, "head").dx, -1);
        assert_eq!(part(&even, "head").dx, 1);
    }

    #[test]
    fn music_wears_the_headphones_in_place_of_the_hat_and_keeps_the_carry() {
        let dress = amber();
        for dance in [
            Dance::Bob,
            Dance::Headbang,
            Dance::Bounce,
            Dance::Sway,
            Dance::Nod,
        ] {
            for since in [0, 200, 450] {
                let pose = State::Music(dance).pose(ms(0), beat(since, 1), &dress);
                assert!(has(&pose, "phones") && !has(&pose, "helmet") && has(&pose, "horn"));
            }
        }
    }

    #[test]
    fn the_horn_waves_show_only_with_the_horn() {
        let bastille = looks::dress(Costume::Amber, Season::on(7, 14), Duration::ZERO);
        let towel = looks::dress(Costume::Amber, Season::on(5, 25), Duration::ZERO);
        let state = State::Music(Dance::Nod);
        assert!(!has(&state.pose(ms(0), beat(400, 1), &towel), "waves"));
        assert!(!has(&state.pose(ms(0), beat(400, 1), &bastille), "waves"));
        assert!(has(&state.pose(ms(0), beat(400, 1), &amber()), "waves"));
    }

    #[test]
    fn no_pose_names_a_layer_the_sheet_lacks_and_every_pose_composes() {
        let names: Vec<&str> = sheet::names().collect();
        let mut dresses = vec![amber()];
        for month in 1..=12u8 {
            for day in 1..=31u8 {
                if let Some(season) = Season::on(month, day) {
                    dresses.push(looks::dress(Costume::Ice, Some(season), ms(300)));
                }
            }
        }
        let check = |pose: &Pose| {
            for p in &pose.parts {
                assert!(names.contains(&p.layer), "{} is not on the sheet", p.layer);
            }
            let rows = if pose.wide {
                pose.pixels_wide(80)
            } else {
                pose.pixels().iter().map(|r| r.to_vec()).collect()
            };
            assert_eq!(rows.len(), BOX_H);
        };
        for dress in &dresses {
            for trick in TRICKS {
                let end = trick.duration().as_millis() as u64;
                for t in (0..=end).step_by(33) {
                    check(&trick.pose(ms(t), dress));
                }
            }
            for dance in [
                Dance::Bob,
                Dance::Headbang,
                Dance::Bounce,
                Dance::Sway,
                Dance::Nod,
            ] {
                let states = [
                    State::Rest,
                    State::Music(dance),
                    State::Muted,
                    State::Curled,
                    State::Lantern,
                    State::Rowing,
                    State::Pointing,
                ];
                for state in states {
                    for t in (0..10_000).step_by(33) {
                        check(&state.pose(ms(t), beat(t % 500, t / 500), dress));
                    }
                }
            }
        }
    }

    #[test]
    fn the_durations_are_the_ported_timelines() {
        let want = [
            (Trick::Climb, 900),
            (Trick::Slide, 700),
            (Trick::BigSwing, 800),
            (Trick::Wake, 660),
            (Trick::Messenger, 3550),
            (Trick::Palette, 2880),
            (Trick::Hop, 700),
            (Trick::Stretch, 1650),
            (Trick::Arrive, 2185),
            (Trick::Depart, 2185),
            (Trick::RowOut, 1485),
            (Trick::RowIn, 2185),
        ];
        for (trick, millis) in want {
            assert_eq!(trick.duration(), ms(millis), "{trick:?}");
        }
    }

    #[test]
    fn face_wear_follows_the_head_and_body_wear_stays() {
        let dress = looks::dress(Costume::Ice, Season::on(8, 29), Duration::ZERO);
        assert!(dress.wear.contains(&"shades") && dress.wear.contains(&"scarf"));
        let offset = |pose: &Pose, layer: &str| (part(pose, layer).dx, part(pose, layer).dy);
        let stretch = Trick::Stretch.pose(ms(300), &dress);
        assert_eq!(offset(&stretch, "head"), (0, -2));
        assert_eq!(offset(&stretch, "shades"), (0, -2));
        assert_eq!(offset(&stretch, "scarf"), (0, 0));
        let sway = State::Music(Dance::Sway).pose(ms(0), beat(0, 1), &dress);
        assert_eq!(offset(&sway, "head"), (-1, 0));
        assert_eq!(offset(&sway, "shades"), (-1, 0));
        assert_eq!(offset(&sway, "scarf"), (0, 0));
        let nod = State::Music(Dance::Headbang).pose(ms(0), beat(0, 1), &dress);
        assert_eq!(offset(&nod, "shades"), (0, 2));
    }

    #[test]
    fn every_frame_of_face_wear_follows_the_head() {
        for (month, day, layer) in [(8, 29, "shades2"), (9, 19, "eyepatch2")] {
            let dress = looks::dress(Costume::Amber, Season::on(month, day), ms(1500));
            assert!(dress.wear.contains(&layer));
            let stretch = Trick::Stretch.pose(ms(300), &dress);
            assert_eq!(
                (part(&stretch, layer).dx, part(&stretch, layer).dy),
                (0, -2)
            );
        }
    }

    #[test]
    fn the_arrival_ship_is_mirrored_and_the_others_are_not() {
        let dress = amber();
        for t in (0..99 * 15).step_by(33) {
            assert!(Trick::Arrive.pose(ms(t), &dress).flip, "{t}");
        }
        assert!(!Trick::Arrive.pose(ms(99 * 15), &dress).flip);
        for trick in [Trick::Depart, Trick::RowOut, Trick::RowIn] {
            for t in (0..trick.duration().as_millis() as u64).step_by(33) {
                assert!(!trick.pose(ms(t), &dress).flip, "{trick:?} {t}");
            }
        }
    }

    #[test]
    fn the_arrival_prow_points_right_where_the_sheet_has_it_left() {
        let dress = amber();
        let mirrored = Trick::Arrive.pose(ms(99 * 7), &dress);
        assert!(mirrored.flip);
        let mirrored = super::super::tricks::Pose { dx: 0, ..mirrored };
        let plain = super::super::tricks::Pose {
            flip: false,
            ..mirrored.clone()
        };
        let edge = |pose: &super::super::tricks::Pose, leftmost: bool| {
            let columns: Vec<usize> = pose
                .pixels()
                .iter()
                .flat_map(|row| row.iter().enumerate())
                .filter(|(_, px)| px.is_some())
                .map(|(x, _)| x)
                .collect();
            if leftmost {
                columns.iter().copied().min()
            } else {
                columns.iter().copied().max()
            }
            .expect("the ship has pixels in the box")
        };
        assert_eq!(edge(&mirrored, false), BOX_W - 1 - edge(&plain, true));
    }

    #[test]
    fn pixels_wide_puts_the_box_at_the_right_and_lets_the_ship_enter_from_the_left() {
        let dress = amber();
        let home = State::Rest.pose(ms(0), beat(0, 0), &dress).pixels_wide(80);
        let drawn = |grid: &[Vec<Option<char>>], cols: std::ops::Range<usize>| {
            grid.iter()
                .flat_map(|r| &r[cols.clone()])
                .any(|c| c.is_some())
        };
        assert!(drawn(&home, 80 - BOX_W..80));
        assert!(!drawn(&home, 0..80 - BOX_W));
        let arriving = Trick::Arrive.pose(ms(0), &dress).pixels_wide(80);
        assert!(drawn(&arriving, 0..80 - BOX_W));
        let landed = Trick::Hop.pose(ms(0), &dress).pixels_wide(80);
        assert!(!drawn(&landed, 0..80 - BOX_W));
    }
}
