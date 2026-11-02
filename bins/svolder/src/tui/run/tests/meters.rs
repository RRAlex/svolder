// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The levels an amp streams, carried into the terminal title.

use super::*;

/// One meter reading on `id`, as a backend hands it over.
fn meter_reading(id: &str, raw: i64) -> ValueObject {
    ValueObject {
        heard_at_ms: None,
        id: id.into(),
        key: None,
        label: "Meter".into(),
        value_type: "u16".into(),
        raw: json!(raw),
        display: raw.to_string(),
        value: None,
        unit: None,
    }
}

/// An app over a profile whose meters carry a scale, with the left channel at the top of it
/// and the right at its idle floor: the fake device is a DAC, and a DAC streams no meters.
fn metered_app(backend: &Backend) -> App {
    let mut app = app_for(backend);
    app.profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get("fake-amp")
        .expect("fake-amp is built in")
        .clone();
    for (id, raw) in [("20.1", 1_023), ("20.2", 8)] {
        app.apply(Update::Status(meter_reading(id, raw)), Instant::now());
    }
    app
}

/// The terminal's title carries the two channels while the switch is on, and the device's
/// name alone while it is off.
#[tokio::test(start_paused = true)]
async fn the_title_carries_the_levels_the_device_streams() {
    for (on, wanted) in [
        (true, "(█████░░░░░) svolder · fake"),
        (false, "svolder · fake"),
    ] {
        let (backend, _gate, _) = fake(SHORT_DEADLINE).await;
        let app = metered_app(&backend);
        let (recorder, _frames) = Recorder::new(SIZE.0, SIZE.1);
        let mut terminal = Terminal::new(recorder).expect("a test terminal opens");
        let mut mouse = SpyMouse::default();
        let (mut title, titles) = spied_title();
        let mut look = look();
        look.tui.title_meter = on;

        let end = event_loop(LoopParts {
            opener: &opener_over(never_opens()),
            adder: &nothing_to_add(),
            watching: watching(backend, app, None),
            terminal: &mut terminal,
            events: quit_after(Duration::from_millis(400)),
            mouse: &mut mouse,
            title: &mut title,
            look,
        })
        .await;

        assert_eq!(end.code, 0, "{:?}", end.why);
        let written = titles.lock().expect("the titles mutex").clone();
        assert_eq!(
            written.first().map(String::as_str),
            Some(wanted),
            "the title with the switch {on}: {written:?}"
        );
        end.watching.close().await;
    }
}
