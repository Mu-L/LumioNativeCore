//! Bounded handle slots and a free list.
//!
//! Generation overflow retires the slot permanently (T-handle-03 / R-00089).

use super::{ContextKey, Generation, Handle, HandleKey, SlotIndex};
use crate::error::{ErrorCategory, ErrorDetail, KernelError};

struct Slot<T> {
    generation: Generation,
    value: Option<T>,
}

pub struct HandleArena<T> {
    context: ContextKey,
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    retired: u32,
}

impl<T> HandleArena<T> {
    pub fn with_capacity(context: ContextKey, capacity: u32) -> Self {
        let cap = capacity as usize;
        let mut slots = Vec::with_capacity(cap);
        slots.extend((0..capacity).map(|_| Slot {
            generation: Generation::new(1),
            value: None,
        }));
        let mut free = Vec::with_capacity(cap);
        free.extend((0..capacity).rev());
        Self {
            context,
            slots,
            free,
            retired: 0,
        }
    }

    pub fn insert(&mut self, value: T) -> Result<Handle<T>, KernelError> {
        let Some(index) = self.free.pop() else {
            let limit = u64::from(self.capacity());
            return Err(KernelError::new(
                ErrorCategory::CapacityExceeded,
                ErrorDetail::LimitExceeded {
                    limit,
                    requested: limit + 1,
                },
            ));
        };

        let slot = &mut self.slots[index as usize];
        slot.value = Some(value);
        Ok(Handle::from_key(HandleKey {
            context: self.context,
            slot: SlotIndex::new(index),
            generation: slot.generation,
        }))
    }

    /// Resolve a live slot. Context is checked before bounds, occupancy, or generation.
    pub(crate) fn get(&self, handle: Handle<T>) -> Result<&T, KernelError> {
        let key = handle.key();
        if key.context != self.context {
            return Err(KernelError::new(
                ErrorCategory::WrongContext,
                ErrorDetail::None,
            ));
        }

        let Some(slot) = self.slots.get(key.slot.raw() as usize) else {
            return Err(KernelError::new(
                ErrorCategory::InvalidHandle,
                ErrorDetail::None,
            ));
        };

        if slot.value.is_none() {
            return Err(KernelError::new(
                ErrorCategory::AlreadyReleased,
                ErrorDetail::None,
            ));
        }
        if slot.generation != key.generation {
            return Err(KernelError::new(
                ErrorCategory::InvalidHandle,
                ErrorDetail::None,
            ));
        }
        Ok(slot.value.as_ref().expect("occupied slot checked above"))
    }

    pub fn remove(&mut self, handle: Handle<T>) -> Result<T, KernelError> {
        let key = handle.key();
        if key.context != self.context {
            return Err(KernelError::new(
                ErrorCategory::WrongContext,
                ErrorDetail::None,
            ));
        }

        let index = key.slot.raw();
        let Some(slot) = self.slots.get_mut(index as usize) else {
            return Err(KernelError::new(
                ErrorCategory::InvalidHandle,
                ErrorDetail::None,
            ));
        };

        if slot.value.is_none() {
            return Err(KernelError::new(
                ErrorCategory::AlreadyReleased,
                ErrorDetail::None,
            ));
        }
        if slot.generation != key.generation {
            return Err(KernelError::new(
                ErrorCategory::InvalidHandle,
                ErrorDetail::None,
            ));
        }

        let value = slot.value.take().expect("occupied slot checked above");
        match slot.generation.raw().checked_add(1) {
            Some(next) => {
                slot.generation = Generation::new(next);
                self.free.push(index);
            }
            None => {
                self.retired += 1;
            }
        }
        Ok(value)
    }

    /// Take live payloads so the caller can drop them outside the lock.
    pub(crate) fn drain_occupied(&mut self) -> Vec<T> {
        let mut drained = Vec::with_capacity(self.len() as usize);
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let Some(value) = slot.value.take() else {
                continue;
            };
            drained.push(value);
            match slot.generation.raw().checked_add(1) {
                Some(next) => {
                    slot.generation = Generation::new(next);
                    self.free.push(index as u32);
                }
                None => {
                    self.retired += 1;
                }
            }
        }
        drained
    }

    pub(crate) fn take_all(&mut self) -> Vec<T> {
        self.drain_occupied()
    }

    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> u32 {
        (self.slots.len() - self.free.len() - self.retired as usize) as u32
    }

    pub fn capacity(&self) -> u32 {
        self.slots.len() as u32
    }

    pub(crate) fn retired_slots(&self) -> u32 {
        self.retired
    }

    /// Test hook: set a free slot's generation without cycling insert/remove.
    #[doc(hidden)]
    #[cfg(feature = "test-support")]
    pub fn force_generation(&mut self, index: u32, generation: Generation) {
        self.slots[index as usize].generation = generation;
    }
}
