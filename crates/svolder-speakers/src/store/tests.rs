// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::atomic::{AtomicU32, Ordering};

use super::*;

struct Dir(PathBuf);

impl Dir {
    fn new() -> Dir {
        static N: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "svolder-speakers-store-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        Dir(path)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn wav() -> Vec<u8> {
    crate::wav::write(
        48000,
        &[
            vec![0.5; 10],
            vec![0.25; 10],
            vec![0.125; 10],
            vec![1.0; 10],
        ],
    )
}

fn meta(id: &str, at: u64) -> SetMeta {
    SetMeta {
        id: id.to_string(),
        room: Room::No,
        label: format!("Set {id}"),
        licence: "CC-BY-4.0".into(),
        credit: "Somebody".into(),
        source_url: "https://example.invalid/set".into(),
        source_sha256: "0".repeat(64),
        wav_sha256: String::new(),
        rate: 48000,
        taps: 10,
        taps_ms: 10.0 * 1000.0 / 48000.0,
        angles: None,
        loudness_db: -3.5,
        scale_db: -0.1,
        worst_gain_db: -0.1,
        cut: false,
        own: false,
        added: at,
        used: at,
    }
}

#[test]
fn a_saved_set_checks_lists_and_forgets() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    assert_eq!(store.save(meta("a", 1), &wav(), None), Ok(vec![]));
    let checked = store.check("a").unwrap();
    assert_eq!(
        checked.wav_sha256,
        crate::sha256_hex(&wav()),
        "the checksum is worked out by the store"
    );
    assert_eq!(checked.label, "Set a");
    assert!(store.wav_path("a").unwrap().is_absolute());
    let list = store.list();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].health, Health::Ok);
    store.forget("a").unwrap();
    assert!(store.list().is_empty());
    assert_eq!(
        store.forget("a"),
        Ok(()),
        "forgetting what is not there is not an error"
    );
}

#[test]
fn the_sidecar_keeps_the_angles_and_every_number() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    let mut m = meta("a", 1);
    let direction = |az_deg| crate::ir::Direction {
        az_deg,
        el_deg: 0.0,
        distance_m: 1.2,
    };
    m.angles = Some(Angles {
        left: direction(30.0),
        right: direction(330.0),
    });
    store.save(m.clone(), &wav(), None).unwrap();
    let back = store.check("a").unwrap();
    m.wav_sha256 = back.wav_sha256.clone();
    assert_eq!(back, m);
}

#[test]
fn a_changed_wav_is_damaged() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    store.save(meta("a", 1), &wav(), None).unwrap();
    let mut bytes = std::fs::read(store.wav_path("a").unwrap()).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    std::fs::write(store.wav_path("a").unwrap(), bytes).unwrap();
    let error = store.check("a").unwrap_err();
    assert!(
        matches!(&error, StoreError::Damaged { id, reason } if id == "a" && reason.contains("checksum")),
        "{error}"
    );
    assert!(error.to_string().contains("fetch it again"));
    assert!(matches!(store.list()[0].health, Health::Damaged(_)));
    assert!(store.list()[0].meta.is_some(), "the sidecar still names it");
}

#[test]
fn a_truncated_wav_whose_checksum_was_recomputed_is_still_damaged() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    store.save(meta("a", 1), &wav(), None).unwrap();
    let mut bytes = std::fs::read(store.wav_path("a").unwrap()).unwrap();
    bytes.truncate(bytes.len() - 8);
    std::fs::write(store.wav_path("a").unwrap(), &bytes).unwrap();
    // The sidecar is rewritten to match the cut file: only the header check can catch it now.
    let sidecar = store.sidecar_path("a").unwrap();
    let text = std::fs::read_to_string(&sidecar).unwrap();
    let old = crate::sha256_hex(&wav());
    std::fs::write(&sidecar, text.replace(&old, &crate::sha256_hex(&bytes))).unwrap();
    assert!(
        matches!(store.check("a"), Err(StoreError::Damaged { reason, .. }) if reason.contains("header"))
    );
}

