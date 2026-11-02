// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use super::*;

/// A correction with every kind of filter and a negative preamp.
fn correction() -> Correction {
    Correction {
        preamp_db: -6.1,
        filters: vec![
            SourceFilter {
                kind: FilterKind::LowShelf,
                f: 105.0,
                g: 6.0,
                q: 0.7,
            },
            SourceFilter {
                kind: FilterKind::Peaking,
                f: 180.0,
                g: -2.1,
                q: 0.9,
            },
            SourceFilter {
                kind: FilterKind::HighShelf,
                f: 10000.0,
                g: -0.0,
                q: 0.7,
            },
        ],
    }
}

/// The smart route's config, as the live check ran it, for the correction above.
const SMART: &str = r#"# Written by `svolder host eq apply`; `svolder host eq remove` deletes it.
context.properties = {
    log.level = 2
}

context.spa-libs = {
    audio.convert.* = audioconvert/libspa-audioconvert
    support.*       = support/libspa-support
}

context.modules = [
    { name = libpipewire-module-rt
        args = { }
        flags = [ ifexists nofail ]
    }
    { name = libpipewire-module-protocol-native }
    { name = libpipewire-module-client-node }
    { name = libpipewire-module-adapter }
    { name = libpipewire-module-filter-chain
        args = {
            node.description = "svolder EQ"
            media.name       = "svolder EQ"
            filter.graph = {
                nodes = [
                    {
                        type  = builtin
                        name  = eq
                        label = param_eq
                        config = {
                            filters = [
                                { type = bq_highshelf freq = 0 gain = -6.1 q = 1.0 }
                                { type = bq_lowshelf freq = 105 gain = 6 q = 0.7 }
                                { type = bq_peaking freq = 180 gain = -2.1 q = 0.9 }
                                { type = bq_highshelf freq = 10000 gain = 0 q = 0.7 }
                            ]
                        }
                    }
                ]
                inputs  = [ "eq:In 1" "eq:In 2" ]
                outputs = [ "eq:Out 1" "eq:Out 2" ]
            }
            audio.channels = 2
            audio.position = [ FL FR ]
            capture.props = {
                node.name = "svolder_eq"
                node.description = "svolder EQ"
                media.class = Audio/Sink
                filter.smart = true
                filter.smart.name = "svolder-eq"
                filter.smart.target = { node.name = "alsa_output.usb-dac" }
                priority.session = 0
            }
            playback.props = {
                node.name = "svolder_eq.output"
                node.passive = true
                stream.dont-remix = true
                node.dont-fallback = true
                node.linger = true
            }
        }
    }
]
"#;

#[test]
fn the_smart_route_is_a_smart_filter_that_waits_for_its_sink() {
    let text = render_conf(&correction(), "alsa_output.usb-dac", Route::Smart);
    assert_eq!(text, SMART);
}

#[test]
fn the_default_route_plays_into_its_target_and_is_no_smart_filter() {
    let text = render_conf(&correction(), "alsa_output.usb-dac", Route::Default);
    let capture_and_playback = "            capture.props = {\n\
         \x20               node.name = \"svolder_eq\"\n\
         \x20               node.description = \"svolder EQ\"\n\
         \x20               media.class = Audio/Sink\n\
         \x20           }\n\
         \x20           playback.props = {\n\
         \x20               node.name = \"svolder_eq.output\"\n\
         \x20               node.passive = true\n\
         \x20               target.object = \"alsa_output.usb-dac\"\n\
         \x20               stream.dont-remix = true\n\
         \x20           }\n";
    assert!(text.contains(capture_and_playback), "{text}");
    for smart in [
        "filter.smart",
        "priority.session",
        "dont-fallback",
        "linger",
    ] {
        assert!(!text.contains(smart), "{smart}: {text}");
    }
    // Everything above the two property blocks is the smart route's text.
    let head = |text: &str| text[..text.find("            capture.props").unwrap()].to_string();
    assert_eq!(head(&text), head(SMART));
}

