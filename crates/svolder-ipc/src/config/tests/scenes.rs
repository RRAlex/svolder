// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Scenes: the `[[scene]]` blocks, their writers, and which scene is on.

use super::*;
use crate::config::scenes::{DeviceNow, SceneState, on};
use svolder_host::eq::{ComputerPart, CrossfeedSettings, LoudnessSettings};
use svolder_profiles::scene::DevicePart;
use svolder_profiles::scene::held_controls;
use svolder_profiles::tone::apo::{FilterKind, SourceFilter};

const BIFROST: &str = "0a0b0c0d0e0f1011";
const KARA: &str = "1112131415161718";

fn computer() -> ComputerPart {
    ComputerPart {
        eq_on: true,
        name: "Warm cans".into(),
        origin: Some("by hand".into()),
        preamp_db: -4.5,
        filters: vec![
            SourceFilter {
                kind: FilterKind::Peaking,
                f: 100.0,
                g: 3.0,
                q: 1.0,
            },
            SourceFilter {
                kind: FilterKind::HighShelf,
                f: 8000.0,
                g: -2.0,
                q: 0.7,
            },
        ],
        loudness: Some(LoudnessSettings {
            on: true,
            strength_pct: 35.0,
            reference_pct: 90.0,
        }),
        crossfeed_on: true,
        crossfeed: Some(CrossfeedSettings::default()),
        speakers_on: false,
        speakers: Some("room-a".into()),
    }
}

fn bifrost() -> DevicePart {
    DevicePart {
        udid: BIFROST.into(),
        name: "Bifrost 3".into(),
        tone: Some(Preset {
            name: "Warm".into(),
            bands: 3,
            mode: EqMode::Eq,
            loudness: 12,
            band: vec![
                PresetBand {
                    f: 63.0,
                    g: 1.0,
                    q: 0.7,
                },
                PresetBand {
                    f: 630.0,
                    g: 0.0,
                    q: 4.3,
                },
                PresetBand {
                    f: 6300.0,
                    g: -1.5,
                    q: 4.3,
                },
            ],
            source: None,
        }),
        raw_bands: None,
        controls: vec![("input".into(), 1), ("phase".into(), 0), ("nos".into(), 2)],
    }
}

fn loki() -> DevicePart {
    DevicePart {
        udid: KARA.into(),
        name: "Kara".into(),
        tone: None,
        raw_bands: Some(vec![("bass".into(), 5), ("treble".into(), 7)]),
        controls: vec![("input".into(), 0)],
    }
}

fn scene(name: &str) -> Scene {
    Scene {
        name: name.into(),
        computer: Some(computer()),
        devices: vec![bifrost(), loki()],
    }
}

fn file_with(text: &str) -> (crate::test_support::TempPath, std::path::PathBuf) {
    let dir = temp_dir("config-scenes");
    let path = dir.join("config.toml");
    std::fs::write(&path, text).unwrap();
    (dir, path)
}

/// A scene with the computer and two devices survives a write and a load, field for field, and
/// the text reads as a hand would write it.
#[test]
fn a_scene_round_trips_and_reads_plainly() {
    let (_dir, path) = file_with(&Config::template());
    Config::add_scene(&path, &scene("Late night")).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("[[scene]]\nname = \"Late night\"\n"),
        "{text}"
    );
    assert!(text.contains("[scene.computer]\neq_on = true\n"), "{text}");
    assert!(
        text.contains(&format!("[[scene.device]]\nudid = \"{BIFROST}\"\n")),
        "{text}"
    );
    assert!(
        text.contains("[scene.device.tone]\nname = \"Warm\"\n"),
        "{text}"
    );
    assert!(
        text.contains("{ type = \"PK\", f = 100.0, g = 3.0, q = 1.0 },"),
        "{text}"
    );
    assert!(text.contains("{ key = \"input\", step = 1 },"), "{text}");
    let config = Config::load_from(&path).unwrap();
    assert_eq!(config.scenes, vec![scene("Late night")]);
    assert_eq!(config.scene(" LATE night ").unwrap().name, "Late night");
}

