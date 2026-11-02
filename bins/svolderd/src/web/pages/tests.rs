// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeSet;

use super::*;

/// The cells `class` fills in the wordmark SVG, as (column, row): each `M{x} {y}h{w}v2h-{w}z`
/// is `w` cells of row `y / 2` from column `x`.
fn cells(class: &str) -> BTreeSet<(usize, usize)> {
    let marker = format!("class=\"{class}\" d=\"");
    let start = WORDMARK.find(&marker).expect("the path") + marker.len();
    let path = &WORDMARK[start..start + WORDMARK[start..].find('"').expect("the end")];
    let mut out = BTreeSet::new();
    for rect in path.split('M').filter(|r| !r.is_empty()) {
        let (x, rest) = rect.split_once(' ').expect("x");
        let (y, rest) = rest.split_once('h').expect("y");
        let w: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let (x, y, w): (usize, usize, usize) =
            (x.parse().unwrap(), y.parse().unwrap(), w.parse().unwrap());
        for column in x..x + w {
            out.insert((column, y / 2));
        }
    }
    out
}

/// The web wordmark is the TUI's large wordmark, cell for cell: the face `▓`, its first shadow
/// `▒` and its second `░`.
#[test]
fn the_wordmark_is_the_tuis_large_wordmark_cell_for_cell() {
    let art = include_str!("../../../../svolder/assets/wordmark/large.txt");
    for (class, glyph) in [("wm1", '▓'), ("wm2", '▒'), ("wm3", '░')] {
        let want: BTreeSet<(usize, usize)> = art
            .lines()
            .enumerate()
            .flat_map(|(row, line)| {
                line.chars()
                    .enumerate()
                    .filter(move |(_, c)| *c == glyph)
                    .map(move |(column, _)| (column, row))
            })
            .collect();
        assert_eq!(cells(class), want, "{class}");
    }
}

/// The device list the header's switcher is drawn from names each device by its percent-encoded
/// URL and marks the one shown (`ember-parts.mjs` holds the case `the_switcher_links_a_device_by_its_encoded_name` for the links the theme
/// draws from it).
#[tokio::test]
async fn the_model_lists_each_device_by_its_encoded_name() {
    use crate::web::test_support::{call, fake_daemon_fleet, get, open, page_model, web};
    let fleet = fake_daemon_fleet(&[
        ("gungnir2-v2", "Living room/DAC", "0102030405060708"),
        ("gungnir2-v2", "Desk", "1112131415161718"),
    ])
    .await;
    let web = web(open(), std::sync::Arc::clone(&fleet.fleet));
    let page = call(&web, get("/?device=Living%20room%2FDAC")).await;
    let model = page_model(&page.body);
    let devices: Vec<(&str, &str)> = model["fleet"]["devices"]
        .as_array()
        .expect("the devices")
        .iter()
        .map(|d| (d["url"].as_str().unwrap(), d["name"].as_str().unwrap()))
        .collect();
    assert!(
        devices.contains(&("Living%20room%2FDAC", "Living room/DAC")),
        "{devices:?}"
    );
    assert!(devices.contains(&("Desk", "Desk")), "{devices:?}");
    assert_eq!(model["shown"], "Living%20room%2FDAC");
    fleet.stop().await;
}

/// The model gives each device the name a switcher link needs for the script that moves the page
/// to it in place, and names the one shown (`ember-parts.mjs` holds the case `the_switcher_swaps_the_screen_through_htmx` for the link
/// the theme draws: its `href` for a page without script, and the name it carries).
#[tokio::test]
async fn the_model_names_each_device_for_the_switcher_script() {
    use crate::web::test_support::{call, fake_daemon_fleet, get, open, page_model, web};
    let fleet = fake_daemon_fleet(&[
        ("gungnir2-v2", "Desk", "0102030405060708"),
        ("gungnir2-v2", "Living room", "1112131415161718"),
    ])
    .await;
    let web = web(open(), std::sync::Arc::clone(&fleet.fleet));
    let page = call(&web, get("/?device=Desk")).await;
    let model = page_model(&page.body);
    let living = model["fleet"]["devices"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["name"] == "Living room")
        .expect("the other device");
    assert_eq!(living["url"], "Living%20room");
    assert_eq!(model["shown"], "Desk");
    fleet.stop().await;
}

/// Every element id a device's model makes is a CSS identifier whatever the device is called: the
/// ids are built from the model's `dom`, the name in hex (`ember-parts.mjs` holds the case `every_id_of_a_screen_is_a_css_identifier_for_any_device_name`
/// for the ids of every screen it draws).
#[tokio::test]
async fn the_ids_a_device_makes_are_css_identifiers_for_any_name() {
    use crate::web::test_support::{call, fake_daemon, get, open, web};
    let name = "Living room/DAC.v2 & co";
    let dac = fake_daemon("gungnir2-v2", name).await;
    let web = web(open(), std::sync::Arc::clone(&dac.fleet));
    let path = format!("/api/v1/devices/{}/model", crate::web::view::segment(name));
    let reply = call(&web, get(&path)).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    let dom = said["device"]["dom"].as_str().expect("the device's dom");
    assert!(
        !dom.is_empty() && dom.chars().all(|c| c.is_ascii_hexdigit()),
        "{dom}"
    );
    let keys: Vec<&str> = said["device"]["controls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["key"].as_str().unwrap())
        .collect();
    assert!(keys.contains(&"balance"), "{keys:?}");
    for key in keys {
        let id = format!("slider-{dom}-{key}");
        let mut chars = id.chars();
        let ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        assert!(ok, "{id}");
    }
    dac.stop().await;
}

/// A guest's model is inert, so the script does not enable the knob when a reading arrives
/// (`ember-parts.mjs` holds the case `a_guests_knob_is_marked_inert` for the mark the knob carries).
#[tokio::test]
async fn a_guests_model_is_inert_and_still_has_its_knob() {
    use crate::web::test_support::{call, fake_daemon, get, page_model, web_with};
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web_with(
        svolder_ipc::config::WebConfig {
            enabled: true,
            guest: svolder_ipc::config::WebGuest::Read,
            ..Default::default()
        },
        std::sync::Arc::clone(&dac.fleet),
        Some("a-test-token-0123456789abcdefghijklmn"),
    );
    let reply = call(&web, get("/")).await;
    let model = page_model(&reply.body);
    assert_eq!(model["device"]["inert"], true, "{model}");
    assert!(model["device"]["volume"].is_object(), "{model}");
    dac.stop().await;
}

/// A negative level is a string of the model that starts with its minus, once, so the page can
/// hang it in a span of its own beside the digits and keep the digits centred
/// (`ember-parts.mjs` holds the case `a_negative_level_hangs_its_minus_in_its_own_span` for the markup and the script's repaint).
#[tokio::test]
async fn a_negative_level_reads_with_one_leading_minus() {
    use crate::web::test_support::{call, fake_daemon, get, open, web};
    let dac = fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), std::sync::Arc::clone(&dac.fleet));
    let reply = call(&web, get("/api/v1/devices/dac/model")).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let said: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    let readout = said["device"]["volume"]["readout"]
        .as_str()
        .expect("the readout");
    assert!(readout.starts_with('−'), "{readout}");
    assert_eq!(readout.matches('−').count(), 1, "{readout}");
    dac.stop().await;
}
