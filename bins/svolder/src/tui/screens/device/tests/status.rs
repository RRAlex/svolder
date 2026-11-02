// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Status pane: the status line, its readouts, faults and advice.

use super::*;

#[test]
fn an_active_fault_flashes_the_header_and_the_status_pane_offers_its_advice() {
    let mut app = faulty_mockup_app();
    hear(
        &mut app,
        vec![vo(
            "230.1",
            Some("fault_overcurrent"),
            json!(true),
            "active",
        )],
    );
    let rows = text(&screen(&app));
    assert_eq!(
        rows[0],
        " svolder  Gungnir 2 · fw 3 · 0102…08  ● connected ⚡  over-current mouse:on 14:02",
        "the header gains the fault's glyph and lower-case name"
    );
    assert_eq!(
        rows[21],
        "│ Sample rate 48 kHz   fw 3   Faults: Over-current                             │",
        "the Status pane names the active fault as the profile writes it"
    );

    app.notices.show_advice = true;
    let rows = text(&screen(&app));
    assert_eq!(
        rows[22],
        "│ Advice: Power-cycle the device. Check for shorts in speakers or headphones i…│",
        "the advice line replaces the Raw line and is cut to fit the pane"
    );
}

/// A failure the loop reported puts what happened on the key line and what to do about it
/// on the advice line, so the step is one key away from the words that need it.
#[test]
fn the_advice_line_offers_the_step_a_failure_named() {
    let mut app = mockup_app();
    app.notify_failure(&Failure::of(Kind::Dropped, "device did not come back"));
    assert_eq!(
        app.toast().map(|toast| toast.text.as_str()),
        Some("device did not come back")
    );
    app.dismiss_toast();
    app.notices.show_advice = true;
    let rows = text(&screen(&app));
    assert_eq!(
        rows[22],
        "│ Advice: If it does not come back, run `svolder doctor`                       │",
        "the advice line replaces the Raw line with the failure's own step"
    );
}

/// With no readout or firmware reading heard yet, the Status pane's first line has nothing
/// to put before `Faults:` — one leading space, not the extra gap a segment would otherwise
/// leave behind.
#[test]
fn the_status_line_has_one_leading_space_with_no_readout_or_firmware_yet() {
    let app = app_on(with_overcurrent(profile_named("gungnir2-v2")));
    let rows = text(&screen(&app));
    assert_eq!(rows[21], pane_row(" Faults: none"));
}

/// A profile with no fault control writes no `Faults:` segment; one with a fault control does.
#[test]
fn only_a_profile_with_a_fault_control_writes_the_faults_segment() {
    let dac = text(&screen(&app_on(profile_named("gungnir2-v2"))));
    assert!(!dac[21].contains("Faults"), "{}", dac[21]);
    let amp = text(&screen(&app_on(profile_named("aegir2"))));
    assert!(amp[21].contains("Faults: none"), "{}", amp[21]);
}

/// With nothing heard yet, the Status pane's second line has no unnamed reading to list.
#[test]
fn the_status_lines_second_row_says_raw_none_with_nothing_unexplained() {
    let app = app_on(profile_named("gungnir2-v2"));
    let rows = text(&screen(&app));
    assert_eq!(rows[22], pane_row(" Raw: none"));
}

/// Two faults active at once are both named, in the order the profile lists their functions.
#[test]
fn two_faults_active_at_once_are_both_named() {
    let mut app = app_on(profile_named("amp-stereo"));
    hear(
        &mut app,
        vec![
            vo("230.1", Some("fault_overcurrent"), json!(true), "active"),
            vo("230.2", Some("fault_overtemp"), json!(true), "active"),
        ],
    );
    let rows = text(&screen(&app));
    assert!(
        rows.iter()
            .any(|row| row.contains("Faults: Over-current, Over-temperature")),
        "expected both faults named on one line, got: {rows:?}"
    );
}

