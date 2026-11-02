// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which write goes to a device next, as bookkeeping alone: one write in flight per function;
//! behind it, in one line in arrival order across every function, the writes that wait — a
//! [`Mode::Latest`] write replaces the one waiting on its function and keeps that one's place,
//! a [`Mode::Ordered`] write is never dropped — and a waiting write is not overtaken by a write
//! that arrived after it, whatever function that one is on; and a preset recall, which holds the
//! writes to the functions it rewrites while it runs or waits and drops the held ones a later
//! recall rewrites, and may start ahead of a waiting write to another control. The writer task
//! carries the actions out; nothing here waits or sends.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use svolder_proto::FunctionId;

/// How a write waits behind the one in flight on its function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// It replaces the write waiting on its function, whose waiter is told it was superseded:
    /// a slider dragged.
    Latest,
    /// It waits its turn and is never dropped, and drops a waiting [`Mode::Latest`] write: a
    /// typed command, a relative step.
    Ordered,
}

/// A write the slots hold: how it waits, whether a recall holds it, and the caller's
/// own item, which the slots never look inside.
pub struct Queued<T> {
    /// How it waits.
    pub mode: Mode,
    /// Whether a recall holds it: it writes a function the recall rewrites.
    pub held: bool,
    /// Whether it goes out only once no held write is in flight and no echo is awaited for any
    /// of its `echoes`, and holds the line behind it until then: a write after whose effects
    /// nothing earlier may still be on its way.
    pub barrier: bool,
    /// The functions whose awaited echo ([`Slots::await_echo`]) a barrier write waits for: the
    /// controls it changes.
    pub echoes: Vec<FunctionId>,
    /// The caller's own.
    pub item: T,
}

/// What the writer does after a change of the slots.
pub enum Action<T, R> {
    /// Send this write, on this function, now.
    Send(FunctionId, T),
    /// Tell this write's waiter a newer write took its place.
    Supersede(T),
    /// Start this recall now.
    Start(R),
    /// Tell this recall's waiter a later one took its place before it started.
    Replace(R),
}

/// The writes and the recalls of one device.
pub struct Slots<T, R> {
    /// The functions with a write in flight, each with whether a recall holds it.
    flying: BTreeMap<FunctionId, bool>,
    /// The writes waiting, oldest first: each behind the one in flight on its function, or
    /// behind an older write that waits.
    queue: VecDeque<(FunctionId, Queued<T>)>,
    /// The writes a recall holds, in the order their functions first arrived, each
    /// with whether it arrived before the recall now waiting.
    held: Vec<(FunctionId, Queued<T>, bool)>,
    /// Whether a recall is running.
    running: bool,
    /// The functions whose echo is awaited after their write was answered: a write that is no
    /// barrier does not wait for it, and neither does the next write on the function. A barrier
    /// write waits for the ones it names, and a recall waiting to start ([`Slots::pick`]), a
    /// preset or a built-in setting, for any.
    echoing: BTreeSet<FunctionId>,
    /// The recall waiting to start: behind the one running, or behind a held write in
    /// flight or waiting.
    waiting: Option<R>,
}

impl<T, R> Default for Slots<T, R> {
    fn default() -> Self {
        Slots {
            flying: BTreeMap::new(),
            queue: VecDeque::new(),
            held: Vec::new(),
            running: false,
            echoing: BTreeSet::new(),
            waiting: None,
        }
    }
}

impl<T, R> Slots<T, R> {
    /// A write for function `id` has arrived. A write a recall holds is held while a recall runs
    /// or waits; any other write goes now when nothing is in flight on its function and no older
    /// write waits, and otherwise waits its turn as its mode says.
    pub fn submit(&mut self, id: FunctionId, queued: Queued<T>) -> Vec<Action<T, R>> {
        if queued.held && self.recall_pending() {
            return self.hold(id, queued);
        }
        let mut actions = self.wait(id, queued);
        actions.extend(self.pump());
        actions
    }

