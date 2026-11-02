// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The `[[hop]]` blocks: what the sound goes through after the DAC, declared by hand or linked
//! to a configured device, and the writers that add, update, remove and move one.

use std::path::Path;

use svolder_profiles::tone::preset::{NAME_MAX, NameError, checked_name, same_name};
use toml_edit::{ArrayOfTables, DocumentMut, Table, Value};

use super::edit::{blocks_mut, detached, edit_file, hand_on, prefix_of, push_block, set_key};
use super::{Config, ConfigError};

/// What a declared hop is, from `kind` in its `[[hop]]` block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HopKind {
    /// A power or headphone amplifier.
    Amp,
    /// Headphones or earphones.
    Headphones,
    /// Loudspeakers.
    Speakers,
    /// Anything else the sound goes through.
    Other,
}

impl HopKind {
    /// Every kind, in the order the configuration error and the front ends list them.
    pub const ALL: [HopKind; 4] = [
        HopKind::Amp,
        HopKind::Headphones,
        HopKind::Speakers,
        HopKind::Other,
    ];

    /// What `kind` in a `[[hop]]` block writes this as, which is the word it is read back from.
    pub fn name(self) -> &'static str {
        match self {
            HopKind::Amp => "amp",
            HopKind::Headphones => "headphones",
            HopKind::Speakers => "speakers",
            HopKind::Other => "other",
        }
    }

    /// The kind `word` names, or `None` for a word that names none of them. The words are
    /// lower case and compared as written, like every other word the file is read from.
    pub fn named(word: &str) -> Option<HopKind> {
        HopKind::ALL.into_iter().find(|kind| kind.name() == word)
    }

    /// Every word, joined as the configuration error lists them: `amp, headphones, ...`.
    pub fn words() -> String {
        let words: Vec<&str> = HopKind::ALL.iter().map(|kind| kind.name()).collect();
        words.join(", ")
    }
}

/// What a `[[hop]]` block stands for.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum HopWhat {
    /// A piece of equipment named by hand, from `name` and `kind`.
    Declared {
        /// What sort of equipment it is.
        kind: HopKind,
        /// The name every front end shows and picks it by.
        name: String,
    },
    /// A configured device, from `device`: the same unit the `[[device]]` block configures,
    /// which a front end can reach.
    Linked {
        /// The configured device's name; it may name no configured device, which
        /// [`Config::warnings`] says.
        device: String,
    },
}

/// One `[[hop]]` block: a stage after the DAC.
///
/// The hops are listed in file order, which is the order the sound takes through them within
/// each DAC they follow.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Hop {
    /// A declared piece of equipment, or a configured device.
    pub what: HopWhat,
    /// A free line about it, from `note`.
    pub note: Option<String>,
    /// The configured device whose output it follows, from `after`; `None` follows the first
    /// device. It may name no configured device, which [`Config::warnings`] says.
    pub after: Option<String>,
}

impl Hop {
    /// What names the hop wherever one is picked: the declared name, or the linked device's.
    pub fn id(&self) -> &str {
        match &self.what {
            HopWhat::Declared { name, .. } => name,
            HopWhat::Linked { device } => device,
        }
    }
}

impl Config {
    /// The hop whose [`Hop::id`] is `id`, matching case and the blanks round it aside
    /// ([`same_name`]), or `None`.
    pub fn hop(&self, id: &str) -> Option<&Hop> {
        self.hops.iter().find(|hop| same_name(hop.id(), id))
    }

    /// Append `hop` to the file at `path` as a new `[[hop]]` block.
    ///
    /// The name or device is written without the blanks round it, and an id another hop
    /// already carries, compared as [`Config::hop`] compares, is refused, so the file never
    /// grows two hops a selector cannot tell apart.
    pub fn add_hop(path: &Path, hop: &Hop) -> Result<(), ConfigError> {
        let hop = &written(hop)?;
        edit_file(path, |doc| {
            if let Some((_, clash)) = hop_named(doc, hop.id()) {
                return Err(hop_taken(&clash));
            }
            let mut table = Table::new();
            write_hop(&mut table, hop);
            push_block(doc, HOP, table)
        })
    }