/// A file with no scene loads with none, and a scene with only a name holds nothing.
#[test]
fn a_file_without_scenes_loads_and_a_bare_scene_holds_nothing() {
    assert!(Config::parse("").unwrap().scenes.is_empty());
    let config = Config::parse("[[scene]]\nname = \"Empty\"\n").unwrap();
    assert_eq!(
        config.scenes,
        vec![Scene {
            name: "Empty".into(),
            computer: None,
            devices: Vec::new()
        }]
    );
}

fn parse_err(text: &str) -> String {
    match Config::parse(text).unwrap_err() {
        ConfigError::Parse(words) => words,
        other => panic!("{other:?}"),
    }
}

/// A malformed block names the scene, and for a part, which part.
#[test]
fn a_malformed_block_names_itself() {
    let base = |extra: &str| format!("[[scene]]\nname = \"Desk\"\n{extra}");
    let computer = "[scene.computer]\neq_on = true\nname = \"x\"\npreamp = 0\n";
    let device = "[[scene.device]]\nudid = \"0102030405060708\"\nname = \"Amp\"\n";
    for (extra, words) in [
        ("sparkle = 1\n".to_string(), "unknown key \"sparkle\""),
        (
            "[scene.computer]\neq_on = 1\n".to_string(),
            "eq_on must be true or false",
        ),
        (
            format!("{computer}filter = []\nwat = 1\n"),
            "computer: unknown key \"wat\"",
        ),
        (
            format!("{computer}filter = [{{ type = \"ZZ\", f = 1, g = 1, q = 1 }}]\n"),
            "computer filter 1: type must be PK, LSC or HSC",
        ),
        (
            format!("{computer}filter = []\nloudness = {{ on = true, sparkle = 1 }}\n"),
            "computer loudness: unknown key \"sparkle\"",
        ),
        (
            "[[scene.device]]\nudid = \"nope\"\n".to_string(),
            "device 1: bad udid \"nope\"",
        ),
        (
            format!(
                "{device}[scene.device.tone]\nname = \"x\"\nmode = \"eq\"\nband = [{{ f = 1, g = 1 }}]\n"
            ),
            "device 1 (Amp): tone: band 1: q is required",
        ),
        (
            format!("{device}tone = 3\n"),
            "device 1 (Amp): tone must be a table",
        ),
        ("computer = 3\n".to_string(), "computer must be a table"),
    ] {
        let got = parse_err(&base(&extra));
        assert!(
            got.contains("scene \"Desk\"") && got.contains(words),
            "{got}"
        );
    }
}

/// A key listed twice in one device part, in `controls`, or once there and once in `raw_bands`,
/// is refused with the scene, the device and the key named.
#[test]
fn a_key_listed_twice_in_a_device_part_is_refused() {
    let part = |body: &str| {
        format!(
            "[[scene]]\nname = \"Desk\"\n[[scene.device]]\nudid = \"0102030405060708\"\n\
             name = \"Amp\"\n{body}"
        )
    };
    for body in [
        "controls = [{ key = \"input\", step = 1 }, { key = \"input\", step = 2 }]\n",
        "raw_bands = [{ key = \"bass\", step = 1 }]\ncontrols = [{ key = \"bass\", step = 2 }]\n",
        "controls = [{ key = \"input\", step = 1 }, { key = \"Input\", step = 2 }]\n",
    ] {
        let got = parse_err(&part(body));
        assert!(
            got.contains("scene \"Desk\"")
                && got.contains("device 1 (Amp)")
                && got.contains("is listed twice"),
            "{got}"
        );
    }
    assert!(got_key(&parse_err(&part(
        "controls = [{ key = \"input\", step = 1 }, { key = \"input\", step = 2 }]\n"
    ))));
    assert!(Config::parse(&part("controls = [{ key = \"input\", step = 1 }]\n")).is_ok());
}

fn got_key(words: &str) -> bool {
    words.contains("\"input\"")
}