    /// The write in flight on `id` has been answered: the waiting writes that can go now do,
    /// oldest first, and a recall waiting for them starts once the writes held before it have
    /// gone and no held write is in flight or waiting.
    pub fn answered(&mut self, id: FunctionId) -> Vec<Action<T, R>> {
        self.flying.remove(&id);
        let mut actions = self.pump();
        actions.extend(self.settle());
        actions
    }

    /// The write on `id` was answered before its echo, which is awaited: a barrier write that
    /// waits for `id` goes out only after [`Slots::echo_in`], and so does a recall waiting to start.
    pub fn await_echo(&mut self, id: FunctionId) {
        self.echoing.insert(id);
    }

    /// The echo awaited on `id` has come or is no longer waited for: whatever waited for it can
    /// go now.
    pub fn echo_in(&mut self, id: FunctionId) -> Vec<Action<T, R>> {
        self.echoing.remove(&id);
        let mut actions = self.pump();
        actions.extend(self.settle());
        actions
    }

    /// A recall has been asked for. The held [`Mode::Latest`] writes it rewrites go —
    /// every one but those `keeps` says it leaves alone — and it starts now, or waits behind the
    /// recall running or a held write in flight or waiting, in place of any recall
    /// waiting; the writes held then are older than it and go out before it.
    pub fn pick(&mut self, recall: R, keeps: impl Fn(FunctionId) -> bool) -> Vec<Action<T, R>> {
        let mut actions = Vec::new();
        for (id, queued, older) in std::mem::take(&mut self.held) {
            match queued.mode == Mode::Latest && !keeps(id) {
                true => actions.push(Action::Supersede(queued.item)),
                false => self.held.push((id, queued, older)),
            }
        }
        if self.running || self.held_pending() {
            for (_, _, older) in &mut self.held {
                *older = true;
            }
            if let Some(replaced) = self.waiting.replace(recall) {
                actions.push(Action::Replace(replaced));
            }
        } else {
            self.running = true;
            actions.push(Action::Start(recall));
        }
        actions
    }

    /// The recall running is done. With one waiting, the writes held from before it go out first,
    /// in the order their functions first arrived, and it starts once no held write is in
    /// flight or waiting, holding the writes held since; with none, every held write goes, in
    /// that order.
    pub fn recall_done(&mut self) -> Vec<Action<T, R>> {
        self.running = false;
        if self.waiting.is_some() {
            return self.settle();
        }
        let mut actions = Vec::new();
        for (id, queued, _) in std::mem::take(&mut self.held) {
            actions.extend(self.wait(id, queued));
            actions.extend(self.pump());
        }
        actions
    }

    /// With no recall running and one waiting: send the writes held from before it, and start it
    /// once no held write is in flight or waiting.
    fn settle(&mut self) -> Vec<Action<T, R>> {
        let mut actions = Vec::new();
        if self.running || self.waiting.is_none() {
            return actions;
        }
        for (id, queued, older) in std::mem::take(&mut self.held) {
            match older {
                true => {
                    actions.extend(self.wait(id, queued));
                    actions.extend(self.pump());
                }
                false => self.held.push((id, queued, false)),
            }
        }
        if !self.held_pending()
            && let Some(recall) = self.waiting.take()
        {
            self.running = true;
            actions.push(Action::Start(recall));
        }
        actions
    }

    /// Every write and recall still waiting, taken out, for a writer that stops to answer.
    pub fn drain(&mut self) -> (Vec<T>, Vec<R>) {
        let mut writes: Vec<T> = self
            .queue
            .drain(..)
            .map(|(_, queued)| queued.item)
            .collect();
        writes.extend(self.held.drain(..).map(|(_, queued, _)| queued.item));
        (writes, self.waiting.take().into_iter().collect())
    }

    /// Whether a write is in flight on `id` or waiting for it.
    pub fn pending(&self, id: FunctionId) -> bool {
        self.flying.contains_key(&id) || self.queue.iter().any(|(waiting, _)| *waiting == id)
    }

    /// Whether a recall holds a write for function `id`.
    pub fn holds(&self, id: FunctionId) -> bool {
        self.held.iter().any(|(held, _, _)| *held == id)
    }

