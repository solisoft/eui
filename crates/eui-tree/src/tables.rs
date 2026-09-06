//! The four append-only session tables.
//!
//! Ids are dense, assigned by the server from 1, and never reused: a table
//! only grows for the life of the session, which is why the protocol ceilings
//! bound it and why a session wanting a clean slate is a new session.

use crate::error::{ApplyError, Result, Table};

/// One define-once table, indexed by id.
#[derive(Debug, Clone)]
pub(crate) struct DefineOnce<T> {
    slots: Vec<Option<T>>,
    which: Table,
    max_id: u32,
}

impl<T> DefineOnce<T> {
    pub(crate) fn new(which: Table, max_id: u32) -> Self {
        Self { slots: Vec::new(), which, max_id }
    }

    /// Define `id`. Fails if `id` is 0, above the ceiling, or already taken.
    pub(crate) fn define(&mut self, id: u32, value: T) -> Result<()> {
        if id == 0 || id > self.max_id {
            return Err(ApplyError::IdOutOfRange(self.which, id));
        }
        let ix = id as usize;
        if self.slots.len() <= ix {
            // Grown by at most one ceiling's worth over a whole session, and
            // the ceiling is a protocol constant, so this cannot run away.
            self.slots.resize_with(ix.saturating_add(1), || None);
        }
        match self.slots.get_mut(ix) {
            Some(slot @ None) => {
                *slot = Some(value);
                Ok(())
            }
            _ => Err(ApplyError::Redefined(self.which, id)),
        }
    }

    /// Look up `id`, failing with the table name if it is not defined.
    pub(crate) fn require(&self, id: u32) -> Result<&T> {
        self.get(id).ok_or(ApplyError::Undefined(self.which, id))
    }

    /// Look up `id`.
    pub(crate) fn get(&self, id: u32) -> Option<&T> {
        self.slots.get(id as usize).and_then(Option::as_ref)
    }

    /// Number of definitions.
    pub(crate) fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }
}