/// A unit twice in one scene, two scenes of one name and the name `custom` are refused.
#[test]
fn twice_and_reserved_are_refused() {
    let one = "[[scene.device]]\nudid = \"0102030405060708\"\n";
    let got = parse_err(&format!("[[scene]]\nname = \"Desk\"\n{one}{one}"));
    assert!(
        got.contains("scene \"Desk\"") && got.contains("twice"),
        "{got}"
    );
    let err =
        Config::parse("[[scene]]\nname = \"Desk\"\n[[scene]]\nname = \" desk\"\n").unwrap_err();
    assert!(
        matches!(err, ConfigError::Taken { kind: "scene", .. }),
        "{err:?}"
    );
    let got = parse_err("[[scene]]\nname = \"Custom\"\n");
    assert!(got.contains("scene \"Custom\""), "{got}");
    assert!(parse_err("[[scene]]\nname = \"  \"\n").contains("needs a name"));
}

const SCENES_FILE: &str = "# my config\nadapter = \"hci1\" # dongle\n\n\
    # the desk one\n[[scene]]\nname = \"Desk\" # keep\n\n[[device]]\nname = \"amp\"\n\
    udid = \"0102030405060708\"\n";

/// Add, update, rename and remove keep the comments and the other blocks byte for byte.
#[test]
fn the_writers_keep_comments_and_other_blocks() {
    let (_dir, path) = file_with(SCENES_FILE);
    let head = SCENES_FILE.split("\n\n[[device]]").next().unwrap();
    let device = &SCENES_FILE[SCENES_FILE.find("[[device]]").unwrap()..];

    Config::add_scene(&path, &scene("Late night")).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.starts_with(head), "{text}");
    assert!(text.ends_with(device) || text.contains(device), "{text}");
    assert_eq!(Config::load_from(&path).unwrap().scenes.len(), 2);

    let mut changed = scene("Late night");
    changed.computer = None;
    Config::update_scene(&path, " LATE night", &changed).unwrap();
    let config = Config::load_from(&path).unwrap();
    assert_eq!(config.scenes[1], changed);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("# the desk one\n[[scene]]\nname = \"Desk\" # keep\n"),
        "{text}"
    );

    Config::rename_scene(&path, "late night", "Den").unwrap();
    assert_eq!(Config::load_from(&path).unwrap().scenes[1].name, "Den");
    assert_eq!(
        Config::load_from(&path).unwrap().scenes[1].devices,
        changed.devices
    );
    Config::rename_scene(&path, "den", "DEN").unwrap();

    Config::remove_scene(&path, "den").unwrap();
    Config::remove_scene(&path, "Desk").unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(Config::parse(&text).unwrap().scenes.is_empty());
    assert!(text.contains("adapter = \"hci1\" # dongle"), "{text}");
    assert!(text.contains("[[device]]\nname = \"amp\"\n"), "{text}");
    assert!(!text.contains("the desk one"), "{text}");
}

