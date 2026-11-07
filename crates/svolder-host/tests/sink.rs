// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The sink against a fake `wpctl`: a script in the case's own directory that keeps a volume,
//! a mute and an `inspect` listing in files beside it and logs every call it gets. Nothing
//! here runs the real `wpctl`.

mod common;

use std::time::{Duration, Instant};

use common::{TempDir, install_script, lines};
use svolder_host::sink::{Sink, SinkState, Wpctl};

/// A `wpctl` that answers `get-volume`, `inspect`, `set-volume` and `set-mute toggle|1|0` on
/// `@DEFAULT_AUDIO_SINK@` from the files beside it, logs each call's arguments to `calls`,
/// and fails anything else.
const FAKE_WPCTL: &str = r#"#!/bin/sh
here=$(dirname "$0")
echo "$*" >> "$here/calls"
[ "$2" = "@DEFAULT_AUDIO_SINK@" ] || exit 2
case "$1" in
get-volume)
    if [ -e "$here/muted" ]; then
        echo "Volume: $(cat "$here/volume") [MUTED]"
    else
        echo "Volume: $(cat "$here/volume")"
    fi
    ;;
inspect) cat "$here/inspect" ;;
set-volume) [ $# -eq 3 ] || exit 2; echo "$3" > "$here/volume" ;;
set-mute)
    case "$3" in
    toggle) if [ -e "$here/muted" ]; then rm "$here/muted"; else : > "$here/muted"; fi ;;
    1) : > "$here/muted" ;;
    0) rm -f "$here/muted" ;;
    *) exit 2 ;;
    esac
    ;;
*) exit 2 ;;
esac
"#;

/// What the fake's `inspect` prints for its sink.
const INSPECT: &str = "id 56, type PipeWire:Interface:Node\n  \
    * media.class = \"Audio/Sink\"\n  \
    * node.description = \"Schiit Modi+ Analog Stereo\"\n  \
    * node.name = \"alsa_output.usb-Schiit_Audio_Modi_-00.analog-stereo\"\n";

/// A fake `wpctl` in a directory of its own, its sink at `volume`.
fn fake(label: &str, volume: &str) -> (TempDir, Wpctl) {
    let dir = TempDir::new(label);
    install_script(&dir.join("wpctl"), FAKE_WPCTL);
    std::fs::write(dir.join("volume"), format!("{volume}\n")).unwrap();
    std::fs::write(dir.join("inspect"), INSPECT).unwrap();
    let wpctl = Wpctl::new(dir.join("wpctl"));
    (dir, wpctl)
}

fn modi(volume: f32, muted: bool) -> SinkState {
    SinkState {
        volume,
        muted,
        name: "Schiit Modi+ Analog Stereo".to_string(),
        target: Some("alsa_output.usb-Schiit_Audio_Modi_-00.analog-stereo".to_string()),
    }
}

#[tokio::test]
async fn a_read_asks_the_default_sink_its_volume_and_its_name() {
    let (dir, wpctl) = fake("sink-read", "0.54");
    assert_eq!(wpctl.read().await, Some(modi(0.54, false)));
    let mut calls = lines(&dir.join("calls"));
    calls.sort();
    assert_eq!(
        calls,
        [
            "get-volume @DEFAULT_AUDIO_SINK@",
            "inspect @DEFAULT_AUDIO_SINK@"
        ]
    );
}

/// The `inspect` calls the fake has had so far.
fn inspects(dir: &TempDir) -> usize {
    lines(&dir.join("calls"))
        .iter()
        .filter(|call| call.starts_with("inspect"))
        .count()
}

