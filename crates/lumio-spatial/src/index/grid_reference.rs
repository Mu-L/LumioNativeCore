//! Independent brute-force oracle, not an optimized grid or R-tree.
use super::SpatialIndexBackend;
use crate::types::{Aabb3, SpatialObjectId};
use lumio_kernel::error::{ErrorCategory, ErrorDetail, KernelError, KernelResult};
use std::collections::BTreeMap;
#[derive(Clone, Debug)]
pub struct GridReferenceIndex {
    objects: BTreeMap<SpatialObjectId, Aabb3>,
    capacity: usize,
}
impl Default for GridReferenceIndex {
    fn default() -> Self {
        Self::new()
    }
}
impl GridReferenceIndex {
    pub fn new() -> Self {
        Self::with_capacity(65536)
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            objects: BTreeMap::new(),
            capacity,
        }
    }
}
fn overlaps(a: Aabb3, b: Aabb3) -> bool {
    !(a.max.x < b.min.x
        || b.max.x < a.min.x
        || a.max.y < b.min.y
        || b.max.y < a.min.y
        || a.max.z < b.min.z
        || b.max.z < a.min.z)
}
impl SpatialIndexBackend for GridReferenceIndex {
    fn upsert(&mut self, id: SpatialObjectId, aabb: Aabb3) -> KernelResult<()> {
        aabb.validate()?;
        if !self.objects.contains_key(&id) && self.objects.len() >= self.capacity {
            return Err(KernelError::new(
                ErrorCategory::CapacityExceeded,
                ErrorDetail::None,
            ));
        }
        self.objects.insert(id, aabb);
        Ok(())
    }
    fn remove(&mut self, id: SpatialObjectId) -> KernelResult<()> {
        self.objects
            .remove(&id)
            .map(|_| ())
            .ok_or_else(|| KernelError::new(ErrorCategory::InvalidHandle, ErrorDetail::None))
    }
    fn query_aabb(&self, aabb: Aabb3, out: &mut [SpatialObjectId]) -> KernelResult<usize> {
        aabb.validate()?;
        let required = self
            .objects
            .values()
            .filter(|item| overlaps(**item, aabb))
            .count();
        if required > out.len() {
            return Err(KernelError::buffer_too_small(
                required as u64,
                out.len() as u64,
            ));
        }
        for (destination, (id, _)) in out[..required].iter_mut().zip(
            self.objects
                .iter()
                .filter(|(_, item)| overlaps(**item, aabb)),
        ) {
            *destination = *id;
        }
        Ok(required)
    }
}
