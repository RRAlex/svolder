// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Bluetooth adapter row and its list: what the row says, how the list is drawn, its keys
//! and its mouse.

use svolder_ble::adapters::AdapterInfo;

use super::*;
use crate::commands::adapters::{Row, Seen, Survey};

fn row(name: &str, address: &str, rssi: Option<i16>, in_use: bool) -> Row {
    Row {
        adapter: AdapterInfo {
            name: name.into(),
            address: address.into(),
            usb: Some("UB500 (2357:0604)".into()),
        },
        in_use,
        heard: vec![(
            "BF3".into(),
            rssi.map_or(Seen::NotHeard, |rssi| Seen::Heard(Some(rssi))),
        )],
        error: None,
    }
}

fn survey(rows: Vec<Row>) -> Survey {
    Survey { rows, note: None }
}

/// The two adapters of the approved mockup.
fn two() -> Vec<Row> {
    vec![
        row("hci0", "AA:BB:CC:DD:EE:01", Some(-62), true),
        row("hci2", "AA:BB:CC:DD:EE:02", Some(-74), false),
    ]
}

/// The Behaviour category's adapter row with Enter pressed and the scan answered.
fn list_app() -> App {
    let mut app = app();
    select(&mut app, Setting::Adapter);
    assert_eq!(press(&mut app, KeyCode::Enter), vec![Intent::ListAdapters]);
    app.overlays.conn.adapters_found(Ok(survey(two())));
    app
}

/// The row says which adapter the file names: `first` for none, a name as it is, an address by
/// its last three pairs.
#[test]
fn the_row_says_which_adapter_the_file_names() {
    let mut app = app();
    assert_eq!(Setting::Adapter.value(&app), "first");
    app.overlays.conn.adapter = Some("hci2".into());
    assert_eq!(Setting::Adapter.value(&app), "hci2");
    app.overlays.conn.adapter = Some("AA:BB:CC:DD:EE:01".into());
    assert_eq!(Setting::Adapter.value(&app), "…DD:EE:01");
}

/// Enter opens the list and asks for the scan; the list waits for it.
#[test]
fn enter_opens_the_list_and_it_waits_for_the_scan() {
    let mut app = app();
    select(&mut app, Setting::Adapter);
    assert_eq!(press(&mut app, KeyCode::Enter), vec![Intent::ListAdapters]);
    assert!(matches!(app.overlays.conn.modal, Some(Modal::Adapters(_))));
    assert!(
        shown(&app).contains("listening on every adapter"),
        "{}",
        shown(&app)
    );
    // Enter before the scan came back picks nothing.
    assert_eq!(press(&mut app, KeyCode::Enter), Vec::<Intent>::new());
    assert!(matches!(app.overlays.conn.modal, Some(Modal::Adapters(_))));
}

/// The list draws the approved table: hciN, address, USB product, the signal of each configured
/// device, the cursor on the adapter in use.
#[test]
fn the_list_is_the_approved_table() {
    let app = list_app();
    let rows = box_rows(&app);
    let line = |needle: &str| {
        rows.iter()
            .find(|r| r.contains(needle))
            .unwrap_or_else(|| panic!("{needle} is not drawn: {rows:#?}"))
            .clone()
    };
    let first = line("hci0");
    assert!(
        first.contains("▶ hci0  AA:BB:CC:DD:EE:01  UB500 (2357:0604)  BF3 −62 dBm  ✓ in use"),
        "{first}"
    );
    let second = line("hci2");
    assert!(
        second.contains("  hci2  AA:BB:CC:DD:EE:02  UB500 (2357:0604)  BF3 −74 dBm"),
        "{second}"
    );
    assert!(!second.contains("in use"), "{second}");
    assert!(shown(&app).contains("Bluetooth adapter"));
    assert!(shown(&app).contains("Enter"), "{}", shown(&app));
}