/// A taken, a bad, the reserved and a missing name are refused with their words; a preset's name
/// and a built-in's are no refusal for a scene.
#[test]
fn bad_names_are_refused_and_a_presets_is_not() {
    let (_dir, path) = file_with(&format!("{SCENES_FILE}{}", three_band_block("Mine")));
    let named = |name: &str| Scene {
        name: name.into(),
        ..scene("x")
    };
    let err = Config::add_scene(&path, &named("desk ")).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Taken { kind: "scene", name } if name == "Desk"),
        "{err:?}"
    );
    assert_eq!(err.reason(), crate::socket::protocol::Reason::Taken);
    for bad in [
        "",
        "   ",
        "custom",
        "CUSTOM",
        &"x".repeat(NAME_MAX + 1),
        "a\nb",
    ] {
        let err = Config::add_scene(&path, &named(bad)).unwrap_err();
        assert_eq!(
            err.reason(),
            crate::socket::protocol::Reason::BadName,
            "{bad:?}: {err:?}"
        );
        assert!(!err.to_string().contains("preset"), "{err}");
    }
    let err = Config::rename_scene(&path, "Desk", "custom").unwrap_err();
    assert_eq!(err.reason(), crate::socket::protocol::Reason::BadName);
    assert!(!err.to_string().contains("preset"), "{err}");
    assert!(matches!(
        Config::rename_scene(&path, "nobody", "x"),
        Err(ConfigError::Missing { kind: "scene", .. })
    ));
    assert!(matches!(
        Config::update_scene(&path, "nobody", &named("x")),
        Err(ConfigError::Missing { kind: "scene", .. })
    ));
    assert!(matches!(
        Config::remove_scene(&path, "nobody"),
        Err(ConfigError::Missing { kind: "scene", .. })
    ));
    Config::add_scene(&path, &named("Mine")).unwrap();
    Config::add_scene(&path, &named("Warm")).unwrap();
    Config::rename_scene(&path, "Desk", "DESK").unwrap();
    let config = Config::load_from(&path).unwrap();
    assert_eq!(config.presets.len(), 1, "the preset is untouched");
    assert_eq!(
        config
            .scenes
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["DESK", "Mine", "Warm"]
    );
}

/// A scene may take a built-in preset's name, once: the taken check still runs for it, and the
/// answer names the spelling the file has.
#[test]
fn a_built_in_name_is_allowed_once_and_refused_when_taken() {
    let (_dir, path) = file_with(&Config::template());
    let named = |name: &str| Scene {
        name: name.into(),
        ..scene("x")
    };
    Config::add_scene(&path, &named("Warm")).unwrap();
    let err = Config::add_scene(&path, &named("warm")).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Taken { kind: "scene", name } if name == "Warm"),
        "{err:?}"
    );
    Config::add_scene(&path, &named("Bass")).unwrap();
    Config::add_scene(&path, &named("Desk")).unwrap();
    let err = Config::rename_scene(&path, "Desk", "BASS").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Taken { kind: "scene", name } if name == "Bass"),
        "{err:?}"
    );
    let err = Config::update_scene(&path, "Desk", &named("bass")).unwrap_err();
    assert_eq!(err.reason(), crate::socket::protocol::Reason::Taken);
    Config::rename_scene(&path, "Desk", "desk").unwrap();
    assert_eq!(Config::load_from(&path).unwrap().scenes.len(), 3);
}

/// The words of every writer refusal say scene, and the long one says how long.
#[test]
fn the_refusal_words_are_a_scenes() {
    let (_dir, path) = file_with(&Config::template());
    let named = |name: &str| Scene {
        name: name.into(),
        ..scene("x")
    };
    let err = Config::add_scene(&path, &named("custom")).unwrap_err();
    assert_eq!(
        err.to_string(),
        "config: \"custom\" stands for no scene at all, so no scene can take it"
    );
    let err = Config::add_scene(&path, &named(&"x".repeat(NAME_MAX + 1))).unwrap_err();
    assert_eq!(
        err.to_string(),
        format!(
            "config: a scene's name is at most {NAME_MAX} characters, not {}",
            NAME_MAX + 1
        )
    );
    let err = Config::add_scene(&path, &named("  ")).unwrap_err();
    assert_eq!(err.to_string(), "config: a scene needs a name");
}

/// Every field of a scene survives a write and a load, the empty ones and the unusual ones too.
#[test]
fn every_field_round_trips() {
    let (_dir, path) = file_with(&Config::template());
    let mut s = scene("full");
    let c = s.computer.as_mut().unwrap();
    c.eq_on = false;
    c.origin = None;
    c.loudness = None;
    c.speakers = None;
    c.speakers_on = true;
    let xf = CrossfeedSettings {
        preset: None,
        edited_from: Some("bs2b default".into()),
        delay_ms: Some(0.45),
        amount: 0.3,
        ..CrossfeedSettings::default()
    };
    c.crossfeed = Some(xf);
    s.devices[0].tone.as_mut().unwrap().mode = EqMode::Loudness;
    s.devices[0].tone.as_mut().unwrap().loudness = 127;
    s.devices[1].controls.clear();
    Config::add_scene(&path, &s).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(Config::load_from(&path).unwrap().scenes, vec![s], "{text}");
    assert!(text.contains("edited_from = \"bs2b default\""), "{text}");
}

