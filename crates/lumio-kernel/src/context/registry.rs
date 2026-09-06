//! Ordered resource ownership; payload destructors run outside the registry lock.

use super::resource::ContextResource;
use crate::error::{ErrorCategory, ErrorDetail, KernelError};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceRegistration {
    pub index: u32,
    pub name: &'static str,
}

#[derive(Default)]
pub struct ResourceRegistry {
    items: Mutex<Vec<Arc<dyn ContextResource>>>,
}

impl ResourceRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(
        &self,
        resource: Arc<dyn ContextResource>,
    ) -> Result<ResourceRegistration, KernelError> {
        self.register_named(resource.name(), resource, u32::MAX)
    }
    pub(crate) fn register_named(
        &self,
        name: &'static str,
        resource: Arc<dyn ContextResource>,
        limit: u32,
    ) -> Result<ResourceRegistration, KernelError> {
        let mut items = self.items.lock().expect("resource registry lock");
        if items.len() >= limit as usize {
            return Err(KernelError::new(
                ErrorCategory::CapacityExceeded,
                ErrorDetail::LimitExceeded {
                    limit: u64::from(limit),
                    requested: items.len() as u64 + 1,
                },
            ));
        }
        let index = items.len() as u32;
        items.push(resource);
        Ok(ResourceRegistration { index, name })
    }
    pub fn snapshot_names(&self) -> Vec<&'static str> {
        self.snapshot().iter().map(|r| r.name()).collect()
    }
    pub fn snapshot(&self) -> Vec<Arc<dyn ContextResource>> {
        self.items.lock().expect("resource registry lock").clone()
    }
    pub(crate) fn clear(&self) {
        let old = std::mem::take(&mut *self.items.lock().expect("resource registry lock"));
        drop(old);
    }
}
