// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use dbus::arg::Variant;
use std::collections::HashMap;

fn seen(status: Status, played: u64) -> Seen {
    Seen { status, played }
}

#[test]
fn no_players_follow_none() {
    assert_eq!(choose(&[]), None);
}

#[test]
fn a_playing_player_is_followed_over_ones_that_are_not() {
    let players = [
        seen(Status::Paused, 5),
        seen(Status::Playing, 1),
        seen(Status::Stopped, 0),
    ];
    assert_eq!(choose(&players), Some(1));
}

#[test]
fn of_two_playing_the_one_that_started_last_is_followed() {
    let players = [seen(Status::Playing, 2), seen(Status::Playing, 3)];
    assert_eq!(choose(&players), Some(1));
    let players = [seen(Status::Playing, 3), seen(Status::Playing, 2)];
    assert_eq!(choose(&players), Some(0));
}

#[test]
fn with_none_playing_the_one_that_played_last_is_followed() {
    let players = [
        seen(Status::Paused, 1),
        seen(Status::Stopped, 4),
        seen(Status::Paused, 0),
    ];
    assert_eq!(choose(&players), Some(1));
}

#[test]
fn with_none_ever_seen_playing_the_first_is_followed() {
    let players = [
        seen(Status::Stopped, 0),
        seen(Status::Paused, 0),
        seen(Status::Stopped, 0),
    ];
    assert_eq!(choose(&players), Some(0));
}

#[test]
fn players_playing_since_the_same_count_follow_the_first() {
    let players = [
        seen(Status::Paused, 0),
        seen(Status::Playing, 1),
        seen(Status::Playing, 1),
    ];
    assert_eq!(choose(&players), Some(1));
}

#[test]
fn a_status_is_one_of_the_three_mpris_names() {
    assert_eq!(status_of(&"Playing".to_string()), Some(Status::Playing));
    assert_eq!(status_of(&"Paused".to_string()), Some(Status::Paused));
    assert_eq!(status_of(&"Stopped".to_string()), Some(Status::Stopped));
    assert_eq!(
        status_of(&Variant(Box::new("Playing".to_string()) as Box<dyn RefArg>)),
        Some(Status::Playing)
    );
    assert_eq!(status_of(&"playing".to_string()), None);
    assert_eq!(status_of(&7u32), None);
}

fn metadata(entries: Vec<(&str, Box<dyn RefArg>)>) -> Variant<Box<dyn RefArg>> {
    let map: HashMap<String, Variant<Box<dyn RefArg>>> = entries
        .into_iter()
        .map(|(k, v)| (k.to_string(), Variant(v)))
        .collect();
    Variant(Box::new(map))
}

#[test]
fn a_track_is_its_title_and_its_artists_joined() {
    let track = metadata(vec![
        ("xesam:title", Box::new("So What".to_string())),
        (
            "xesam:artist",
            Box::new(vec!["Miles Davis".to_string(), "John Coltrane".to_string()]),
        ),
    ]);
    let read = track_of(&track);
    assert_eq!(read.title, "So What");
    assert_eq!(read.artist, "Miles Davis, John Coltrane");
    assert!(read.genre.is_empty());
}

#[test]
fn track_reads_album_art_length_and_id() {
    let track = metadata(vec![
        ("xesam:title", Box::new("So What".to_string())),
        ("xesam:album", Box::new("Kind of Blue".to_string())),
        (
            "mpris:artUrl",
            Box::new("file:///home/me/.cache/cover.jpg".to_string()),
        ),
        ("mpris:length", Box::new(562_000_000i64)),
        (
            "mpris:trackid",
            Box::new(dbus::Path::new("/org/fake/track/1").unwrap()),
        ),
    ]);
    assert_eq!(
        track_of(&track),
        Track {
            title: "So What".to_string(),
            album: "Kind of Blue".to_string(),
            art_url: Some("file:///home/me/.cache/cover.jpg".to_string()),
            length_us: Some(562_000_000),
            track_id: Some("/org/fake/track/1".to_string()),
            ..Track::default()
        }
    );
}