/// Every read asks the volume, and only a read that may meet another sink names it again: the
/// first, one after the volume or the mute moved, and one after a read that failed. A sink
/// that reads the same is not inspected on every read.
#[tokio::test]
async fn a_read_names_the_sink_only_when_it_may_have_changed() {
    let (dir, wpctl) = fake("sink-inspect-once", "0.54");
    for _ in 0..3 {
        assert_eq!(wpctl.read().await, Some(modi(0.54, false)));
    }
    assert_eq!(
        lines(&dir.join("calls")),
        [
            "get-volume @DEFAULT_AUDIO_SINK@",
            "inspect @DEFAULT_AUDIO_SINK@",
            "get-volume @DEFAULT_AUDIO_SINK@",
            "get-volume @DEFAULT_AUDIO_SINK@",
        ],
        "a sink that read the same was inspected again"
    );

    std::fs::write(dir.join("volume"), "0.60\n").unwrap();
    assert_eq!(wpctl.read().await, Some(modi(0.6, false)));
    assert_eq!(
        inspects(&dir),
        2,
        "a volume that moved did not name the sink again"
    );
    std::fs::write(dir.join("muted"), "").unwrap();
    assert_eq!(wpctl.read().await, Some(modi(0.6, true)));
    assert_eq!(
        inspects(&dir),
        3,
        "a mute that moved did not name the sink again"
    );

    std::fs::remove_file(dir.join("volume")).unwrap();
    assert_eq!(
        wpctl.read().await,
        None,
        "a volume that did not read gave a sink"
    );
    std::fs::write(dir.join("volume"), "0.60\n").unwrap();
    assert_eq!(wpctl.read().await, Some(modi(0.6, true)));
    assert_eq!(
        inspects(&dir),
        4,
        "the read after a failed one did not name the sink"
    );
}

/// A new default sink that reads the very volume and mute the old one did is named within the
/// time a naming stands for, and not before.
#[tokio::test]
async fn a_new_default_at_the_same_volume_is_named_once_the_naming_is_old() {
    let (dir, wpctl) = fake("sink-inspect-every", "0.54");
    let wpctl = wpctl.with_inspect_every(Duration::from_millis(500));
    assert_eq!(wpctl.read().await, Some(modi(0.54, false)));
    std::fs::write(
        dir.join("inspect"),
        "id 57, type PipeWire:Interface:Node\n  \
         * media.class = \"Audio/Sink\"\n  \
         * node.description = \"HDMI Output\"\n  \
         * node.name = \"alsa_output.hdmi-stereo\"\n",
    )
    .unwrap();
    assert_eq!(
        wpctl.read().await.map(|sink| sink.name),
        Some("Schiit Modi+ Analog Stereo".to_string()),
        "the naming did not stand"
    );
    tokio::time::sleep(Duration::from_millis(600)).await;
    let hdmi = wpctl.read().await.expect("the sink reads");
    assert_eq!(hdmi.name, "HDMI Output");
    assert_eq!(hdmi.target.as_deref(), Some("alsa_output.hdmi-stereo"));
    assert_eq!(inspects(&dir), 2);
}

#[tokio::test]
async fn a_muted_sink_reads_muted() {
    let (dir, wpctl) = fake("sink-muted", "0.54");
    std::fs::write(dir.join("muted"), "").unwrap();
    assert_eq!(wpctl.read().await, Some(modi(0.54, true)));
}

#[tokio::test]
async fn a_volume_is_written_to_the_default_sink_and_read_back() {
    let (dir, wpctl) = fake("sink-volume", "0.54");
    assert!(wpctl.set_volume(0.56).await);
    assert_eq!(
        lines(&dir.join("calls")),
        ["set-volume @DEFAULT_AUDIO_SINK@ 0.56"]
    );
    assert_eq!(wpctl.read().await, Some(modi(0.56, false)));
}

#[tokio::test]
async fn a_volume_is_capped_to_0_to_100_percent_and_never_a_boost() {
    let (dir, wpctl) = fake("sink-capped", "0.54");
    assert!(wpctl.set_volume(1.3).await);
    assert!(wpctl.set_volume(-0.2).await);
    assert_eq!(
        lines(&dir.join("calls")),
        [
            "set-volume @DEFAULT_AUDIO_SINK@ 1.00",
            "set-volume @DEFAULT_AUDIO_SINK@ 0.00"
        ]
    );
}

#[tokio::test]
async fn a_volume_that_is_not_a_number_is_not_sent() {
    let (dir, wpctl) = fake("sink-nan", "0.54");
    assert!(!wpctl.set_volume(f32::NAN).await);
    assert!(!wpctl.set_volume(f32::INFINITY).await);
    assert!(lines(&dir.join("calls")).is_empty());
}