#[test]
fn the_preamp_is_the_first_filter_and_a_flat_correction_has_only_it() {
    let flat = Correction {
        preamp_db: 0.0,
        filters: Vec::new(),
    };
    let text = render_conf(&flat, "dac", Route::Smart);
    assert!(
        text.contains(
            "filters = [\n                                { type = bq_highshelf freq = 0 \
             gain = 0 q = 1.0 }\n                            ]"
        ),
        "{text}"
    );
}

#[test]
fn a_target_with_a_quote_cannot_leave_its_string() {
    let text = render_conf(&correction(), "a\"b\\c\nd", Route::Smart);
    assert!(
        text.contains("filter.smart.target = { node.name = \"a\\\"b\\\\cd\" }"),
        "{text}"
    );
}

#[test]
fn the_unit_follows_pipewire_and_runs_the_absolute_command() {
    let unit = render_unit(
        &PathBuf::from("/usr/bin/pipewire"),
        &PathBuf::from("/home/user/.config/svolder/host-eq.conf"),
    );
    assert_eq!(
        unit,
        "[Unit]\n\
         Description=svolder host EQ (PipeWire filter chain)\n\
         After=pipewire.service pipewire-session-manager.service\n\
         BindsTo=pipewire.service\n\
         \n\
         [Service]\n\
         LockPersonality=yes\n\
         MemoryDenyWriteExecute=yes\n\
         NoNewPrivileges=yes\n\
         RestrictNamespaces=yes\n\
         SystemCallArchitectures=native\n\
         SystemCallFilter=@system-service\n\
         Type=simple\n\
         ExecStart=/usr/bin/pipewire -c /home/user/.config/svolder/host-eq.conf\n\
         Restart=on-failure\n\
         Slice=session.slice\n\
         \n\
         [Install]\n\
         WantedBy=pipewire.service\n"
    );
}

#[test]
fn a_path_with_a_blank_or_a_percent_is_quoted_in_the_unit() {
    let unit = render_unit(
        &PathBuf::from("/usr/bin/pipewire"),
        &PathBuf::from("/home/my user/50%/host-eq.conf"),
    );
    assert!(
        unit.contains("ExecStart=/usr/bin/pipewire -c \"/home/my user/50%%/host-eq.conf\"\n"),
        "{unit}"
    );
}

#[test]
fn the_desktop_entry_is_a_hidden_application_with_the_same_command() {
    let desktop = render_desktop(
        &PathBuf::from("/usr/bin/pipewire"),
        &PathBuf::from("/home/user/.config/svolder/host-eq.conf"),
    );
    assert_eq!(
        desktop,
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=svolder host EQ\n\
         Exec=/usr/bin/pipewire -c /home/user/.config/svolder/host-eq.conf\n\
         NoDisplay=true\n"
    );
    let spaced = render_desktop(
        &PathBuf::from("/usr/bin/pipewire"),
        &PathBuf::from("/home/my user/host-eq.conf"),
    );
    assert!(
        spaced.contains("Exec=/usr/bin/pipewire -c \"/home/my user/host-eq.conf\"\n"),
        "{spaced}"
    );
}

#[test]
fn the_linked_wireplumber_version_is_read_from_wpctl() {
    let real = "wpctl \nCompiled with libwireplumber 0.5.10\nLinked with libwireplumber 0.5.13\n";
    assert_eq!(wireplumber_version(real), Some((0, 5, 13)));
    assert_eq!(
        wireplumber_version("Linked with libwireplumber 0.4.17"),
        Some((0, 4, 17))
    );
    assert_eq!(
        wireplumber_version("Linked with libwireplumber 1.0"),
        Some((1, 0, 0))
    );
}

#[test]
fn only_the_linked_line_counts_and_it_must_parse() {
    for text in [
        "",
        "Compiled with libwireplumber 0.5.13\n",
        "Linked with libwireplumber\n",
        "Linked with libwireplumber next\n",
        "wpctl 0.5.13\n",
    ] {
        assert_eq!(wireplumber_version(text), None, "{text:?}");
    }
}

