// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Cases that came out of review rounds.

use super::*;

#[tokio::test]
async fn a_re_apply_that_changes_manager_stops_the_other_ones_leftovers_with_no_state_file() {
    let f = Fixture::new();
    // A unit file and an autostart entry no state file names.
    std::fs::write(f.dir.join("units/svolder-host-eq.service"), "[Unit]\n").unwrap();
    std::fs::create_dir_all(f.dir.join("config/autostart")).unwrap();
    let desktop = f.dir.join("config/autostart/svolder-host-eq.desktop");
    std::fs::write(&desktop, "[Desktop Entry]\n").unwrap();
    assert!(!f.host(Manager::Process).state.exists());
    f.host(Manager::Process)
        .apply(request("Now"))
        .await
        .unwrap();
    assert!(!f.dir.join("units/svolder-host-eq.service").exists());
    assert!(
        f.systemctl_writes()
            .iter()
            .any(|line| line.contains("stop")),
        "{:?}",
        f.systemctl_writes()
    );
    f.host(Manager::Process).remove().await.unwrap();
    // And the other way: an autostart entry is removed when systemd takes over.
    std::fs::create_dir_all(f.dir.join("config/autostart")).unwrap();
    std::fs::write(&desktop, "[Desktop Entry]\n").unwrap();
    std::fs::remove_file(&f.host(Manager::Systemd).state).unwrap();
    f.host(Manager::Systemd)
        .apply(request("Next"))
        .await
        .unwrap();
    assert!(!desktop.exists());
}

#[test]
fn a_pipewire_path_with_escape_characters_is_found_again_through_the_entry() {
    let dir = std::env::temp_dir().join(format!("svhe-desktop-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dir = TempDir(dir);
    let entry = dir.join("e.desktop");
    for program in [
        "/opt/a b/pipewire",
        "/opt/back\\slash/pipewire",
        "/opt/q\"uote/pipewire",
        "/opt/dol$lar/pipewire",
        "/opt/tick`/pipewire",
        "/opt/100%/pipewire",
        "/opt/plain/pipewire",
    ] {
        std::fs::write(
            &entry,
            render_desktop(Path::new(program), Path::new("/etc/c.conf")),
        )
        .unwrap();
        assert_eq!(
            desktop_program(&entry).as_deref(),
            Some(program),
            "{program}"
        );
    }
}

// ---- residual round ----

#[test]
fn a_narrow_boost_between_two_grid_points_is_measured_at_its_true_height() {
    let grid = response::log_freqs(1024);
    let k = grid.iter().position(|f| *f > 13_500.0).unwrap();
    let centre = (grid[k] * grid[k + 1]).sqrt();
    let narrow = Correction {
        preamp_db: 0.0,
        filters: vec![SourceFilter {
            kind: FilterKind::Peaking,
            f: centre,
            g: 12.0,
            q: 20.0,
        }],
    };
    let truth = response::correction_db(&narrow, &[centre])[0];
    let sampled = response::correction_db(&narrow, &grid)
        .into_iter()
        .fold(f64::NEG_INFINITY, f64::max);
    assert!(
        truth - sampled > 0.5,
        "the grid misses it: {truth} {sampled}"
    );
    let preamp = safe_preamp(&narrow);
    assert!(preamp <= -truth + 1e-9, "{preamp} {truth}");
    assert!(preamp > -truth - 0.1 - 1e-9, "{preamp} {truth}");
}

#[tokio::test]
async fn a_curve_peaking_above_thirty_decibels_applies_and_applies_again() {
    let f = Fixture::new();
    let host = f.host(Manager::Process);
    let mut asked = request("Loud");
    asked.correction.preamp_db = 0.0;
    asked.correction.filters = vec![
        SourceFilter {
            kind: FilterKind::Peaking,
            f: 1000.0,
            g: 20.0,
            q: 1.0,
        },
        SourceFilter {
            kind: FilterKind::Peaking,
            f: 1000.0,
            g: 20.0,
            q: 1.0,
        },
    ];
    let first = host.apply(asked).await.unwrap();
    assert!(first.preamp_db < -30.0, "{}", first.preamp_db);
    let again = host
        .apply(ApplyRequest {
            name: first.name.clone(),
            origin: None,
            correction: first.correction(),
            sink: None,
            slots: Vec::new(),
            rate_hz: None,
            flat: false,
            crossfeed: None,
            speakers: None,
            identity: None,
        })
        .await
        .unwrap();
    assert_eq!(again.preamp_db, first.preamp_db);
}
