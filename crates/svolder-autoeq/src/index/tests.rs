// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const HD650: &str =
    "- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990";
const HD650_GRAS: &str = "- [Sennheiser HD 650](./crinacle/GRAS%2043AG-7%20over-ear/Sennheiser%20HD%20650) by crinacle on GRAS 43AG-7";
const N8: &str = "- [64 Audio N8 (CIEM, m15 Apex module)](./crinacle/711%20in-ear/64%20Audio%20N8%20(CIEM,%20m15%20Apex%20module)) by crinacle on 711";

fn entry(name: &str, source: &str) -> Entry {
    Entry {
        name: name.into(),
        source: source.into(),
        form: "over-ear".into(),
        rig: None,
    }
}

#[test]
fn the_three_real_shapes_parse() {
    let entries = parse_index(&[HD650, HD650_GRAS, N8].join("\n"));
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0], entry("Sennheiser HD 650", "oratory1990"));
    assert_eq!(entries[1].source, "crinacle");
    assert_eq!(entries[1].form, "over-ear");
    assert_eq!(entries[1].rig.as_deref(), Some("GRAS 43AG-7"));
    assert_eq!(entries[2].name, "64 Audio N8 (CIEM, m15 Apex module)");
    assert_eq!(entries[2].form, "in-ear");
    assert_eq!(entries[2].rig.as_deref(), Some("711"));
}

#[test]
fn balanced_parentheses_ampersands_and_commas_parse() {
    let line = "- [1MORE Quad Driver](./HypetheSonics/Bruel%20&%20Kjaer%205128%20in-ear/1MORE%20Quad%20Driver) by HypetheSonics on Bruel & Kjaer 5128";
    let entries = parse_index(line);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].rig.as_deref(), Some("Bruel & Kjaer 5128"));
    let nested = "- [A (B (C)) D](./s/earbud/A%20(B%20(C))%20D) by s";
    assert_eq!(parse_index(nested)[0].name, "A (B (C)) D");
    let unbalanced = "- [A (B](./s/earbud/A%20(B) by s";
    assert!(parse_index(unbalanced).is_empty());
}

#[test]
fn bad_lines_are_skipped() {
    let text = [
        "This is a list of all equalization profiles.",
        "",
        "- [no link]",
        "- [Two parts](./a/over-ear) by a",
        "- [Four parts](./a/over-ear/x/y) by a",
        "- [Odd form](./a/headset/Name) by a",
        "- [No by](./a/over-ear/Name)",
        "- [Dots](./a/over-ear/..) by a",
        "- [Bad utf8](./a/over-ear/%FF) by a",
        "- [Empty source](.//over-ear/Name) by a",
        "- [Rig only](./a/%20over-ear/Name) by a",
        HD650,
    ]
    .join("\r\n");
    let entries = parse_index(&text);
    assert_eq!(entries, vec![entry("Sennheiser HD 650", "oratory1990")]);
}

#[test]
fn origin_names_source_form_and_rig() {
    let mut e = entry("X", "oratory1990");
    assert_eq!(e.origin(), "AutoEq · oratory1990 · over-ear");
    e.rig = Some("GRAS 43AG-7".into());
    e.source = "crinacle".into();
    assert_eq!(e.origin(), "AutoEq · crinacle · over-ear · GRAS 43AG-7");
}

#[test]
fn the_url_encodes_each_component() {
    let e = parse_index(HD650).remove(0);
    assert_eq!(
        e.peq_url("https://example.com/results/"),
        "https://example.com/results/oratory1990/over-ear/Sennheiser%20HD%20650/Sennheiser%20HD%20650%20ParametricEQ.txt"
    );
    let gras = parse_index(HD650_GRAS).remove(0);
    assert!(
        gras.peq_url("http://h/r")
            .starts_with("http://h/r/crinacle/GRAS%2043AG-7%20over-ear/")
    );
    let n8 = parse_index(N8).remove(0);
    assert!(
        n8.peq_url("http://h/")
            .contains("/64%20Audio%20N8%20%28CIEM%2C%20m15%20Apex%20module%29/")
    );
}

/// Review Focus 3: names with `/`, `..`, `%2F`, `#` or `?` stay inside one component.
#[test]
fn awkward_names_stay_in_one_url_component() {
    for raw in [
        "a/b", "..", ".", "a/../b", "%2F", "x#y", "x?y=1", "a\\b", "a%2Fb", "q&r",
    ] {
        let e = Entry {
            name: raw.into(),
            source: raw.into(),
            form: "over-ear".into(),
            rig: Some(raw.into()),
        };
        let url = e.peq_url("http://h/results/");
        let path = url.strip_prefix("http://h/results/").unwrap();
        assert_eq!(path.split('/').count(), 4, "{raw}: {url}");
        assert!(!path.contains('#') && !path.contains('?'), "{raw}: {url}");
        assert!(
            path.split('/').all(|c| c != "." && c != ".."),
            "{raw}: {url}"
        );
        assert!(
            !path.contains("%2f") || path.contains("%2F"),
            "{raw}: {url}"
        );
    }
    let e = Entry {
        name: "..".into(),
        source: "..".into(),
        form: "in-ear".into(),
        rig: None,
    };
    assert!(
        e.peq_url("http://h/")
            .starts_with("http://h/%2E%2E/in-ear/%2E%2E/")
    );
}

