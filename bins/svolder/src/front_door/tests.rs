// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_ipc::Config;

use super::*;

/// The daemon is looked at before the front door chooses, once, whichever way it chooses, and
/// what the look said travels with the choice.
#[tokio::test]
async fn the_front_door_looks_at_the_daemon_once_before_it_chooses() {
    let none = Config::parse("schema = 1\n").expect("an empty configuration");
    let one =
        Config::parse("schema = 1\n[[device]]\nname = \"dac\"\nudid = \"0102030405060708\"\n")
            .expect("a configuration with a device");
    for (config, want) in [
        (&none, Door::Wizard(Some("svolderd re-read x".into()))),
        (&one, Door::Tui(Some("svolderd re-read x".into()))),
    ] {
        let looks = std::cell::Cell::new(0);
        let door = choose_door(config, async {
            looks.set(looks.get() + 1);
            Some("svolderd re-read x".to_string())
        })
        .await;
        assert_eq!(door, want);
        assert_eq!(looks.get(), 1);
    }
}