#[test]
fn a_readout_the_profile_names_on_the_firmware_family_is_drawn_once() {
    let mut profile = profile_named("gungnir2-v2");
    profile.controls.push(Control {
        id: "210.2.1".parse().expect("a valid function id"),
        key: "main_pic_fw".into(),
        label: "Main PIC fw".into(),
        kind: ControlKind::Readout,
        value_type: ValueType::U8,
        values: vec![],
        range: None,
        center: None,
        unit: None,
        convert: None,
        channel: None,
        floor: None,
        tick: None,
        log_scale: None,
        advice: None,
        only_while: None,
        auto_off: false,
        output_gain: false,
        mutes_output: false,
        powers_output: false,
        asks_on_change: false,
        reads: Vec::new(),
    });
    let mut app = app_on(profile);
    hear(&mut app, vec![vo("210.2.1", None, json!(3), "3")]);
    let rows = text(&screen(&app));
    assert_eq!(
        rows[21],
        pane_row(" Main PIC fw 3"),
        "a firmware readout the profile names is not also listed as a bare readout"
    );
}

/// A boolean readout is drawn as its bare label while it reads true and written out with the
/// web's false word while it reads false, in the address-sorted order the numeric readouts keep.
#[test]
fn a_true_boolean_readout_is_drawn_as_its_label_and_a_false_one_is_written_out() {
    let mut app = app_on(profile_named("gungnir2-v2"));
    hear(
        &mut app,
        vec![
            vo("6", Some("sample_rate"), json!(2), "2 (48 kHz)"),
            vo("7.1", Some("usb1_connected"), json!(true), "true"),
            vo("7.2", Some("usb2_connected"), json!(false), "false"),
            vo("8", Some("spdif_locked"), json!(true), "true"),
        ],
    );
    let rows = text(&screen(&app));
    assert_eq!(
        rows[21],
        pane_row(" Sample rate 48 kHz   USB 1 connected   USB 2 not connected   S/PDIF locked")
    );
}

#[test]
fn the_headphone_output_readout_follows_its_boolean() {
    let mut app = app_on(profile_named("kara"));
    hear(
        &mut app,
        vec![vo("5", Some("headphone"), json!(true), "true")],
    );
    let rows = text(&screen(&app));
    assert!(
        rows[21].contains("Headphone output"),
        "a true headphone readout was not drawn: {:?}",
        rows[21]
    );
    hear(
        &mut app,
        vec![vo("5", Some("headphone"), json!(false), "false")],
    );
    let rows = text(&screen(&app));
    assert!(
        rows[21].contains("Headphone output no"),
        "a false headphone readout was not written out: {:?}",
        rows[21]
    );
}

/// A profile with a warm-up readout that depends on the power toggle.
fn warmup_profile() -> Profile {
    let source = "schema = 1\nname = \"t\"\n\
        [[control]]\nid = [4]\nkey = \"warming_up\"\nlabel = \"Warming up\"\nkind = \"readout\"\n\
        value_type = \"bool\"\nonly_while = \"power\"\n\
        [[control]]\nid = [10]\nkey = \"power\"\nlabel = \"Power\"\nkind = \"toggle\"\n";
    Registry::from_sources(vec![("t".into(), source.into())])
        .expect("the test profile loads")
        .get("t")
        .expect("the test profile exists")
        .clone()
}

/// A boolean readout the device sends as an integer is its bare label for 1 and absent for
/// 0 and 2.
#[test]
fn an_integer_boolean_readout_is_drawn_as_its_label_only_for_one() {
    for (raw, drawn) in [(0, false), (1, true), (2, false)] {
        let mut app = app_on(profile_named("gungnir2-v2"));
        hear(
            &mut app,
            vec![vo("8", Some("spdif_locked"), json!(raw), "whatever")],
        );
        let rows = text(&screen(&app));
        assert_eq!(
            rows.iter().any(|row| row.contains("S/PDIF locked")),
            drawn,
            "{raw}: {rows:?}"
        );
    }
}

/// A readout with `only_while` reads true only while the control it names also reads true, and
/// is written out false the rest of the time.
#[test]
fn a_gated_readout_reads_true_only_while_its_control_reads_true() {
    for (power, said) in [(true, "Warming up"), (false, "Warming up no")] {
        let mut app = app_on(warmup_profile());
        hear(
            &mut app,
            vec![
                vo("4", Some("warming_up"), json!(true), "true"),
                vo("10", Some("power"), json!(power), "on"),
            ],
        );
        let rows = text(&screen(&app));
        assert_eq!(rows[21], pane_row(&format!(" {said}")), "power {power}");
    }
}