/// ↓ moves the cursor, Enter saves the address of the adapter under it and closes the list.
#[test]
fn enter_saves_the_address_of_the_adapter_under_the_cursor() {
    let mut app = list_app();
    press(&mut app, KeyCode::Down);
    let intents = press(&mut app, KeyCode::Enter);
    assert_eq!(
        intents,
        vec![Intent::PickAdapter {
            address: "AA:BB:CC:DD:EE:02".into(),
            name: "hci2".into()
        }]
    );
    assert_eq!(app.overlays.conn.modal, None);
}

/// The cursor starts on the adapter in use and stops at the ends; Esc closes with no pick.
#[test]
fn the_cursor_starts_on_the_one_in_use_and_esc_picks_nothing() {
    let mut app = app();
    select(&mut app, Setting::Adapter);
    press(&mut app, KeyCode::Enter);
    app.overlays.conn.adapters_found(Ok(survey(vec![
        row("hci0", "AA:BB:CC:DD:EE:01", Some(-62), false),
        row("hci2", "AA:BB:CC:DD:EE:02", Some(-74), true),
    ])));
    let Some(Modal::Adapters(list)) = &app.overlays.conn.modal else {
        panic!("the list is open")
    };
    assert_eq!(list.at, 1);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    let Some(Modal::Adapters(list)) = &app.overlays.conn.modal else {
        panic!("the list is open")
    };
    assert_eq!(list.at, 1);
    assert_eq!(press(&mut app, KeyCode::Esc), Vec::<Intent>::new());
    assert_eq!(app.overlays.conn.modal, None);
}

/// A scan that failed says why in the list, and Enter picks nothing.
#[test]
fn a_failed_scan_is_said_in_the_list() {
    let mut app = app();
    select(&mut app, Setting::Adapter);
    press(&mut app, KeyCode::Enter);
    app.overlays
        .conn
        .adapters_found(Err("no Bluetooth adapter found".into()));
    assert!(
        shown(&app).contains("no Bluetooth adapter found"),
        "{}",
        shown(&app)
    );
    assert_eq!(press(&mut app, KeyCode::Enter), Vec::<Intent>::new());
}

/// A click on an adapter's line selects it, as elsewhere in Settings; a second click on the
/// selected line saves it.
#[test]
fn a_click_on_a_line_selects_and_a_second_saves() {
    let mut app = list_app();
    let (x, y) = find_at(&app, 80, 24, "hci2");
    assert_eq!(clicked(&app, x, y), Some(MouseAction::AdapterAt(1)));
    press(&mut app, KeyCode::Down);
    assert_eq!(
        clicked(&app, x, y),
        Some(MouseAction::Intent(Intent::PickAdapter {
            address: "AA:BB:CC:DD:EE:02".into(),
            name: "hci2".into()
        }))
    );
}

/// A note about the adapter that was named is said under the table.
#[test]
fn a_note_is_said_under_the_table() {
    let mut app = app();
    select(&mut app, Setting::Adapter);
    press(&mut app, KeyCode::Enter);
    app.overlays.conn.adapters_found(Ok(Survey {
        rows: two(),
        note: Some(
            "the configured adapter matches no adapter: no adapter has the address X".into(),
        ),
    }));
    assert!(
        shown(&app).contains("matches no adapter"),
        "{}",
        shown(&app)
    );
}

/// The row has no arrows, so a click on it once it is selected opens the list; a first click
/// only selects.
#[test]
fn a_second_click_on_the_row_opens_the_list() {
    let mut app = app();
    app.overlays.settings = Some(SettingsBox {
        category: 2,
        row: 0,
    });
    let (x, y) = find_at(&app, 80, 24, "Bluetooth adapter");
    assert!(matches!(
        clicked(&app, x, y),
        Some(MouseAction::Setting(SettingsBox {
            category: 2,
            row: 8
        }))
    ));
    select(&mut app, Setting::Adapter);
    assert_eq!(clicked(&app, x, y), Some(MouseAction::Key(KeyCode::Enter)));
}