    /// The writes waiting for function `id`, oldest first.
    pub fn waiting_for(&self, id: FunctionId) -> impl Iterator<Item = &T> {
        self.queue
            .iter()
            .filter(move |(waiting, _)| *waiting == id)
            .map(|(_, queued)| &queued.item)
    }

    /// The functions with a write in flight.
    #[cfg(test)]
    pub fn flying(&self) -> Vec<FunctionId> {
        self.flying.keys().copied().collect()
    }

    /// How many writes a recall holds.
    #[cfg(test)]
    pub fn held(&self) -> usize {
        self.held.len()
    }

    /// Whether a recall is running.
    #[cfg(test)]
    pub fn running(&self) -> bool {
        self.running
    }

    /// The recall waiting to start.
    #[cfg(test)]
    pub fn waiting(&self) -> Option<&R> {
        self.waiting.as_ref()
    }

    fn recall_pending(&self) -> bool {
        self.running || self.waiting.is_some()
    }

    /// Whether a held write is in flight or waiting, or the echo of one answered is awaited.
    fn held_pending(&self) -> bool {
        self.flying.values().any(|held| *held)
            || !self.echoing.is_empty()
            || self.queue.iter().any(|(_, queued)| queued.held)
    }

    /// Hold a write behind a recall. A [`Mode::Latest`] one takes the place of the
    /// one held last for its function when that is [`Mode::Latest`] too, keeping its place in
    /// the order; a [`Mode::Ordered`] one drops the [`Mode::Latest`] ones held for its function
    /// before it.
    fn hold(&mut self, id: FunctionId, queued: Queued<T>) -> Vec<Action<T, R>> {
        let mut actions = Vec::new();
        match queued.mode {
            Mode::Latest => {
                if let Some(slot) = self.held.iter_mut().rev().find(|(held, _, _)| *held == id)
                    && slot.1.mode == Mode::Latest
                {
                    slot.2 = false;
                    let replaced = std::mem::replace(&mut slot.1, queued);
                    return vec![Action::Supersede(replaced.item)];
                }
            }
            Mode::Ordered => {
                for (held, waiting, older) in std::mem::take(&mut self.held) {
                    match held == id && waiting.mode == Mode::Latest {
                        true => actions.push(Action::Supersede(waiting.item)),
                        false => self.held.push((held, waiting, older)),
                    }
                }
            }
        }
        self.held.push((id, queued, false));
        actions
    }

    /// Put a write in the line of waiting writes, as its mode says.
    fn wait(&mut self, id: FunctionId, queued: Queued<T>) -> Vec<Action<T, R>> {
        let mut actions = Vec::new();
        match queued.mode {
            Mode::Latest => {
                if let Some(slot) = self
                    .queue
                    .iter_mut()
                    .rev()
                    .find(|(waiting, _)| *waiting == id)
                    && slot.1.mode == Mode::Latest
                {
                    let replaced = std::mem::replace(&mut slot.1, queued);
                    return vec![Action::Supersede(replaced.item)];
                }
            }
            Mode::Ordered => {
                for (waiting, write) in std::mem::take(&mut self.queue) {
                    match waiting == id && write.mode == Mode::Latest {
                        true => actions.push(Action::Supersede(write.item)),
                        false => self.queue.push_back((waiting, write)),
                    }
                }
            }
        }
        self.queue.push_back((id, queued));
        actions
    }

    /// Send the writes at the front of the line whose function has nothing in flight, and a
    /// barrier write once no held write is in flight and no echo it waits for is awaited; stop at
    /// the first that cannot go, so none is overtaken.
    fn pump(&mut self) -> Vec<Action<T, R>> {
        let mut actions = Vec::new();
        while let Some((id, queued)) = self.queue.front() {
            if self.flying.contains_key(id) {
                break;
            }
            if queued.barrier
                && (self.flying.values().any(|held| *held)
                    || queued.echoes.iter().any(|echo| self.echoing.contains(echo)))
            {
                break;
            }
            let Some((id, queued)) = self.queue.pop_front() else {
                break;
            };
            self.flying.insert(id, queued.held);
            actions.push(Action::Send(id, queued.item));
        }
        actions
    }
}

#[cfg(test)]
mod tests;