#[test]
fn a_wav_of_the_wrong_shape_is_damaged() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    let two_channels = crate::wav::write(48000, &[vec![0.0; 10], vec![0.0; 10]]);
    store.save(meta("a", 1), &two_channels, None).unwrap();
    assert!(
        matches!(store.check("a"), Err(StoreError::Damaged { reason, .. }) if reason.contains("four-channel"))
    );
    let mut other_rate = meta("b", 1);
    other_rate.rate = 44100;
    store.save(other_rate, &wav(), None).unwrap();
    assert!(matches!(store.check("b"), Err(StoreError::Damaged { .. })));
}

#[test]
fn a_missing_sidecar_or_wav_is_damaged_and_listed() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    store.save(meta("a", 1), &wav(), None).unwrap();
    store.save(meta("b", 1), &wav(), None).unwrap();
    std::fs::remove_file(store.sidecar_path("a").unwrap()).unwrap();
    std::fs::remove_file(store.wav_path("b").unwrap()).unwrap();
    assert!(
        matches!(store.check("a"), Err(StoreError::Damaged { reason, .. }) if reason.contains("sidecar"))
    );
    assert!(
        matches!(store.check("b"), Err(StoreError::Damaged { reason, .. }) if reason.contains("WAV is missing"))
    );
    let list = store.list();
    assert_eq!(
        list.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert!(list.iter().all(|s| matches!(s.health, Health::Damaged(_))));
    assert!(list[0].meta.is_none() && list[1].meta.is_some());
}

#[test]
fn an_unreadable_sidecar_is_damaged() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    store.save(meta("a", 1), &wav(), None).unwrap();
    std::fs::write(store.sidecar_path("a").unwrap(), "this is [not toml").unwrap();
    assert!(matches!(store.check("a"), Err(StoreError::Damaged { .. })));
}

#[test]
fn the_33rd_set_evicts_the_oldest_unused_and_never_the_chosen_one() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    for i in 0..MAX_SETS {
        assert_eq!(
            store.save(meta(&format!("s{i:02}"), 100 + i as u64), &wav(), None),
            Ok(vec![])
        );
    }
    // s00 is the oldest, but it is the chosen one; s01 is the oldest unused.
    let gone = store.save(meta("new", 500), &wav(), Some("s00")).unwrap();
    assert_eq!(gone, ["s01"]);
    let ids: Vec<String> = store.list().into_iter().map(|s| s.id).collect();
    assert_eq!(ids.len(), MAX_SETS);
    assert!(ids.contains(&"s00".to_string()) && ids.contains(&"new".to_string()));
    assert!(!ids.contains(&"s01".to_string()));
}

#[test]
fn a_set_used_lately_outlasts_an_older_one() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    for i in 0..MAX_SETS {
        store
            .save(meta(&format!("s{i:02}"), 100 + i as u64), &wav(), None)
            .unwrap();
    }
    store.touch("s00", 9000).unwrap();
    let gone = store.save(meta("new", 9001), &wav(), None).unwrap();
    assert_eq!(gone, ["s01"]);
    assert_eq!(store.check("s00").unwrap().used, 9000);
}

/// A touch of a set whose WAV went (a forget that raced it) leaves no sidecar behind: no ghost
/// set is listed.
#[test]
fn a_touch_of_a_set_whose_wav_is_gone_writes_nothing() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    store.save(meta("s00", 100), &wav(), None).unwrap();
    std::fs::remove_file(store.wav_path("s00").unwrap()).unwrap();
    let sidecar = dir.0.join("s00.toml");
    let before = std::fs::read_to_string(&sidecar).unwrap();
    store.touch("s00", 9000).unwrap();
    assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), before);
    std::fs::remove_file(&sidecar).unwrap();
    assert!(
        store.touch("s00", 9000).is_ok(),
        "a set that is gone is no error"
    );
    assert!(store.list().is_empty(), "no ghost");
}