    /// Write `hop` over the `[[hop]]` block whose id is `id` in the file at `path`, in the
    /// place it already holds.
    ///
    /// The caller merges whatever it keeps from the old hop into `hop` first: every key the
    /// new hop says nothing about (`note`, `after`) is taken out of the block. Each key that
    /// stays is set where it already sits, so the comments around the block survive, and
    /// turning a declared hop into a linked one, or back, takes the other's keys out.
    ///
    /// An id no block carries is refused, and so is renaming onto another hop's id; renaming a
    /// block to the id it has, or to another spelling of it, is no collision.
    pub fn update_hop(path: &Path, id: &str, hop: &Hop) -> Result<(), ConfigError> {
        let hop = &written(hop)?;
        edit_file(path, |doc| {
            let at = hop_at(doc, id)?;
            if let Some((clash_at, clash)) = hop_named(doc, hop.id())
                && clash_at != at
            {
                return Err(hop_taken(&clash));
            }
            let table = hops_mut(doc)?
                .get_mut(at)
                .ok_or_else(|| ConfigError::Parse(format!("hop {id:?} went missing")))?;
            write_hop(table, hop);
            Ok(())
        })
    }

    /// Take the `[[hop]]` block whose id is `id` out of the file at `path`.
    ///
    /// The comments written right above the block go with it, and comments a blank line sets
    /// apart from it are the file's, handed to whatever the document prints next, as
    /// [`Config::remove_preset`] does. An id no block carries is refused.
    pub fn remove_hop(path: &Path, id: &str) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            let at = hop_at(doc, id)?;
            let taken = hops_mut(doc)?.remove(at);
            let carried = detached(prefix_of(&taken));
            if !carried.is_empty() {
                hand_on(doc, HOP, at, carried)?;
            }
            Ok(())
        })
    }

    /// Put the `[[hop]]` block whose id is `id` at position `to` among all the hops of the file
    /// at `path`, counted from 0; a position past the end puts it last.
    ///
    /// The block keeps its own keys and comments; the blocks round it keep theirs. An id no
    /// block carries is refused.
    pub fn move_hop(path: &Path, id: &str, to: usize) -> Result<(), ConfigError> {
        edit_file(path, |doc| {
            let at = hop_at(doc, id)?;
            let blocks = hops_mut(doc)?;
            let tables: Vec<Table> = std::mem::take(blocks).into_iter().collect();
            // The document prints its tables in the order of the positions they were read at,
            // so the places the blocks held are handed out again in the new order. What sits
            // above a block is the block's own only from its last blank line down; the spacing
            // and the comments a blank line sets apart stay with the place.
            let places: Vec<Option<isize>> = tables.iter().map(Table::position).collect();
            let above: Vec<(String, String)> = tables
                .iter()
                .map(|table| {
                    let (place, own) = prefix_of(table).split_at(own_from(prefix_of(table)));
                    (place.to_string(), own.to_string())
                })
                .collect();
            let mut order: Vec<usize> = (0..tables.len()).collect();
            let moved = order.remove(at);
            order.insert(to.min(order.len()), moved);
            let mut kept: Vec<Option<Table>> = tables.into_iter().map(Some).collect();
            for (now, &was) in order.iter().enumerate() {
                let mut table = kept[was].take().expect("each block is placed once");
                table.set_position(places[now]);
                table
                    .decor_mut()
                    .set_prefix(format!("{}{}", above[now].0, above[was].1));
                blocks.push(table);
            }
            Ok(())
        })
    }
}

/// Where in `prefix`, the text above a block, the block's own part starts: after the last line
/// that is blank, or at the start where there is none.
fn own_from(prefix: &str) -> usize {
    let mut at = 0;
    let mut from = 0;
    for line in prefix.split_inclusive('\n') {
        at += line.len();
        if line.trim().is_empty() {
            from = at;
        }
    }
    from
}

/// The key the `[[hop]]` blocks sit under.
const HOP: &str = "hop";

