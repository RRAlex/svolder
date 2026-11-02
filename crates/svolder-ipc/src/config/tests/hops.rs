// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Declared hops: parsing their blocks, what is refused, lookup, the warnings and the hop
//! writers.

use super::*;

/// A declared hop with nothing but a kind and a name.
fn declared(kind: HopKind, name: &str) -> Hop {
    Hop {
        what: HopWhat::Declared {
            kind,
            name: name.into(),
        },
        note: None,
        after: None,
    }
}

/// The ids of the hops a file holds, in order.
fn ids(path: &Path) -> Vec<String> {
    Config::load_from(path)
        .unwrap()
        .hops
        .iter()
        .map(|hop| hop.id().to_string())
        .collect()
}

#[test]
fn a_hop_block_parses_declared_and_linked() {
    let text = "[[hop]]\nname = \"HD 650\"\nkind = \"headphones\"\nnote = \"balanced\"\n\n\
                [[hop]]\ndevice = \"Vali\"\nafter = \"Bifrost 3\"\n";
    let config = Config::parse(text).unwrap();
    assert_eq!(config.hops.len(), 2);
    assert_eq!(config.hops[0].id(), "HD 650");
    assert_eq!(
        config.hops[0].what,
        HopWhat::Declared {
            kind: HopKind::Headphones,
            name: "HD 650".into()
        }
    );
    assert_eq!(config.hops[0].note.as_deref(), Some("balanced"));
    assert_eq!(
        config.hops[1].what,
        HopWhat::Linked {
            device: "Vali".into()
        }
    );
    assert_eq!(config.hops[1].id(), "Vali");
    assert_eq!(config.hops[1].after.as_deref(), Some("Bifrost 3"));
    assert_eq!(config.hops[1].note, None);
}

#[test]
fn a_hop_names_a_device_or_a_name_never_both() {
    let both = "[[hop]]\nname = \"HD 650\"\nkind = \"headphones\"\ndevice = \"Vali\"\n";
    let err = Config::parse(both).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m)
            if m.contains("[[hop]]") && m.contains("\"name\"") && m.contains("\"device\"")),
        "{err:?}"
    );
    let neither = "[[hop]]\nnote = \"alone\"\n";
    let err = Config::parse(neither).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("[[hop]]") && m.contains("needs")),
        "{err:?}"
    );
}

#[test]
fn a_hop_kind_is_one_of_four_words() {
    let err = Config::parse("[[hop]]\nname = \"X\"\nkind = \"tube\"\n").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m)
            if m.contains("\"X\"") && m.contains("amp, headphones, speakers, other")),
        "{err:?}"
    );
    let err = Config::parse("[[hop]]\nname = \"X\"\n").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("needs a kind")),
        "{err:?}"
    );
    let err = Config::parse("[[hop]]\ndevice = \"Vali\"\nkind = \"amp\"\n").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("\"Vali\"") && m.contains("kind")),
        "{err:?}"
    );
}

#[test]
fn a_hop_with_an_unknown_key_or_a_taken_id_or_no_name_is_refused() {
    let err = Config::parse("[[hop]]\nname = \"X\"\nkind = \"amp\"\nsparkle = true\n").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("\"X\"") && m.contains("sparkle")),
        "{err:?}"
    );
    let twice = "[[hop]]\nname = \"X\"\nkind = \"amp\"\n[[hop]]\ndevice = \" x \"\n";
    let err = Config::parse(twice).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("already configured")),
        "{err:?}"
    );
    let err = Config::parse("[[hop]]\nname = \"  \"\nkind = \"amp\"\n").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("needs a name")),
        "{err:?}"
    );
    let err = Config::parse("[[hop]]\nname = 3\nkind = \"amp\"\n").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("string")),
        "{err:?}"
    );
}