/// The comments on the block being renamed or updated stay with it.
#[test]
fn the_edited_blocks_own_comments_survive() {
    let kept = "# the desk one\n[[scene]]\nname = \"";
    let (_dir, path) = file_with(SCENES_FILE);
    Config::rename_scene(&path, "Desk", "DESK").unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(&format!("{kept}DESK\" # keep\n")), "{text}");
    Config::update_scene(&path, "desk", &scene("Desk")).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(&format!("{kept}Desk\" # keep\n")), "{text}");
    assert_eq!(
        Config::load_from(&path).unwrap().scenes,
        vec![scene("Desk")]
    );
}

/// Two writers adding different scenes at once both keep theirs.
#[test]
fn two_writers_adding_scenes_at_once_both_keep_theirs() {
    let (_dir, path) = file_with(&Config::template());
    super::writers::meet_at(&path);
    std::thread::scope(|scope| {
        for name in ["one", "two"] {
            let path = &path;
            scope.spawn(move || Config::add_scene(path, &scene(name)).expect("a scene added"));
        }
    });
    let mut names: Vec<_> = Config::load_from(&path)
        .unwrap()
        .scenes
        .into_iter()
        .map(|s| s.name)
        .collect();
    names.sort();
    assert_eq!(names, ["one", "two"]);
}

/// Real profiles with real readings, and the scenes captured from them: `on` judges with
/// `scene::matches` and `ComputerPart::matches`, never with a stand-in.
mod judging {
    use std::collections::HashMap;

    use super::*;
    use svolder_host::eq::{EqView, PointView};
    use svolder_profiles::tone::preset::{Write, builtins, plan_recall};
    use svolder_profiles::{ControlKind, Profile, Registry};
    use svolder_proto::FunctionId;

    type Readings = HashMap<FunctionId, i64>;

    fn profile(name: &str) -> Profile {
        Registry::builtin()
            .unwrap()
            .get(name)
            .unwrap_or_else(|| panic!("{name} is a built-in profile"))
            .clone()
    }

    /// Readings for every id a scene reads: held controls on their first value, the tone
    /// control flat, the built-in bands at 15.
    fn readings(profile: &Profile) -> Readings {
        let mut map = HashMap::new();
        for control in held_controls(profile) {
            let raw = match control.kind {
                ControlKind::Select => i64::from(control.values[0].0),
                ControlKind::Level => control.range.expect("a level has a range").0,
                _ => 0,
            };
            map.insert(control.id, raw);
        }
        if let Some(eq) = &profile.eq {
            let flat = &builtins(eq)[0];
            for write in plan_recall(eq, |_| None, flat).unwrap().writes {
                let raw = match write {
                    Write::Band { raw, .. } => i64::from(raw),
                    Write::Mode(mode) => i64::from(mode as u8),
                    Write::Loudness(raw) => i64::from(raw),
                };
                map.insert(write.id(eq).unwrap(), raw);
            }
            map.insert(eq.loudness_id(), 0);
        }
        if let Some(table) = &profile.builtin_settings {
            for key in &table.keys {
                map.insert(profile.control_by_key(key).unwrap().id, 15);
            }
        }
        map
    }

    /// The first held control's id and a value other than the one `readings` gives it.
    fn other(profile: &Profile) -> (FunctionId, i64) {
        let control = held_controls(profile)[0];
        let now = readings(profile)[&control.id];
        let moved = match control.kind {
            ControlKind::Select => control
                .values
                .iter()
                .map(|(raw, _)| i64::from(*raw))
                .find(|raw| *raw != now),
            ControlKind::Level => control.range.map(|(low, _)| low + 1),
            _ => Some(1 - now),
        };
        (control.id, moved.expect("a held control has another value"))
    }