#[test]
fn the_set_just_saved_is_never_the_one_evicted() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    for i in 0..MAX_SETS {
        store
            .save(meta(&format!("s{i:02}"), 100 + i as u64), &wav(), None)
            .unwrap();
    }
    // Older than every other, and nothing chosen: still kept.
    let gone = store.save(meta("old", 1), &wav(), None).unwrap();
    assert_eq!(gone, ["s00"]);
    assert!(store.check("old").is_ok());
}

#[test]
fn an_id_that_is_not_a_safe_name_is_refused() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    for bad in ["", "../x", "a/b", "A", "-a", "a.b", &"a".repeat(49)] {
        assert!(!valid_id(bad), "{bad:?}");
        assert!(matches!(store.wav_path(bad), Err(StoreError::BadId(_))));
        assert!(matches!(
            store.save(meta(bad, 1), &wav(), None),
            Err(StoreError::BadId(_))
        ));
    }
    assert!(valid_id("own-1a2b3c4d") && valid_id("sadie-d2-brir"));
    assert!(!dir.0.exists(), "a refused id writes nothing");
}

#[test]
fn a_save_that_fails_leaves_no_temporary_file() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    store.save(meta("a", 1), &wav(), None).unwrap();
    // A directory where the sidecar goes makes the rename fail.
    std::fs::remove_file(store.sidecar_path("a").unwrap()).unwrap();
    std::fs::create_dir(store.sidecar_path("a").unwrap()).unwrap();
    assert!(matches!(
        store.save(meta("a", 2), &wav(), None),
        Err(StoreError::Io { .. })
    ));
    let names: Vec<String> = std::fs::read_dir(&dir.0)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().all(|n| !n.contains("tmp")), "{names:?}");
}

#[test]
fn a_sidecar_naming_another_set_is_damaged() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    store.save(meta("a", 1), &wav(), None).unwrap();
    let path = store.sidecar_path("a").unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replace("id = \"a\"", "id = \"b\"")).unwrap();
    assert!(
        matches!(store.check("a"), Err(StoreError::Damaged { reason, .. }) if reason.contains("another set"))
    );
    assert!(matches!(store.list()[0].health, Health::Damaged(_)));
}

#[test]
fn a_wav_holding_a_nan_is_damaged() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    let mut channels = vec![
        vec![0.5; 10],
        vec![0.25; 10],
        vec![0.125; 10],
        vec![1.0; 10],
    ];
    channels[2][4] = f32::NAN;
    store
        .save(meta("a", 1), &crate::wav::write(48000, &channels), None)
        .unwrap();
    assert!(
        matches!(store.check("a"), Err(StoreError::Damaged { reason, .. }) if reason.contains("not numbers"))
    );
}

#[test]
fn orphan_files_do_not_count_against_the_limit_and_are_cleaned() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    std::fs::create_dir_all(&dir.0).unwrap();
    for i in 0..3 {
        std::fs::write(dir.0.join(format!("orphan{i}.wav")), b"x").unwrap();
    }
    for i in 0..(MAX_SETS + 1) {
        store
            .save(meta(&format!("s{i:02}"), 100 + i as u64), &wav(), None)
            .unwrap();
    }
    let ids: Vec<String> = store.list().into_iter().map(|s| s.id).collect();
    assert_eq!(ids.len(), MAX_SETS, "{ids:?}");
    assert!(ids.iter().all(|id| !id.starts_with("orphan")));
}

#[test]
fn the_set_in_use_is_never_forgotten_or_evicted() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    for i in 0..MAX_SETS {
        store
            .save(meta(&format!("s{i:02}"), 100 + i as u64), &wav(), None)
            .unwrap();
    }
    // s00 is the oldest; it is in use (through a clone, as the controller holds it).
    store.clone().set_in_use(&["s00"]);
    assert_eq!(store.forget("s00"), Err(StoreError::InUse("s00".into())));
    assert!(store.check("s00").is_ok());
    let gone = store.save(meta("new", 500), &wav(), None).unwrap();
    assert_eq!(
        gone,
        ["s01"],
        "the oldest unused one goes, not the one in use"
    );
    assert!(store.check("s00").is_ok());
    store.set_in_use(&[]);
    assert_eq!(store.forget("s00"), Ok(()));
}