#[test]
fn a_hop_naming_no_configured_device_is_a_warning() {
    let device = "[[device]]\nname = \"Bifrost 3\"\nudid = \"0102030405060708\"\n";
    let linked = format!("{device}[[hop]]\ndevice = \"Ghost\"\n");
    let said = Config::parse(&linked).unwrap().warnings();
    assert!(said.iter().any(|line| line.contains("Ghost")), "{said:?}");
    let after = format!("{device}[[hop]]\nname = \"X\"\nkind = \"amp\"\nafter = \"Ghost\"\n");
    let said = Config::parse(&after).unwrap().warnings();
    assert!(said.iter().any(|line| line.contains("Ghost")), "{said:?}");
    let fine = format!("{device}[[hop]]\ndevice = \"bifrost 3\"\nafter = \"Bifrost 3\"\n");
    assert_eq!(
        Config::parse(&fine).unwrap().warnings(),
        Vec::<String>::new()
    );
}

#[test]
fn a_hop_is_found_by_its_id_case_and_blanks_aside() {
    let config = Config::parse("[[hop]]\nname = \"HD 650\"\nkind = \"headphones\"\n").unwrap();
    assert!(config.hop(" hd 650 ").is_some());
    assert!(config.hop("HD 600").is_none());
}

#[test]
fn add_hop_appends_and_refuses_a_taken_id() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let hop = Hop {
        note: Some("balanced".into()),
        ..declared(HopKind::Headphones, "HD 650")
    };
    Config::add_hop(&path, &hop).unwrap();
    assert_eq!(Config::load_from(&path).unwrap().hops, vec![hop.clone()]);
    let before = std::fs::read_to_string(&path).unwrap();
    for taken in ["HD 650", "hd 650 "] {
        let err = Config::add_hop(&path, &declared(HopKind::Speakers, taken)).unwrap_err();
        assert!(
            matches!(&err, ConfigError::Parse(m) if m.contains("HD 650")),
            "{taken:?}: {err:?}"
        );
    }
    let linked = Hop {
        what: HopWhat::Linked {
            device: "hd 650".into(),
        },
        note: None,
        after: None,
    };
    assert!(
        Config::add_hop(&path, &linked).is_err(),
        "a device id counts too"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    let blank = Config::add_hop(&path, &declared(HopKind::Amp, "  ")).unwrap_err();
    assert!(
        matches!(&blank, ConfigError::Parse(m) if m.contains("needs a name")),
        "{blank:?}"
    );
    Config::add_hop(&path, &declared(HopKind::Amp, " Vali 2 ")).unwrap();
    assert_eq!(
        ids(&path),
        ["HD 650", "Vali 2"],
        "the blanks are not written"
    );
}

#[test]
fn update_hop_keeps_its_place_and_its_comments() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original = "# mine\nschema = 1\n\n# the cans\n[[hop]]\nname = \"HD 650\" # balanced\n\
                    kind = \"headphones\"\nnote = \"old\"\n\n[[hop]]\ndevice = \"Vali\"\n\n\
                    [tui]\npalette = \"ice\"\n";
    std::fs::write(&path, original).unwrap();
    let hop = Hop {
        what: HopWhat::Declared {
            kind: HopKind::Speakers,
            name: "HD 660".into(),
        },
        note: None,
        after: Some("Bifrost 3".into()),
    };
    Config::update_hop(&path, "hd 650", &hop).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("# the cans\n[[hop]]\nname = \"HD 660\" # balanced\n"),
        "{text}"
    );
    assert!(text.contains("kind = \"speakers\""), "{text}");
    assert!(!text.contains("note"), "{text}");
    assert!(text.contains("[tui]\npalette = \"ice\"\n"), "{text}");
    let config = Config::parse(&text).unwrap();
    assert_eq!(config.hops[0], hop, "the same place in the order");
    assert_eq!(config.hops[1].id(), "Vali");

    // Linking the block to a device takes the declared keys out.
    let linked = Hop {
        what: HopWhat::Linked {
            device: "Vali 2".into(),
        },
        note: Some("hi".into()),
        after: None,
    };
    Config::update_hop(&path, "HD 660", &linked).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("kind") && !text.contains("HD 660"), "{text}");
    assert_eq!(Config::parse(&text).unwrap().hops[0], linked);

    let err = Config::update_hop(&path, "nobody", &linked).unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("nobody")),
        "{err:?}"
    );
    let onto = Hop {
        what: HopWhat::Linked {
            device: "vali".into(),
        },
        ..linked.clone()
    };
    assert!(
        Config::update_hop(&path, "Vali 2", &onto).is_err(),
        "onto another hop's id"
    );
    let again = Hop {
        what: HopWhat::Linked {
            device: "VALI 2".into(),
        },
        ..linked
    };
    Config::update_hop(&path, "Vali 2", &again).expect("another spelling of its own id");
}

