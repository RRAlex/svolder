// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the crossfeed stage should do to a tone, from the engine's own response functions: the
//! oracle of `make host-e2e` (scripts/host-e2e/analyse.py).
//!
//! `xf_expect <rate> <model> <amount_pct> <crossover_hz> <delay_ms|own> <hz>...` prints one JSON
//! object per frequency: `{"hz", "same": [re, im], "other": [re, im]}`, the complex gains of an
//! output channel from its own channel's tone and from the other channel's (see
//! [`svolder_host::eq::stage_response`]). The numbers are the ones a `host` view's `crossfeed`
//! carries, so a run passes them on as the view gave them.

use svolder_host::eq::{CrossfeedModel, CrossfeedSettings, CrossfeedStage, stage_response};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 6 {
        eprintln!(
            "usage: xf_expect <rate> <model> <amount_pct> <crossover_hz> <delay_ms|own> <hz>..."
        );
        std::process::exit(2);
    }
    let number = |text: &str| -> f64 {
        text.parse().unwrap_or_else(|_| {
            eprintln!("xf_expect: {text:?} is not a number");
            std::process::exit(2)
        })
    };
    let rate = number(&args[0]) as u32;
    let Some(model) = CrossfeedModel::from_word(&args[1]) else {
        eprintln!("xf_expect: no model {:?}", args[1]);
        std::process::exit(2)
    };
    let settings = CrossfeedSettings {
        model,
        preset: None,
        edited_from: None,
        amount: number(&args[2]) / 100.0,
        crossover_hz: number(&args[3]),
        delay_ms: (args[4] != "own").then(|| number(&args[4])),
    };
    let stage = match CrossfeedStage::of(&settings, rate) {
        Ok(stage) => stage,
        Err(why) => {
            eprintln!("xf_expect: {why}");
            std::process::exit(1)
        }
    };
    for hz in &args[5..] {
        let hz = number(hz);
        let r = stage_response(&stage, rate, hz);
        println!(
            "{{\"hz\": {hz}, \"same\": [{}, {}], \"other\": [{}, {}], \"delay_samples\": {}}}",
            r.same[0], r.same[1], r.other[0], r.other[1], stage.delay_samples
        );
    }
}
