// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use clap::CommandFactory;

use super::*;

/// Every verb in the tree stands in the grouped block, and every line of the block names a
/// verb the tree has: the command list `--help` prints is the whole vocabulary, in a
/// build where the two are written apart.
///
/// `help` is clap's own and prints the block rather than standing in it, so it is left out
/// of the comparison.
#[test]
fn the_grouped_block_lists_every_verb_the_tree_has() {
    let mut tree: Vec<String> = Cli::command()
        .get_subcommands()
        .map(|sub| sub.get_name().to_string())
        .filter(|name| name != "help")
        .collect();
    let mut listed: Vec<String> = GROUPS
        .iter()
        .flat_map(|(_, verbs)| verbs.iter())
        .map(|(verb, _)| (*verb).to_string())
        .collect();
    tree.sort();
    listed.sort();
    assert_eq!(listed, tree);
}

/// What the command list says a verb gives is what the verb's own `--help` says: both read
/// the one table.
#[test]
fn each_verb_describes_itself_the_way_the_list_describes_it() {
    let command = Cli::command();
    for (_, verbs) in GROUPS {
        for (verb, about) in *verbs {
            let sub = command
                .get_subcommands()
                .find(|sub| sub.get_name() == *verb)
                .unwrap_or_else(|| panic!("the tree has no {verb}"));
            assert_eq!(
                sub.get_about().map(|a| a.to_string()).as_deref(),
                Some(*about),
                "{verb}"
            );
        }
    }
}

/// Each group heads the verbs under it, and the groups come in the order the block lists
/// them: what a new user needs first at the top, what takes a device apart at the bottom.
#[test]
fn the_block_heads_each_group_above_its_own_verbs() {
    let block = grouped_help();
    let mut at = 0;
    for (group, verbs) in GROUPS {
        let heading = block[at..]
            .find(&format!("{group}:"))
            .unwrap_or_else(|| panic!("{group} is not in the block after the group before it"));
        at += heading;
        for (verb, about) in *verbs {
            let line = block[at..]
                .find(&format!("  {verb} "))
                .unwrap_or_else(|| panic!("{verb} is not under {group}"));
            assert!(
                block[at + line..].starts_with(&format!("  {verb} ")),
                "{verb}"
            );
            assert!(block[at + line..].contains(about), "{verb}");
            at += line;
        }
    }
}

/// The tree itself parses: a mistyped derive is a panic here rather than on a user's first
/// command.
#[test]
fn the_command_tree_is_well_formed() {
    Cli::command().debug_assert();
}

#[test]
fn parse_duration_reads_seconds_milliseconds_and_bare_numbers() {
    assert_eq!(parse_duration("5s").unwrap(), Duration::from_secs(5));
    assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
    assert_eq!(parse_duration("2.5").unwrap(), Duration::from_millis(2500));
}

/// Nothing `Duration::from_secs_f64` would panic on, and nothing absurd for a CLI timeout,
/// gets past the parse.
#[test]
fn parse_duration_rejects_what_would_not_make_sense_as_a_timeout() {
    assert!(parse_duration("not-a-duration").is_err());
    assert!(parse_duration("-1s").is_err());
    assert!(parse_duration("nans").is_err());
    assert!(parse_duration("1e18s").is_err(), "far past the 1e9 s bound");
}