#[test]
fn remove_hop_takes_out_one_block() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original = "# mine\nschema = 1\n\n# the cans\n[[hop]]\nname = \"a\"\nkind = \"amp\"\n\n\
                    [[hop]]\nname = \"b\"\nkind = \"amp\"\n\n[tui]\npalette = \"ice\"\n";
    std::fs::write(&path, original).unwrap();
    Config::remove_hop(&path, "A").unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        !text.contains("the cans"),
        "a comment right above goes with it: {text}"
    );
    assert!(text.contains("# mine\nschema = 1\n"), "{text}");
    assert!(text.contains("[tui]\npalette = \"ice\"\n"), "{text}");
    assert_eq!(ids(&path), ["b"]);
    Config::remove_hop(&path, "b").unwrap();
    assert_eq!(ids(&path), Vec::<String>::new());
    assert!(Config::remove_hop(&path, "b").is_err());
}

#[test]
fn move_hop_puts_it_where_asked() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    let original = "schema = 1\n\n# first\n[[hop]]\nname = \"a\"\nkind = \"amp\"\n\n\
                    [[hop]]\nname = \"b\"\nkind = \"amp\"\n\n[[hop]]\nname = \"c\"\nkind = \"amp\"\n\n\
                    [tui]\npalette = \"ice\"\n";
    std::fs::write(&path, original).unwrap();
    Config::move_hop(&path, "c", 0).unwrap();
    assert_eq!(ids(&path), ["c", "a", "b"]);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(
        text.contains("# first\n[[hop]]\nname = \"a\""),
        "a comment stays with its block: {text}"
    );
    assert!(text.contains("[tui]\npalette = \"ice\"\n"), "{text}");
    Config::move_hop(&path, "c", 1).unwrap();
    assert_eq!(ids(&path), ["a", "c", "b"]);
    Config::move_hop(&path, "a", 99).unwrap();
    assert_eq!(ids(&path), ["c", "b", "a"], "past the end is last");
    Config::move_hop(&path, "b", 1).unwrap();
    assert_eq!(ids(&path), ["c", "b", "a"], "where it is already");
    assert!(Config::move_hop(&path, "nobody", 0).is_err());
}

/// The template's own comments end where the first hop starts: moving hops round leaves them
/// above the first block rather than carrying them along with whichever block was added first.
#[test]
fn move_hop_leaves_the_comments_a_blank_line_sets_apart_where_they_were() {
    let dir = temp_dir("config-writer");
    let path = dir.join("config.toml");
    Config::add_hop(&path, &declared(HopKind::Amp, "a")).unwrap();
    Config::add_hop(&path, &declared(HopKind::Amp, "b")).unwrap();
    Config::move_hop(&path, "b", 0).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(ids(&path), ["b", "a"]);
    let first = text.find("\n[[hop]]\n").expect("a hop block");
    let among = &text[first..];
    assert!(
        !among.contains('#'),
        "no comment ended up among the hops: {text}"
    );
    assert!(
        text[..first]
            .trim_end()
            .ends_with("# stops = [\"#5a1a08\", \"#c04a10\", \"#e0a030\", \"#f6dca0\"]"),
        "the template's last comment stays above the hops: {text}"
    );
}

#[test]
fn a_hop_kind_is_read_as_written_in_lower_case() {
    let err = Config::parse("[[hop]]\nname = \"X\"\nkind = \"Amp\"\n").unwrap_err();
    assert!(
        matches!(&err, ConfigError::Parse(m) if m.contains("amp, headphones, speakers, other")),
        "{err:?}"
    );
    assert_eq!(HopKind::named("amp"), Some(HopKind::Amp));
    assert_eq!(HopKind::named("Amp"), None);
}