/// The id of `table` as the loaded configuration spells it: its `name`, else its `device`,
/// without the blanks round it.
fn id_of(table: &Table) -> Option<String> {
    let text = table
        .get("name")
        .or_else(|| table.get("device"))?
        .as_str()?;
    Some(text.trim().to_string())
}

/// The `[[hop]]` block of `doc` whose id is `id`, as where it sits and its id as the file
/// spells it, matching case and the blanks round either aside.
fn hop_named(doc: &DocumentMut, id: &str) -> Option<(usize, String)> {
    doc.as_table()
        .get(HOP)?
        .as_array_of_tables()?
        .iter()
        .enumerate()
        .find_map(|(at, table)| {
            let carried = id_of(table)?;
            same_name(&carried, id).then_some((at, carried))
        })
}

/// Which `[[hop]]` block of `doc` carries `id`, or the error that names none is.
fn hop_at(doc: &DocumentMut, id: &str) -> Result<usize, ConfigError> {
    hop_named(doc, id)
        .map(|(at, _)| at)
        .ok_or_else(|| ConfigError::Parse(format!("no hop named {id:?} is configured")))
}

/// What a hop writer, and the parse of the file, refuse an id another block carries with.
pub(super) fn hop_taken(id: &str) -> ConfigError {
    ConfigError::Parse(format!("a hop named {id:?} is already configured"))
}

/// Why `why` makes no name for a hop, in words.
pub(super) fn name_refusal(why: NameError) -> String {
    match why {
        NameError::Blank => "a hop needs a name or a device".to_string(),
        NameError::TooLong(length) => {
            format!("a hop's name is at most {NAME_MAX} characters, not {length}")
        }
        NameError::Control => {
            "a hop's name cannot hold a control character, such as a newline or a tab".to_string()
        }
    }
}

/// `text` as a hop writer puts a name or a device in the file: without the blanks round it, or
/// the error saying why it is none.
fn written_name(text: &str) -> Result<String, ConfigError> {
    checked_name(text)
        .map(str::to_string)
        .map_err(|why| ConfigError::Parse(name_refusal(why)))
}

/// `text` as a hop writer puts an optional line in the file: without the blanks round it, and
/// `None` for blanks alone, which says nothing at all.
fn written_line(text: &Option<String>) -> Option<String> {
    text.as_deref()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
}

/// `hop` as a writer puts it in the file, or the error saying why it cannot be written.
fn written(hop: &Hop) -> Result<Hop, ConfigError> {
    let what = match &hop.what {
        HopWhat::Declared { kind, name } => HopWhat::Declared {
            kind: *kind,
            name: written_name(name)?,
        },
        HopWhat::Linked { device } => HopWhat::Linked {
            device: written_name(device)?,
        },
    };
    Ok(Hop {
        what,
        note: written_line(&hop.note),
        after: written_line(&hop.after),
    })
}

/// The `[[hop]]` array of `doc`, for a writer that has already found a block in it.
fn hops_mut(doc: &mut DocumentMut) -> Result<&mut ArrayOfTables, ConfigError> {
    blocks_mut(doc, HOP)
}

/// Write the keys a hop is configured by into `table`: `name` and `kind`, or `device`, then
/// `note` and `after` where they say something, and none of the keys of the other sort or the
/// ones that say nothing.
///
/// Each key is set where it already sits, keeping the comments around it, by [`set_key`].
fn write_hop(table: &mut Table, hop: &Hop) {
    let mut nothing = String::new();
    match &hop.what {
        HopWhat::Declared { kind, name } => {
            table.remove("device");
            set_key(table, &mut nothing, "name", Value::from(name.as_str()));
            set_key(table, &mut nothing, "kind", Value::from(kind.name()));
        }
        HopWhat::Linked { device } => {
            table.remove("name");
            table.remove("kind");
            set_key(table, &mut nothing, "device", Value::from(device.as_str()));
        }
    }
    for (key, line) in [("note", &hop.note), ("after", &hop.after)] {
        match line {
            Some(line) => {
                set_key(table, &mut nothing, key, Value::from(line.as_str()));
            }
            None => {
                table.remove(key);
            }
        }
    }
}