/// Names that decode to a path-like text never survive the parse as one: `%2F` decodes inside
/// its component and re-encodes on the way out.
#[test]
fn encoded_slashes_in_the_index_do_not_split_a_component() {
    let line = "- [a/b](./s/over-ear/a%2Fb) by s";
    let entries = parse_index(line);
    assert_eq!(entries[0].name, "a/b");
    assert!(
        entries[0]
            .peq_url("http://h/")
            .contains("/a%2Fb/a%2Fb%20ParametricEQ.txt")
    );
    for bad in [
        "- [x](./s/over-ear/%2E%2E) by s",
        "- [x](./%2E%2E/over-ear/n) by s",
        "- [x](./s/over-ear/a%00b) by s",
        "- [x](./s/over-ear/a%0Ab) by s",
    ] {
        assert!(parse_index(bad).is_empty(), "{bad}");
    }
}

fn corpus() -> Vec<Entry> {
    parse_index(
        &[
            "- [Audio-Technica ATH-M50x](./oratory1990/over-ear/Audio-Technica%20ATH-M50x) by oratory1990",
            "- [Audio-Technica ATH-M50xBT](./oratory1990/over-ear/Audio-Technica%20ATH-M50xBT) by oratory1990",
            "- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990",
            "- [Sennheiser HD 650](./crinacle/GRAS%2043AG-7%20over-ear/Sennheiser%20HD%20650) by crinacle on GRAS 43AG-7",
            "- [Sennheiser HD 650](./Rtings/over-ear/Sennheiser%20HD%20650) by Rtings",
            "- [Sennheiser HD 6XX](./oratory1990/over-ear/Sennheiser%20HD%206XX) by oratory1990",
            "- [Beyerdynamic DT 770 Pro](./oratory1990/over-ear/Beyerdynamic%20DT%20770%20Pro) by oratory1990",
            "- [HD 650 mod](./x/over-ear/HD%20650%20mod) by x",
            "- [HD 650](./x/over-ear/HD%20650) by x",
        ]
        .join("\n"),
    )
}

fn names<'a>(found: &[&'a Entry]) -> Vec<&'a str> {
    found.iter().map(|e| e.name.as_str()).collect()
}

#[test]
fn search_ignores_case_spaces_and_punctuation() {
    let entries = corpus();
    assert!(names(&search(&entries, "hd650")).contains(&"Sennheiser HD 650"));
    assert_eq!(
        names(&search(&entries, "athm50x")),
        ["Audio-Technica ATH-M50x", "Audio-Technica ATH-M50xBT"]
    );
    assert_eq!(search(&entries, "").len(), 0);
    assert_eq!(search(&entries, " - ").len(), 0);
    assert_eq!(search(&entries, "nonexistent").len(), 0);
}

#[test]
fn every_word_must_appear() {
    let entries = corpus();
    assert_eq!(search(&entries, "sennheiser 650").len(), 3);
    assert_eq!(search(&entries, "650 sennheiser").len(), 3);
    assert_eq!(search(&entries, "sennheiser dt").len(), 0);
}

#[test]
fn ranking_is_exact_then_prefix_then_alphabetical() {
    let entries = corpus();
    let found = search(&entries, "hd 650");
    assert_eq!(
        names(&found),
        [
            "HD 650",
            "HD 650 mod",
            "Sennheiser HD 650",
            "Sennheiser HD 650",
            "Sennheiser HD 650"
        ]
    );
    let found = search(&entries, "sennheiser");
    assert_eq!(found.len(), 4);
    assert_eq!(found[3].name, "Sennheiser HD 6XX");
}

#[test]
fn autoeqs_order_holds_within_a_name() {
    let entries = corpus();
    let found = search(&entries, "sennheiser hd 650");
    let sources: Vec<&str> = found.iter().map(|e| e.source.as_str()).collect();
    assert_eq!(sources, ["oratory1990", "crinacle", "Rtings"]);
}

#[test]
fn a_query_starting_a_word_outranks_one_found_across_words() {
    let entries = parse_index(
        "- [Denon AH-D600](./a/over-ear/Denon%20AH-D600) by a\n\
         - [Sennheiser HD 600](./a/over-ear/Sennheiser%20HD%20600) by a\n\
         - [HD 600 Pro](./a/over-ear/HD%20600%20Pro) by a\n",
    );
    assert_eq!(
        names(&search(&entries, "hd 600")),
        ["HD 600 Pro", "Sennheiser HD 600", "Denon AH-D600"]
    );
    assert_eq!(
        names(&search(&entries, "hd600")),
        ["HD 600 Pro", "Sennheiser HD 600", "Denon AH-D600"]
    );
}

#[test]
fn exact_finds_only_the_whole_name() {
    let entries = corpus();
    let found = exact(&entries, "hd 650");
    assert_eq!(names(&found), ["HD 650"]);
    assert_eq!(exact(&entries, "SENNHEISER hd650").len(), 3);
    assert!(exact(&entries, "650").is_empty());
    assert!(exact(&entries, " - ").is_empty());
}
