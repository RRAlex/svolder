// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;
use std::sync::Arc;

use svolder_ipc::{CODE_USAGE, Config, Handler, PresetOp, Reason, Request, Response};
use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};
use svolder_profiles::tone::preset::{self, Preset};

use crate::handler::Daemon;
use crate::handler::tests::daemon_over;

/// A scratch folder with a configuration file holding `text`, and a shared daemon reading it.
fn daemon_on(tag: &str, text: &str) -> (PathBuf, PathBuf, Arc<Daemon>) {
    let dir = std::env::temp_dir().join(format!("svolderd-pw-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("profiles")).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(&path, text).unwrap();
    let daemon = daemon_over(path.clone(), dir.join("profiles")).shared();
    (dir, path, daemon)
}

fn imported(name: &str, gain: f64) -> Preset {
    preset::from_computer(
        name,
        Correction {
            preamp_db: -2.0,
            filters: vec![SourceFilter {
                kind: FilterKind::Peaking,
                f: 1000.0,
                g: gain,
                q: 1.0,
            }],
        },
    )
}

fn save(preset: Preset, force: bool) -> Request {
    Request::PresetWrite {
        op: PresetOp::Save {
            preset: Box::new(preset),
            force,
        },
    }
}

fn rename(from: &str, to: &str) -> Request {
    Request::PresetWrite {
        op: PresetOp::Rename {
            from: from.into(),
            to: to.into(),
        },
    }
}

fn delete(name: &str) -> Request {
    Request::PresetWrite {
        op: PresetOp::Delete { name: name.into() },
    }
}

/// The reason and words of a refusal.
fn refusal(answer: Response) -> (Option<Reason>, String) {
    match answer {
        Response::Error {
            code,
            message,
            reason,
        } => {
            assert_eq!(code, CODE_USAGE);
            (reason, message)
        }
        other => panic!("not a refusal: {other:?}"),
    }
}

/// Save, rename and delete each write the file and answer with the name as the file spells it.
#[tokio::test]
async fn each_op_writes_the_file_and_answers_the_name() {
    let (dir, path, daemon) = daemon_on("ops", "schema = 1\n");
    let saved = daemon.handle(save(imported(" Mine ", 3.0), false)).await;
    assert_eq!(
        saved,
        Response::PresetWritten {
            name: "Mine".into()
        }
    );
    let renamed = daemon.handle(rename("mine", " Yours ")).await;
    assert_eq!(
        renamed,
        Response::PresetWritten {
            name: "Yours".into()
        }
    );
    assert_eq!(
        Config::load_from(&path).unwrap().presets[0].name,
        "Yours",
        "the file holds the rename"
    );
    let over = daemon.handle(save(imported("yours", 5.0), true)).await;
    assert_eq!(
        over,
        Response::PresetWritten {
            name: "yours".into()
        }
    );
    assert_eq!(Config::load_from(&path).unwrap().presets.len(), 1);
    let gone = daemon.handle(delete("YOURS")).await;
    assert_eq!(
        gone,
        Response::PresetWritten {
            name: "yours".into()
        }
    );
    assert!(Config::load_from(&path).unwrap().presets.is_empty());
    assert!(
        daemon.config_snapshot().presets.is_empty(),
        "the snapshot follows"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A taken name is refused with the reason `taken` and the core's words, the same words the web
/// page's 422 carries; a built-in's name, a blank, a missing preset and a built-in to rename or
/// delete are each refused with their own reason and nothing written.
#[tokio::test]
async fn a_refusal_names_its_reason_and_writes_nothing() {
    let (dir, path, daemon) = daemon_on("refusals", "schema = 1\n");
    daemon.handle(save(imported("Mine", 3.0), false)).await;
    daemon.handle(save(imported("Other", 3.0), false)).await;

    let (reason, words) = refusal(daemon.handle(save(imported("mine", 4.0), false)).await);
    assert_eq!(reason, Some(Reason::Taken));
    let wanted = preset::new_name("mine", ["Mine"], None)
        .unwrap_err()
        .to_string();
    assert_eq!(words, wanted, "the core's words, as the page says them");

    let (reason, words) = refusal(daemon.handle(rename("Mine", "other")).await);
    assert_eq!(reason, Some(Reason::Taken), "{words}");
    let (reason, _) = refusal(daemon.handle(save(imported("Flat", 1.0), false)).await);
    assert_eq!(reason, Some(Reason::BadName));
    let (reason, _) = refusal(daemon.handle(rename("Mine", "  ")).await);
    assert_eq!(reason, Some(Reason::BadName));
    let (reason, words) = refusal(daemon.handle(rename("Flat", "Mine2")).await);
    assert_eq!(reason, Some(Reason::BadName));
    assert!(words.contains("built-in"), "{words}");
    let (reason, words) = refusal(daemon.handle(delete("Flat")).await);
    assert_eq!(reason, Some(Reason::BadName));
    assert!(words.contains("built-in"), "{words}");
    let (reason, words) = refusal(daemon.handle(rename("Nope", "Mine2")).await);
    assert_eq!(reason, Some(Reason::Missing));
    assert!(words.contains("no preset named"), "{words}");
    let (reason, _) = refusal(daemon.handle(delete("Nope")).await);
    assert_eq!(reason, Some(Reason::Missing));

    let names: Vec<_> = Config::load_from(&path)
        .unwrap()
        .presets
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, ["Mine", "Other"]);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A device's curve saved over an imported preset is refused (the web page's 422); an imported
/// preset written over another imported one, as an import with `force` does, is not.
#[tokio::test]
async fn a_device_curve_never_overwrites_an_imported_preset() {
    let (dir, path, daemon) = daemon_on("imported", "schema = 1\n");
    let imp = imported("Cans", 3.0);
    assert!(imp.is_imported());
    let first = daemon.handle(save(imp.clone(), false)).await;
    assert_eq!(
        first,
        Response::PresetWritten {
            name: "Cans".into()
        }
    );
    let device = Preset {
        name: "Cans".into(),
        bands: 3,
        mode: svolder_profiles::convert::EqMode::Eq,
        loudness: 0,
        band: Vec::new(),
        source: None,
    };
    let (reason, words) = refusal(daemon.handle(save(device, true)).await);
    assert_eq!(reason, Some(Reason::BadName));
    assert_eq!(words, preset::imported_update_refusal("Cans"));
    assert_eq!(
        daemon.handle(save(imp, true)).await,
        Response::PresetWritten {
            name: "Cans".into()
        }
    );
    assert_eq!(Config::load_from(&path).unwrap().presets.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// An update writes over a preset that is there and says it is gone when it is not, where a save
/// with `force` would add it.
#[tokio::test]
async fn an_update_of_a_preset_that_is_gone_says_so() {
    let (dir, path, daemon) = daemon_on("update", "schema = 1\n");
    let update = |p: Preset| Request::PresetWrite {
        op: PresetOp::Update {
            preset: Box::new(p),
        },
    };
    let (reason, words) = refusal(daemon.handle(update(imported("Mine", 3.0))).await);
    assert_eq!(reason, Some(Reason::Missing));
    assert!(words.contains("no preset named"), "{words}");
    assert!(Config::load_from(&path).unwrap().presets.is_empty());
    daemon.handle(save(imported("Mine", 3.0), false)).await;
    assert_eq!(
        daemon.handle(update(imported("mine", 5.0))).await,
        Response::PresetWritten {
            name: "mine".into()
        }
    );
    assert_eq!(Config::load_from(&path).unwrap().presets.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
