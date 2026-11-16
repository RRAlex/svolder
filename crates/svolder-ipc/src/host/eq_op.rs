// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The one handler of a computer-EQ request, for the daemon and for a TUI with no daemon.
//!
//! Both own an [`EqControl`] and carry out the same [`HostEqOp`]s on it, so the mapping from a
//! request to the controller lives here once: the wire's 1-based point indexes, the point-type
//! word ([`parse_kind`]), the crossfeed's fields and the virtual speakers'. What only one of them does (the daemon's
//! preset lookup and its question about devices, the sink a daemon refuses) stays with it.

use svolder_host::eq::{CrossfeedEdit, Edit, EqControl, EqFailure, SpeakersEdit, parse_kind};
use svolder_profiles::tone::apo::SourceFilter;

use crate::HostEqOp;

/// Carry out `op` on `control`. A `Preset` or `SaveAs` is not the controller's: the caller
/// answers it before it gets here, and it is refused in words if it does. An `Apply` puts its
/// correction on the default sink: a caller that refuses another sink checks `sink` first.
pub async fn run_eq_op(control: &mut EqControl, op: HostEqOp) -> Result<(), EqFailure> {
    match op {
        HostEqOp::Mode { on } => control.set_on(on).await,
        HostEqOp::Apply {
            name,
            origin,
            correction,
            ..
        } => control.load(name, origin, correction).await,
        HostEqOp::Set {
            index,
            slot,
            kind,
            f,
            g,
            q,
            done,
        } => {
            let kind = kind.as_deref().map(parse_kind).transpose()?;
            let index = index_for(index, slot)?;
            control
                .edit(Edit::Set {
                    index,
                    slot,
                    kind,
                    f,
                    g,
                    q,
                    done,
                })
                .await
        }
        HostEqOp::Add { kind, f, g, q } => {
            let kind = parse_kind(&kind)?;
            control
                .edit(Edit::Add(SourceFilter { kind, f, g, q }))
                .await
        }
        HostEqOp::Remove { index, slot } => {
            let index = index_for(index, slot)?;
            control.edit(Edit::Remove { index, slot }).await
        }
        HostEqOp::Crossfeed {
            on,
            model,
            preset,
            amount_pct,
            crossover_hz,
            delay_ms,
            done,
        } => {
            control
                .crossfeed(CrossfeedEdit {
                    on,
                    model,
                    preset,
                    amount_pct,
                    crossover_hz,
                    delay_ms,
                    done,
                })
                .await
        }
        HostEqOp::Speakers { on, set, done } => {
            control.speakers(SpeakersEdit { on, set, done }).await
        }
        HostEqOp::Preset { .. } | HostEqOp::SaveAs { .. } => Err(EqFailure {
            what: "this request is not a change of the controller's".to_string(),
            next: "Recall a preset through the daemon".to_string(),
        }),
    }
}

/// A point's 0-based index from the wire, which counts from 1.
fn zero_based(index: u8) -> Result<usize, EqFailure> {
    usize::from(index).checked_sub(1).ok_or_else(|| EqFailure {
        what: "points count from 1".to_string(),
        next: "Choose a point that is there".to_string(),
    })
}

/// The 0-based index a change means: with a slot the controller finds the point by the slot and
/// the index is not looked at, else it counts from 1.
fn index_for(index: u8, slot: Option<u8>) -> Result<usize, EqFailure> {
    match slot {
        Some(_) => Ok(usize::from(index.max(1)) - 1),
        None => zero_based(index),
    }
}