/// The Status line follows the control a readout depends on: the readout appears when the
/// control turns on and goes when it turns off, with no new status for the readout itself.
#[test]
fn a_gated_readout_follows_its_control_with_no_new_status_of_its_own() {
    let mut app = app_on(warmup_profile());
    let drawn = |app: &App| text(&screen(app))[21] == pane_row(" Warming up");
    hear(
        &mut app,
        vec![
            vo("4", Some("warming_up"), json!(true), "true"),
            vo("10", Some("power"), json!(false), "off"),
        ],
    );
    assert!(!drawn(&app));
    hear(&mut app, vec![vo("10", Some("power"), json!(true), "on")]);
    assert!(drawn(&app));
    hear(&mut app, vec![vo("10", Some("power"), json!(false), "off")]);
    assert!(!drawn(&app));
}

/// A boolean readout is explained by its control, so the Raw line leaves it out.
#[test]
fn a_boolean_readout_is_not_listed_on_the_raw_line() {
    let mut app = app_on(profile_named("gungnir2-v2"));
    hear(
        &mut app,
        vec![vo("8", Some("spdif_locked"), json!(true), "true")],
    );
    let rows = text(&screen(&app));
    assert_eq!(rows[22], pane_row(" Raw: none"));
    hear(
        &mut app,
        vec![vo("8", Some("spdif_locked"), json!(false), "false")],
    );
    let rows = text(&screen(&app));
    assert_eq!(rows[22], pane_row(" Raw: none"));
}

/// The first Status line of `app` at `width` columns, as the text it draws.
fn first_status_line(app: &App, width: usize) -> String {
    let theme = Theme::new(false);
    super::super::status::status_lines(app, &theme, width, crate::tui::layout_tier::Tier::default())
        [0]
    .spans
    .iter()
    .map(|span| span.content.as_ref())
    .collect()
}

/// A first line with no room left for fault text writes neither the `Faults: ` word nor a
/// fault line for the animation layer to pulse: the header's fault mark carries the faults
/// there. With room the line is as it always was.
#[test]
fn a_status_line_with_no_room_for_faults_writes_no_faults_word_and_no_pulse_line() {
    let mut app = faulty_mockup_app();
    hear(
        &mut app,
        vec![vo(
            "230.1",
            Some("fault_overcurrent"),
            json!(true),
            "active",
        )],
    );
    let head = " Sample rate 48 kHz   fw 3   Faults: ";
    let wide = first_status_line(&app, 60);
    assert!(
        wide.starts_with(head) && wide.contains("Over-current"),
        "{wide:?}"
    );
    for width in [head.chars().count(), 32, 20] {
        let narrow = first_status_line(&app, width);
        assert!(!narrow.contains("Faults"), "{width}: {narrow:?}");
    }
    assert_eq!(first_status_line(&app, 37), " Sample rate 48 kHz   fw 3   ");
    assert_eq!(first_status_line(&app, 20), " Sample rate 48 kHz…");
    let fits = frame(&app, 126, 30);
    assert!(
        text(&fits)
            .iter()
            .any(|row| row.contains("Faults: Over-current"))
    );
    assert!(fault_line(Rect::new(0, 0, 126, 30), &app).is_some());
    assert_eq!(
        fault_line(Rect::new(0, 0, 34, 30), &app),
        None,
        "a pane too narrow for the fault text has no line to pulse"
    );
}

/// The Status pane's top border row of `buffer`.
fn status_border(buffer: &Buffer) -> String {
    text(buffer)
        .into_iter()
        .find(|row| row.starts_with("┌ Status"))
        .expect("the Status pane's top border")
}

/// `advice` is on the Status border of every device, since `a` always does something, and is
/// never dim.
#[test]
fn advice_is_on_the_status_border_of_every_device() {
    let dac = status_border(&screen(&mockup_app()));
    assert!(dac.ends_with(" advice ┐"), "a DAC's Status border: {dac}");
    let buffer = screen(&mockup_app());
    let row = text(&buffer)
        .iter()
        .position(|row| row.starts_with("┌ Status"))
        .expect("the Status border row");
    for x in 71..77u16 {
        let cell = buffer.cell((x, row as u16)).expect("a cell of the word");
        assert!(
            !cell.modifier.contains(Modifier::DIM),
            "dim at {x}: {cell:?}"
        );
    }
    let amp = status_border(&screen(&faulty_mockup_app()));
    assert!(amp.ends_with(" advice ┐"), "an amp's Status border: {amp}");
}