    fn part(profile: &Profile, udid: &str, map: &Readings) -> DevicePart {
        let fitting = profile.eq.as_ref().map(builtins).unwrap_or_default();
        svolder_profiles::scene::capture(profile, udid, "Dev", &fitting, None, |id| {
            map.get(&id).copied()
        })
        .expect("every reading is there")
    }

    /// The part for `udid` with its first held control on the other value.
    fn moved_part(profile: &Profile, udid: &str) -> DevicePart {
        let mut moved = readings(profile);
        let (id, to) = other(profile);
        moved.insert(id, to);
        part(profile, udid, &moved)
    }

    /// The computer part every scene here holds, and the view that plays it.
    fn plain_computer() -> ComputerPart {
        ComputerPart {
            eq_on: true,
            name: "Warm cans".into(),
            origin: None,
            preamp_db: -4.5,
            filters: vec![SourceFilter {
                kind: FilterKind::Peaking,
                f: 100.0,
                g: 3.0,
                q: 1.0,
            }],
            loudness: None,
            crossfeed_on: false,
            crossfeed: None,
            speakers_on: false,
            speakers: None,
        }
    }

    fn playing() -> EqView {
        EqView {
            on: true,
            stated_preamp_db: -4.5,
            points: vec![PointView {
                slot: 1,
                kind: "PK".into(),
                f: 100.0,
                g: 3.0,
                q: 1.0,
            }],
            ..EqView::default()
        }
    }

    fn quieter() -> EqView {
        EqView {
            stated_preamp_db: -6.0,
            ..playing()
        }
    }

    /// What a device reads against the readings its scenes were captured from.
    #[derive(Clone, Copy)]
    enum Read {
        Same,
        Moved,
        Unread,
    }

    struct World {
        /// `None`: the computer's controls are not there.
        computer: Option<EqView>,
        /// `None`: not connected.
        bifrost: Option<Read>,
        loki: Option<Read>,
    }

    fn device_scene(name: &str, parts: Vec<DevicePart>) -> Scene {
        Scene {
            name: name.into(),
            computer: None,
            devices: parts,
        }
    }

    fn which(scenes: &[Scene], world: &World, recalled: Option<&str>) -> (Option<String>, bool) {
        let (bifrost, loki) = (profile("bifrost3"), profile("lokimax"));
        let (mut b_map, mut l_map) = (readings(&bifrost), readings(&loki));
        for (map, profile, read) in [
            (&mut b_map, &bifrost, world.bifrost),
            (&mut l_map, &loki, world.loki),
        ] {
            let (id, to) = other(profile);
            match read {
                Some(Read::Moved) => {
                    map.insert(id, to);
                }
                Some(Read::Unread) => {
                    map.remove(&id);
                }
                _ => {}
            }
        }
        let (b_raw, l_raw) = (|id| b_map.get(&id).copied(), |id| l_map.get(&id).copied());
        let mut devices = Vec::new();
        if world.bifrost.is_some() {
            devices.push(DeviceNow {
                udid: BIFROST,
                profile: &bifrost,
                raw: &b_raw,
            });
        }
        if world.loki.is_some() {
            devices.push(DeviceNow {
                udid: KARA,
                profile: &loki,
                raw: &l_raw,
            });
        }
        let state = SceneState {
            devices: &devices,
            computer: world.computer.as_ref(),
        };
        let got = on(scenes, &state, recalled);
        (got.name, got.known)
    }

    fn bifrost_part() -> DevicePart {
        let profile = profile("bifrost3");
        part(&profile, BIFROST, &readings(&profile))
    }

    fn loki_part() -> DevicePart {
        let profile = profile("lokimax");
        part(&profile, KARA, &readings(&profile))
    }

    fn full(name: &str) -> Scene {
        Scene {
            name: name.into(),
            computer: Some(plain_computer()),
            devices: vec![bifrost_part(), loki_part()],
        }
    }

