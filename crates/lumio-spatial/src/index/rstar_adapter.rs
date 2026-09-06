//! Real rstar 0.12.2 backend. The brute-force oracle is not used here.
use super::SpatialIndexBackend;
use crate::types::{Aabb3, SpatialObjectId};
use lumio_kernel::error::{ErrorCategory, ErrorDetail, KernelError, KernelResult};
use rstar::{AABB, RTree, RTreeObject};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
struct Entry {
    id: SpatialObjectId,
    bounds: Aabb3,
}
fn envelope(a: Aabb3) -> AABB<[f64; 3]> {
    // f64 also avoids f32 overflow in the vendor's envelope area heuristics.
    AABB::from_corners(
        [a.min.x as f64, a.min.y as f64, a.min.z as f64],
        [a.max.x as f64, a.max.y as f64, a.max.z as f64],
    )
}
impl RTreeObject for Entry {
    type Envelope = AABB<[f64; 3]>;
    fn envelope(&self) -> Self::Envelope {
        envelope(self.bounds)
    }
}
#[derive(Clone, Debug)]
pub struct RStarIndexAdapter {
    tree: RTree<Entry>,
    objects: BTreeMap<SpatialObjectId, Aabb3>,
    capacity: usize,
}
impl Default for RStarIndexAdapter {
    fn default() -> Self {
        Self::new()
    }
}
impl RStarIndexAdapter {
    pub fn new() -> Self {
        Self::with_capacity(65536)
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            tree: RTree::new(),
            objects: BTreeMap::new(),
            capacity,
        }
    }
}
impl SpatialIndexBackend for RStarIndexAdapter {
    fn upsert(&mut self, id: SpatialObjectId, aabb: Aabb3) -> KernelResult<()> {
        aabb.validate()?;
        if !self.objects.contains_key(&id) && self.objects.len() >= self.capacity {
            return Err(KernelError::new(
                ErrorCategory::CapacityExceeded,
                ErrorDetail::None,
            ));
        }
        if let Some(old) = self.objects.get(&id).copied()
            && self.tree.remove(&Entry { id, bounds: old }).is_none()
        {
            return Err(KernelError::new(
                ErrorCategory::InternalInvariant,
                ErrorDetail::None,
            ));
        }
        self.tree.insert(Entry { id, bounds: aabb });
        self.objects.insert(id, aabb);
        Ok(())
    }
    fn remove(&mut self, id: SpatialObjectId) -> KernelResult<()> {
        let bounds = *self
            .objects
            .get(&id)
            .ok_or_else(|| KernelError::new(ErrorCategory::InvalidHandle, ErrorDetail::None))?;
        if self.tree.remove(&Entry { id, bounds }).is_none() {
            return Err(KernelError::new(
                ErrorCategory::InternalInvariant,
                ErrorDetail::None,
            ));
        }
        self.objects.remove(&id);
        Ok(())
    }
    fn query_aabb(&self, aabb: Aabb3, out: &mut [SpatialObjectId]) -> KernelResult<usize> {
        aabb.validate()?;
        let envelope = envelope(aabb);
        let required = self.tree.locate_in_envelope_intersecting(&envelope).count();
        if required > out.len() {
            return Err(KernelError::buffer_too_small(
                required as u64,
                out.len() as u64,
            ));
        }
        for (slot, entry) in out[..required]
            .iter_mut()
            .zip(self.tree.locate_in_envelope_intersecting(&envelope))
        {
            *slot = entry.id;
        }
        out[..required].sort_unstable();
        Ok(required)
    }
}