#[test]
fn every_set_pinned_together_is_kept_until_the_pin_is_replaced() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    for id in ["old", "new", "other"] {
        store.save(meta(id, 10), &wav(), None).unwrap();
    }
    // The chain that holds `old` is being applied again with `new`.
    store.set_in_use(&["old", "new"]);
    assert_eq!(store.forget("old"), Err(StoreError::InUse("old".into())));
    assert_eq!(store.forget("new"), Err(StoreError::InUse("new".into())));
    assert_eq!(store.forget("other"), Ok(()));
    // The new chain runs: `old` is free.
    store.set_in_use(&["new"]);
    assert_eq!(store.forget("old"), Ok(()));
    assert_eq!(store.forget("new"), Err(StoreError::InUse("new".into())));
}

type Hook = std::sync::Arc<dyn Fn(&str) + Send + Sync>;

static HOOK: std::sync::Mutex<Option<Hook>> = std::sync::Mutex::new(None);

/// Called by the store between its in-use check and a removal; a test sets what it does.
pub(super) fn between_check_and_removal(id: &str) {
    let hook = HOOK.lock().unwrap().clone();
    if let Some(hook) = hook {
        hook(id);
    }
}

/// A pin that arrives between a forget's check and its removal waits for the removal: it never
/// finds the set sound and then loses its files.
#[test]
fn a_pin_set_during_a_forget_waits_for_it() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    store.save(meta("racy-hook", 10), &wav(), None).unwrap();
    let pinner = std::sync::Arc::new(std::sync::Mutex::new(None));
    {
        let (store, pinner) = (store.clone(), pinner.clone());
        *HOOK.lock().unwrap() = Some(std::sync::Arc::new(move |id: &str| {
            if id != "racy-hook" {
                return;
            }
            let store = store.clone();
            *pinner.lock().unwrap() = Some(std::thread::spawn(move || {
                store.set_in_use(&["racy-hook"]);
                store.check("racy-hook").is_ok()
            }));
            // Time for the pinning thread to try, if it can.
            std::thread::sleep(std::time::Duration::from_millis(300));
        }));
    }
    let forgot = store.forget("racy-hook");
    *HOOK.lock().unwrap() = None;
    let sound_when_pinned = pinner.lock().unwrap().take().unwrap().join().unwrap();
    assert_eq!(forgot, Ok(()));
    assert!(
        !sound_when_pinned,
        "the pin came in under the removal and saw the set sound"
    );
    assert!(store.check("racy-hook").is_err());
}

#[test]
fn the_quick_list_reads_the_sidecars_and_not_the_wavs() {
    let dir = Dir::new();
    let store = Store::new(&dir.0);
    for id in ["whole", "damaged", "no-wav"] {
        store.save(meta(id, 10), &wav(), None).unwrap();
    }
    let path = store.wav_path("damaged").unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[60] ^= 0xFF;
    std::fs::write(&path, bytes).unwrap();
    std::fs::remove_file(store.wav_path("no-wav").unwrap()).unwrap();
    std::fs::write(dir.0.join("stray.wav"), b"not a set").unwrap();
    let ids: Vec<String> = store.list_sidecars().into_iter().map(|m| m.id).collect();
    assert_eq!(
        ids,
        ["damaged", "whole"],
        "a WAV damaged in place is found by the check"
    );
    let healthy: Vec<String> = store
        .list()
        .into_iter()
        .filter(|status| status.health == Health::Ok)
        .map(|status| status.id)
        .collect();
    assert_eq!(healthy, ["whole"]);
}
