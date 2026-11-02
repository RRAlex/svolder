// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon's one rule for a lock a panic left poisoned: take what it holds and go on. What
//! the daemon keeps behind a lock is a report or a slot the next write puts right — a device's
//! link and identity, its history, the bridge's and the MCP server's slots, the registry — so a
//! panic in one holder must neither freeze it nor make every later caller panic too.

use std::sync::{Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// `mutex`, locked whatever a holder that panicked left behind.
pub fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `lock`, read whatever a writer that panicked left behind.
pub fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

/// `lock`, written whatever a writer that panicked left behind.
pub fn written<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(PoisonError::into_inner)
}
