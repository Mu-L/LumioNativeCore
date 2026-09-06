//! Host-computed guard results. The kernel never evaluates a guard itself.

use crate::ids::GuardId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuardValue {
    True,
    False,
    /// Not supplied by the host — an error, never treated as `False` (contract §3).
    Missing,
}

/// Borrowed `(GuardId, bool)` pairs. Duplicated ids make the frame invalid.
#[derive(Clone, Copy, Debug)]
pub struct GuardFrame<'a> {
    entries: &'a [(GuardId, bool)],
}

impl<'a> GuardFrame<'a> {
    pub const fn new(entries: &'a [(GuardId, bool)]) -> Self {
        Self { entries }
    }

    pub fn entries(&self) -> &'a [(GuardId, bool)] {
        self.entries
    }

    pub fn lookup(&self, guard: GuardId) -> GuardValue {
        match self.entries.iter().find(|(g, _)| *g == guard) {
            Some((_, true)) => GuardValue::True,
            Some((_, false)) => GuardValue::False,
            None => GuardValue::Missing,
        }
    }

    /// `false` when any `GuardId` appears twice.
    pub fn is_well_formed(&self) -> bool {
        self.entries
            .iter()
            .enumerate()
            .all(|(i, (g, _))| !self.entries[..i].iter().any(|(h, _)| h == g))
    }
}