/// The wall clock the Device row's cases hold, in unix milliseconds.
const WALL: u64 = 1_800_000_000_000;

/// The facts a daemon lists for a Bifrost 3: product, product id, profile and a `signal` heard
/// `secs` ago as an advert, or live when `live`.
fn bifrost_facts(signal: Option<(i16, bool, u64)>) -> crate::backend::DeviceRow {
    crate::backend::DeviceRow {
        name: "gungnir".into(),
        product: Some("Bifrost 3".into()),
        profile: Some("bifrost3".into()),
        product_id: Some(10),
        signal: signal.map(|(dbm, live, secs)| svolder_ipc::Signal {
            dbm,
            source: match live {
                true => svolder_ipc::SignalSource::Live,
                false => svolder_ipc::SignalSource::Advert,
            },
            heard_at_ms: WALL - secs * 1000,
        }),
        ..crate::backend::DeviceRow::default()
    }
}

/// The Status line `index` of `app` at `width` columns, as spans.
fn status_line_spans(app: &App, width: usize, index: usize) -> Vec<Span<'static>> {
    let theme = Theme::new(false);
    super::super::status::status_lines(app, &theme, width, crate::tui::layout_tier::Tier::default())
        [index]
        .spans
        .clone()
}

/// The Status line `index` of `app` at `width` columns, as the text it draws.
fn status_line_text(app: &App, width: usize, index: usize) -> String {
    status_line_spans(app, width, index)
        .iter()
        .map(|span| span.content.as_ref())
        .collect()
}

/// With facts, the Status pane gains a `Device` row under its two lines: the product and its id,
/// the profile and the signal, worded as everywhere else; the signal is left out when none was
/// heard, and with no facts there is no row and the pane is as it was.
#[test]
fn status_draws_a_device_row_with_profile_product_and_signal() {
    crate::tui::widgets::graph::hold_wall_ms(WALL);
    let mut app = mockup_app();
    let before = text(&screen(&app));
    app.session.facts = Some(bifrost_facts(Some((-62, false, 180))));
    let rows = text(&screen(&app));
    assert_eq!(
        rows[22],
        pane_row(
            " Bifrost 3 (product 10) \u{b7} profile bifrost3 \u{b7} \u{2212}62 dBm advert, 3 min ago"
        )
    );
    assert_eq!(rows[20], pane_row(" Sample rate 48 kHz   fw 3"));
    app.session.facts = Some(bifrost_facts(Some((-58, true, 2))));
    assert_eq!(
        text(&screen(&app))[22],
        pane_row(" Bifrost 3 (product 10) \u{b7} profile bifrost3 \u{b7} \u{2212}58 dBm live")
    );
    app.session.facts = Some(bifrost_facts(None));
    assert_eq!(
        text(&screen(&app))[22],
        pane_row(" Bifrost 3 (product 10) \u{b7} profile bifrost3")
    );
    app.session.facts = None;
    assert_eq!(text(&screen(&app)), before, "no facts, no row");
}

/// A Device row too wide for its pane drops what is on its right whole, the signal first, then
/// the profile; the product alone is cut with an ellipsis when even it does not fit.
#[test]
fn device_row_drops_signal_first_when_narrow() {
    crate::tui::widgets::graph::hold_wall_ms(WALL);
    let mut app = mockup_app();
    app.session.facts = Some(bifrost_facts(Some((-62, false, 180))));
    let row = |width| status_line_text(&app, width, 2);
    assert_eq!(
        row(78),
        " Bifrost 3 (product 10) \u{b7} profile bifrost3 \u{b7} \u{2212}62 dBm advert, 3 min ago"
    );
    assert_eq!(row(60), " Bifrost 3 (product 10) \u{b7} profile bifrost3");
    assert_eq!(row(42), " Bifrost 3 (product 10) \u{b7} profile bifrost3");
    assert_eq!(row(41), " Bifrost 3 (product 10)");
    assert_eq!(row(23), " Bifrost 3 (product 10)");
    let cut = row(12);
    assert!(
        cut.ends_with('\u{2026}') && widgets::text::cells(&cut) <= 12,
        "{cut:?}"
    );
}