    fn all_same() -> World {
        World {
            computer: Some(playing()),
            bifrost: Some(Read::Same),
            loki: Some(Read::Same),
        }
    }

    /// A real device part and a real computer part both judge: a scene of the two is on while both
    /// agree, and off when either moves.
    #[test]
    fn a_real_device_part_and_a_real_computer_part_both_judge() {
        let scenes = [full("a")];
        assert_eq!(which(&scenes, &all_same(), None), (Some("a".into()), true));
        let moved_device = World {
            bifrost: Some(Read::Moved),
            ..all_same()
        };
        assert_eq!(which(&scenes, &moved_device, None), (None, true));
        let moved_computer = World {
            computer: Some(quieter()),
            ..all_same()
        };
        assert_eq!(
            which(&scenes, &moved_computer, None),
            (None, true),
            "the stated preamp is compared"
        );
    }

    /// The scene last recalled wins a tie while it still matches; else the first that matches;
    /// none is none and known.
    #[test]
    fn the_recalled_scene_wins_a_tie_else_the_first_that_matches() {
        let scenes = [full("a"), full("b")];
        assert_eq!(which(&scenes, &all_same(), None), (Some("a".into()), true));
        assert_eq!(
            which(&scenes, &all_same(), Some("B ")),
            (Some("b".into()), true)
        );
        assert_eq!(
            which(&scenes, &all_same(), Some("gone")),
            (Some("a".into()), true)
        );
        let none = World {
            computer: Some(quieter()),
            ..all_same()
        };
        assert_eq!(which(&scenes, &none, Some("b")), (None, true));
        assert_eq!(which(&[], &none, None), (None, true));
    }

    /// The recalled scene no longer matches and another does: the first that matches is on.
    #[test]
    fn a_recalled_scene_that_no_longer_matches_gives_way() {
        let profile = profile("bifrost3");
        let scenes = [
            device_scene("a", vec![moved_part(&profile, BIFROST)]),
            device_scene("b", vec![bifrost_part()]),
        ];
        let world = World {
            computer: None,
            bifrost: Some(Read::Same),
            loki: None,
        };
        assert_eq!(which(&scenes, &world, Some("a")), (Some("b".into()), true));
        let both = [
            device_scene("a", vec![bifrost_part()]),
            device_scene("b", vec![bifrost_part()]),
        ];
        assert_eq!(which(&both, &world, Some("b")), (Some("b".into()), true));
        assert_eq!(which(&both, &world, None), (Some("a".into()), true));
    }

    /// A device that is not connected does not count; a scene with nothing that counts is never
    /// on.
    #[test]
    fn absent_devices_are_not_counted_and_an_empty_count_is_no_scene() {
        let scenes = [full("a")];
        let only_bifrost = World {
            loki: None,
            ..all_same()
        };
        assert_eq!(
            which(&scenes, &only_bifrost, None),
            (Some("a".into()), true)
        );
        let wrong = World {
            bifrost: Some(Read::Moved),
            loki: None,
            ..all_same()
        };
        assert_eq!(which(&scenes, &wrong, None), (None, true));
        let nobody = World {
            computer: None,
            bifrost: None,
            loki: None,
        };
        assert_eq!(which(&scenes, &nobody, None), (None, true));
        let device_only = [device_scene("d", vec![bifrost_part()])];
        let world = World {
            computer: None,
            bifrost: Some(Read::Same),
            loki: None,
        };
        assert_eq!(which(&device_only, &world, None), (Some("d".into()), true));
        let lower = [device_scene(
            "lower",
            vec![DevicePart {
                udid: BIFROST.to_ascii_uppercase(),
                ..bifrost_part()
            }],
        )];
        assert_eq!(which(&lower, &world, None), (Some("lower".into()), true));
    }