#[tokio::test]
async fn a_mute_toggle_mutes_the_default_sink_and_a_second_unmutes_it() {
    let (dir, wpctl) = fake("sink-mute", "0.54");
    assert!(wpctl.toggle_mute().await);
    assert_eq!(wpctl.read().await, Some(modi(0.54, true)));
    assert!(wpctl.toggle_mute().await);
    assert_eq!(wpctl.read().await, Some(modi(0.54, false)));
    let toggles: Vec<String> = lines(&dir.join("calls"))
        .into_iter()
        .filter(|call| call.starts_with("set-mute"))
        .collect();
    assert_eq!(
        toggles,
        [
            "set-mute @DEFAULT_AUDIO_SINK@ toggle",
            "set-mute @DEFAULT_AUDIO_SINK@ toggle"
        ]
    );
}

#[tokio::test]
async fn set_mute_sends_one_or_zero() {
    let (dir, wpctl) = fake("sink-set-mute", "0.54");
    assert!(wpctl.set_mute(true).await);
    assert_eq!(wpctl.read().await, Some(modi(0.54, true)));
    assert!(wpctl.set_mute(true).await);
    assert_eq!(wpctl.read().await, Some(modi(0.54, true)));
    assert!(wpctl.set_mute(false).await);
    assert_eq!(wpctl.read().await, Some(modi(0.54, false)));
    let sets: Vec<String> = lines(&dir.join("calls"))
        .into_iter()
        .filter(|call| call.starts_with("set-mute"))
        .collect();
    assert_eq!(
        sets,
        [
            "set-mute @DEFAULT_AUDIO_SINK@ 1",
            "set-mute @DEFAULT_AUDIO_SINK@ 1",
            "set-mute @DEFAULT_AUDIO_SINK@ 0"
        ]
    );
}

#[tokio::test]
async fn a_missing_tool_reads_no_sink_and_writes_nothing() {
    let dir = TempDir::new("sink-missing");
    let wpctl = Wpctl::new(dir.join("no-wpctl"));
    assert_eq!(wpctl.read().await, None);
    assert!(!wpctl.set_volume(0.5).await);
    assert!(!wpctl.toggle_mute().await);
    assert!(!wpctl.set_mute(true).await);
}

#[tokio::test]
async fn a_tool_that_fails_reads_no_sink() {
    let dir = TempDir::new("sink-fails");
    install_script(&dir.join("wpctl"), "#!/bin/sh\nexit 1\n");
    let wpctl = Wpctl::new(dir.join("wpctl"));
    assert_eq!(wpctl.read().await, None);
    assert!(!wpctl.set_volume(0.5).await);
    assert!(!wpctl.toggle_mute().await);
}

#[tokio::test]
async fn a_sink_inspect_cannot_name_still_reads_its_volume() {
    let (dir, wpctl) = fake("sink-unnamed", "0.40");
    std::fs::remove_file(dir.join("inspect")).unwrap();
    assert_eq!(
        wpctl.read().await,
        Some(SinkState {
            volume: 0.4,
            muted: false,
            name: String::new(),
            target: None,
        })
    );
}

#[tokio::test]
async fn a_tool_that_does_not_answer_is_given_up_on_and_ended() {
    let dir = TempDir::new("sink-hangs");
    let pid_file = dir.join("pid");
    // The pid is written by the shell itself, before anything else runs, and `exec` makes the
    // sleeping process the one that was started.
    install_script(
        &dir.join("wpctl"),
        &format!(
            "#!/bin/sh\necho $$ > '{}'\nexec sleep 30\n",
            pid_file.display()
        ),
    );
    let wpctl = Wpctl::new(dir.join("wpctl")).with_timeout(Duration::from_secs(2));
    let started = Instant::now();
    let call = tokio::spawn(async move { wpctl.toggle_mute().await });
    // The call is judged once the fake is known to be running, however long it took to start.
    let deadline = Instant::now() + Duration::from_secs(5);
    let pid = loop {
        if let Ok(pid) = std::fs::read_to_string(&pid_file)
            && !pid.trim().is_empty()
        {
            break pid.trim().to_string();
        }
        assert!(Instant::now() < deadline, "the fake never wrote its pid");
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert!(!call.await.unwrap(), "a call that ran out of time took");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the call waited {:?}",
        started.elapsed()
    );
    let stat = format!("/proc/{pid}/stat");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        // Gone, or a zombie waiting to be reaped: either way no longer running.
        let state = std::fs::read_to_string(&stat)
            .ok()
            .and_then(|s| s.rsplit_once(") ").map(|(_, rest)| rest.chars().next()))
            .flatten();
        if matches!(state, None | Some('Z') | Some('X')) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the fake is still running: {state:?}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