#[test]
fn wireplumber_0_5_takes_the_smart_route_and_older_the_default_one() {
    assert_eq!(Route::for_wireplumber((0, 5, 0)), Route::Smart);
    assert_eq!(Route::for_wireplumber((0, 5, 13)), Route::Smart);
    assert_eq!(Route::for_wireplumber((1, 0, 0)), Route::Smart);
    assert_eq!(Route::for_wireplumber((0, 4, 17)), Route::Default);
}

#[test]
fn managers_read_by_their_word() {
    assert_eq!(Manager::parse(" Systemd\n"), Some(Manager::Systemd));
    assert_eq!(Manager::parse("PROCESS"), Some(Manager::Process));
    assert_eq!(Manager::parse("launchd"), None);
}

fn scratch(what: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("svolder-hosteq-{what}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn state() -> HostEqState {
    let c = correction();
    HostEqState {
        on: true,
        name: "Sennheiser HD 650".into(),
        origin: Some("AutoEq · oratory1990 · over-ear".into()),
        target: "alsa_output.usb-dac".into(),
        route: Route::Default,
        manager: Manager::Process,
        previous_default: Some("alsa_output.pci-speakers".into()),
        preamp_db: c.preamp_db,
        filters: c.filters,
    }
}

#[test]
fn a_state_round_trips_through_its_file() {
    let dir = scratch("roundtrip");
    let path = dir.join("nested").join("host-eq.toml");
    let before = state();
    before.save(&path).expect("the state saves");
    assert_eq!(HostEqState::load(&path), Ok(Some(before.clone())));
    assert_eq!(before.correction(), correction());
    let mut without = state();
    without.origin = None;
    without.previous_default = None;
    without.save(&path).expect("the state saves again");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        !text.contains("origin") && !text.contains("previous_default"),
        "{text}"
    );
    assert_eq!(HostEqState::load(&path), Ok(Some(without)));
    let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(leftovers.len(), 1, "{leftovers:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_state_is_none_and_a_broken_one_names_its_file() {
    let dir = scratch("broken");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("host-eq.toml");
    assert_eq!(HostEqState::load(&path), Ok(None));
    std::fs::write(&path, "on = \"maybe\"").unwrap();
    let error = HostEqState::load(&path).unwrap_err();
    assert!(error.starts_with(&path.display().to_string()), "{error}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_desktop_exec_is_escaped_for_the_exec_rule_and_then_for_the_string_rule() {
    // A backslash reads `\\\\` in the file, a dollar `\\$`, a quote `\\"` and a backtick
    // `\\``; the whole argument is quoted and `%` is doubled.
    let desktop = render_desktop(
        &PathBuf::from("/usr/bin/pipewire"),
        &PathBuf::from("/home/a\\b$c\"d`e 100%/host-eq.conf"),
    );
    assert!(
        desktop.contains(
            "Exec=/usr/bin/pipewire -c \"/home/a\\\\\\\\b\\\\$c\\\\\"d\\\\`e 100%%/host-eq.conf\"\n"
        ),
        "{desktop}"
    );
}

#[test]
#[should_panic(expected = "validated before it is rendered")]
fn a_number_that_is_not_finite_is_never_rendered() {
    let mut c = correction();
    c.filters[0].q = f64::NAN;
    render_conf(&c, "dac", Route::Smart);
}

/// A staged file is beside its path and not at it until it is committed, and one dropped
/// uncommitted leaves nothing behind; the file at the path is the old one until the commit.
#[test]
fn a_staged_file_changes_nothing_until_it_is_committed() {
    let dir = std::env::temp_dir().join(format!("svolder-staged-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("sub/file.conf");
    write_atomically(&path, b"old").unwrap();

    let dropped = Staged::write(&path, b"dropped").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"old");
    drop(dropped);
    let staged = Staged::write(&path, b"new").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"old");
    staged.commit().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"new");
    let left: Vec<_> = std::fs::read_dir(dir.join("sub"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(left, ["file.conf"], "a temporary file was left behind");
    std::fs::remove_dir_all(&dir).unwrap();
}
