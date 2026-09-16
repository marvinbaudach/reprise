//! Non-blocking access to Android's coordinated library writer.

use std::sync::{Mutex, MutexGuard, TryLockError};

use reprise_core::db::Db;

pub(crate) struct Poisoned;

pub(crate) fn try_lock_writer(writer: &Mutex<Db>) -> Result<Option<MutexGuard<'_, Db>>, Poisoned> {
    match writer.try_lock() {
        Ok(database) => Ok(Some(database)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Poisoned(_)) => Err(Poisoned),
    }
}