    /// A computer part while the computer's controls are not there does not count, as an absent
    /// device's does not: the scene is judged by its devices alone.
    #[test]
    fn a_computer_part_with_no_computer_is_not_counted() {
        let scenes = [full("a")];
        let no_computer = World {
            computer: None,
            ..all_same()
        };
        assert_eq!(which(&scenes, &no_computer, None), (Some("a".into()), true));
        let moved = World {
            bifrost: Some(Read::Moved),
            ..no_computer
        };
        assert_eq!(which(&scenes, &moved, None), (None, true));
    }

    /// A part that is unread leaves the answer unknown, and a part that disagrees settles its
    /// scene without it.
    #[test]
    fn an_unread_part_is_not_known_unless_another_part_already_disagrees() {
        let scenes = [full("a")];
        let unread = World {
            bifrost: Some(Read::Unread),
            ..all_same()
        };
        assert_eq!(which(&scenes, &unread, None), (None, false));
        assert_eq!(which(&scenes, &unread, Some("a")), (None, false));
        let settled = World {
            computer: Some(quieter()),
            ..unread
        };
        assert_eq!(which(&scenes, &settled, None), (None, true));
    }

    /// The answer is unknown only when an unread scene could outrank the winner: it comes before
    /// the winner in the order (recalled first, then the file's), or no scene matches.
    #[test]
    fn an_unread_scene_spoils_only_what_it_could_outrank() {
        let bifrost = profile("bifrost3");
        let win = device_scene("win", vec![bifrost_part()]);
        let unread = device_scene("unread", vec![loki_part()]);
        let no = device_scene("no", vec![moved_part(&bifrost, BIFROST)]);
        let world = World {
            computer: None,
            bifrost: Some(Read::Same),
            loki: Some(Read::Unread),
        };
        let after = [win.clone(), unread.clone()];
        assert_eq!(which(&after, &world, None), (Some("win".into()), true));
        let before = [unread.clone(), win];
        assert_eq!(which(&before, &world, None), (None, false));
        assert_eq!(which(&after, &world, Some("unread")), (None, false));
        assert_eq!(
            which(&before, &world, Some("win")),
            (Some("win".into()), true)
        );
        let none = [no, unread];
        assert_eq!(which(&none, &world, None), (None, false));
        assert_eq!(which(&none[..1], &world, None), (None, true));
    }
}

/// A device part that holds a tone and built-in bands both is refused with the scene and the
/// device named; either alone is read.
#[test]
fn a_part_with_a_tone_and_raw_bands_is_refused() {
    let tone = "[scene.device.tone]\nname = \"Warm\"\nmode = \"eq\"\nloudness = 0\n\
                band = [{ f = 63.0, g = 1.0, q = 0.7 }]\n";
    let part = |body: &str| {
        format!(
            "[[scene]]\nname = \"Desk\"\n[[scene.device]]\nudid = \"0102030405060708\"\n\
             name = \"Amp\"\n{body}"
        )
    };
    let raw = "raw_bands = [{ key = \"band1\", step = 1 }]\n";
    let both = parse_err(&part(&format!("{raw}{tone}")));
    assert!(
        both.contains("scene \"Desk\"")
            && both.contains("device 1 (Amp)")
            && both.contains("not both"),
        "{both}"
    );
    assert!(Config::parse(&part(raw)).is_ok());
    assert!(Config::parse(&part(tone)).is_ok());
}

/// A part is named by the device the configuration holds under its unit id (case aside), so a
/// renamed device shows its new name; a part whose device is gone keeps the name it was saved
/// under; the computer comes first.
#[test]
fn parts_are_named_by_the_configured_device_else_by_the_saved_name() {
    let scene = Scene {
        name: "Desk".into(),
        computer: Some(computer()),
        devices: vec![bifrost(), loki()],
    };
    let devices = vec![device("Desk DAC", &BIFROST.to_uppercase())];
    assert_eq!(
        scene.part_names(&devices),
        ["computer", "Desk DAC", "Kara"].map(String::from)
    );
    let bare = Scene {
        computer: None,
        ..scene
    };
    assert_eq!(
        bare.part_names(&[]),
        ["Bifrost 3", "Kara"].map(String::from)
    );
}