/// A boolean readout is written in both states: the label as it is while true, the web's false
/// word while false (`not connected`, `unlocked`, or `no`), the false ones dim.
#[test]
fn false_bool_readouts_are_written_out() {
    let mut app = app_on(profile_named("gungnir2-v2"));
    hear(
        &mut app,
        vec![
            vo("7.1", Some("usb1_connected"), json!(true), "true"),
            vo("7.2", Some("usb2_connected"), json!(false), "false"),
            vo("8", Some("spdif_locked"), json!(false), "false"),
        ],
    );
    assert_eq!(
        status_line_text(&app, 78, 0),
        " USB 1 connected   USB 2 not connected   S/PDIF unlocked"
    );
    let theme = Theme::new(false);
    let spans = status_line_spans(&app, 78, 0);
    let style_of = |wanted: &str| {
        spans
            .iter()
            .find(|span| span.content.contains(wanted))
            .unwrap_or_else(|| panic!("no span carries {wanted:?}: {spans:?}"))
            .style
    };
    assert_eq!(style_of("USB 2 not connected"), theme.style(Role::Dim));
    assert_eq!(style_of("S/PDIF unlocked"), theme.style(Role::Dim));
    assert_ne!(style_of("USB 1 connected"), theme.style(Role::Dim));

    let mut amp = app_on(profile_named("kara"));
    hear(
        &mut amp,
        vec![vo("5", Some("headphone"), json!(false), "false")],
    );
    assert!(
        status_line_text(&amp, 78, 0).contains("Headphone output no"),
        "{}",
        status_line_text(&amp, 78, 0)
    );
}

/// The Link row says in words when the device refused the passphrase or the readings are stale,
/// in the words the Devices list and the web page use too, and says nothing extra otherwise.
#[test]
fn refused_and_stale_are_said() {
    crate::tui::widgets::graph::hold_wall_ms(WALL);
    let words = |app: &App| {
        text(&frame(app, 120, 24))
            .into_iter()
            .find(|row| row.contains("Link"))
            .expect("the Link row")
    };
    let mut app = mockup_app();
    assert!(!words(&app).contains('\u{b7}'), "{}", words(&app));
    app.session.facts = Some(crate::backend::DeviceRow {
        refused: true,
        ..bifrost_facts(None)
    });
    assert!(
        words(&app).contains("connected \u{b7} passphrase refused"),
        "{}",
        words(&app)
    );
    app.session.connected = false;
    app.session.facts = Some(crate::backend::DeviceRow {
        stale: true,
        ..bifrost_facts(None)
    });
    assert!(
        words(&app).contains("disconnected \u{b7} readings stale"),
        "{}",
        words(&app)
    );
}

/// An amp with two false boolean readouts, both firmware builds and an active fault: on a pane too
/// narrow for all of that, the dim false readouts go (the last first) and the firmware builds and
/// the faults keep the room they had without them, with the fault pulse still on.
#[test]
fn false_readouts_give_way_to_firmware_and_faults_when_narrow() {
    let mut app = app_on(profile_named("aegir2"));
    hear(
        &mut app,
        vec![
            vo("4", Some("warming_up"), json!(false), "false"),
            vo("21.1", Some("temperature_reporting"), json!(false), "false"),
            vo("210.2.1", None, json!(3), "3"),
            vo("210.2.2", None, json!(4), "4"),
            vo("230.1", Some("fault_overcurrent"), json!(true), "active"),
        ],
    );
    let rows = text(&screen(&app));
    let line = rows
        .iter()
        .find(|row| row.contains("Faults:"))
        .unwrap_or_else(|| panic!("the Faults word was cut: {rows:#?}"));
    assert!(line.contains("fw 3") && line.contains("DSP 4"), "{line}");
    assert!(line.contains("Faults: Over-current"), "{line}");
    assert!(
        !line.contains("Temperature reporting no"),
        "the last false readout kept its room: {line}"
    );
    assert!(
        fault_line(Rect::new(0, 0, 80, 24), &app).is_some(),
        "the fault pulse is off"
    );
    // Where everything fits, the false readouts are all there; where nothing but the build and
    // the faults fit, none of them is.
    let wide = status_line_text(&app, 120, 0);
    assert!(
        wide.starts_with(" Warming up no   Temperature reporting no   fw 3"),
        "{wide}"
    );
    let tight = status_line_text(&app, 48, 0);
    assert_eq!(tight, " fw 3   DSP 4   Faults: Over-current");
}