#[test]
fn a_length_comes_as_any_integer_and_a_negative_one_is_none() {
    let unsigned = metadata(vec![("mpris:length", Box::new(7_000_000u64))]);
    assert_eq!(track_of(&unsigned).length_us, Some(7_000_000));
    let narrow = metadata(vec![("mpris:length", Box::new(9i32))]);
    assert_eq!(track_of(&narrow).length_us, Some(9));
    let negative = metadata(vec![("mpris:length", Box::new(-1i64))]);
    assert_eq!(track_of(&negative).length_us, None);
}

#[test]
fn an_empty_art_address_or_track_id_is_none() {
    let track = metadata(vec![
        ("mpris:artUrl", Box::new(String::new())),
        ("mpris:trackid", Box::new(String::new())),
    ]);
    let read = track_of(&track);
    assert_eq!((read.art_url, read.track_id), (None, None));
}

#[test]
fn a_track_id_given_as_a_plain_string_is_read_too() {
    let track = metadata(vec![(
        "mpris:trackid",
        Box::new("spotify:track:abc".to_string()),
    )]);
    assert_eq!(
        track_of(&track).track_id,
        Some("spotify:track:abc".to_string())
    );
}

#[test]
fn rate_reads_in_thousandths() {
    assert_eq!(rate_milli(Some(1.0)), 1000);
    assert_eq!(rate_milli(Some(0.5)), 500);
    assert_eq!(rate_milli(Some(1.25)), 1250);
    assert_eq!(rate_milli(None), 1000);
    assert_eq!(rate_milli(Some(0.0)), 1000);
    assert_eq!(rate_milli(Some(f64::NAN)), 1000);
}

#[test]
fn a_single_artist_given_as_a_string_is_read_too() {
    let track = metadata(vec![("xesam:artist", Box::new("Nina Simone".to_string()))]);
    let read = track_of(&track);
    assert_eq!(read.title, "");
    assert_eq!(read.artist, "Nina Simone");
}

#[test]
fn a_track_without_metadata_is_empty() {
    assert_eq!(track_of(&metadata(vec![])), Track::default());
    assert_eq!(track_of(&7u32), Track::default());
}

#[test]
fn a_player_without_an_identity_goes_by_its_bus_name() {
    assert_eq!(
        name_identity("org.mpris.MediaPlayer2.firefox.instance_1_23"),
        "firefox"
    );
    assert_eq!(name_identity("org.mpris.MediaPlayer2.vlc"), "vlc");
}

#[test]
fn the_session_bus_is_the_one_the_environment_names() {
    assert_eq!(
        session_address(
            Some(OsStr::new("unix:path=/run/user/1000/bus")),
            Some(OsStr::new("/nowhere"))
        ),
        Some("unix:path=/run/user/1000/bus".to_string())
    );
}

#[test]
fn without_one_named_the_runtime_directory_s_socket_is_used_when_it_is_there() {
    let dir = std::env::temp_dir().join(format!(
        "svolder-host-session-address-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let found = session_address(Some(OsStr::new("")), Some(dir.as_os_str()));
    std::fs::write(dir.join("bus"), "").unwrap();
    let there = session_address(None, Some(dir.as_os_str()));
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(found, None);
    assert_eq!(there, Some(format!("unix:path={}/bus", dir.display())));
}

#[test]
fn with_neither_there_is_no_session_bus() {
    assert_eq!(session_address(None, None), None);
}

#[test]
fn the_buttons_call_the_four_mpris_transport_methods() {
    let methods: Vec<&str> = Button::ALL.iter().map(|b| b.method()).collect();
    assert_eq!(methods, ["Previous", "Play", "Pause", "Next"]);
}

#[test]
fn a_genre_is_read_from_an_array_or_a_plain_string() {
    let all = metadata(vec![(
        "xesam:genre",
        Box::new(vec!["Rock".to_string(), "Indie".to_string()]),
    )]);
    assert_eq!(track_of(&all).genre, ["Rock", "Indie"]);
    let one = metadata(vec![("xesam:genre", Box::new("Jazz".to_string()))]);
    assert_eq!(track_of(&one).genre, ["Jazz"]);
    assert!(track_of(&metadata(vec![])).genre.is_empty());
}
