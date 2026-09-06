//! Capacity-first batch queries with an injectable, independently tested backend.
use crate::index::SpatialIndexBackend;
use crate::types::{Aabb3, SpatialObjectId};
use lumio_kernel::error::{ErrorCategory, ErrorDetail, KernelError, KernelResult};
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct SpatialHit {
    pub query_ordinal: u32,
    pub object_id: SpatialObjectId,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AabbQuery {
    pub aabb: Aabb3,
}
#[derive(Clone, Copy, Debug)]
pub struct SpatialQueryLimits {
    pub max_queries: usize,
    pub max_hits: usize,
}
impl Default for SpatialQueryLimits {
    fn default() -> Self {
        Self {
            max_queries: 4096,
            max_hits: 262144,
        }
    }
}
pub struct SpatialContext {
    index: Box<dyn SpatialIndexBackend>,
    limits: SpatialQueryLimits,
}
impl Default for SpatialContext {
    fn default() -> Self {
        Self::new()
    }
}
impl SpatialContext {
    pub fn new() -> Self {
        #[cfg(feature = "rstar-backend")]
        let backend = crate::index::RStarIndexAdapter::new();
        #[cfg(not(feature = "rstar-backend"))]
        let backend = crate::index::GridReferenceIndex::new();
        Self::with_backend(Box::new(backend), SpatialQueryLimits::default())
    }
    pub fn with_backend(index: Box<dyn SpatialIndexBackend>, limits: SpatialQueryLimits) -> Self {
        Self { index, limits }
    }
    pub fn upsert(&mut self, id: SpatialObjectId, aabb: Aabb3) -> KernelResult<()> {
        aabb.validate()?;
        self.index.upsert(id, aabb)
    }
    pub fn remove(&mut self, id: SpatialObjectId) -> KernelResult<()> {
        self.index.remove(id)
    }
    pub fn query_aabb_batch(
        &self,
        queries: &[AabbQuery],
        out: &mut [SpatialHit],
    ) -> KernelResult<usize> {
        if queries.len() > self.limits.max_queries || queries.len() > u32::MAX as usize {
            return Err(capacity());
        }
        let mut sizes = Vec::with_capacity(queries.len());
        let mut total = 0usize;
        for query in queries {
            query.aabb.validate()?;
            let required = match self.index.query_aabb(query.aabb, &mut []) {
                Ok(0) => 0,
                Ok(_) => return Err(invariant()),
                Err(error) if error.category() == ErrorCategory::BufferTooSmall => {
                    match error.detail() {
                        ErrorDetail::RequiredCapacity { required, .. } => {
                            usize::try_from(*required).map_err(|_| capacity())?
                        }
                        _ => return Err(error),
                    }
                }
                Err(error) => return Err(error),
            };
            total = total
                .checked_add(required)
                .filter(|n| *n <= self.limits.max_hits)
                .ok_or_else(capacity)?;
            sizes.push(required);
        }
        if total > out.len() {
            return Err(KernelError::buffer_too_small(
                total as u64,
                out.len() as u64,
            ));
        }
        let mut staged = Vec::with_capacity(total);
        let mut scratch = Vec::new();
        for (ordinal, (query, required)) in queries.iter().zip(sizes).enumerate() {
            scratch.resize(required, SpatialObjectId::from_raw(0));
            let written = self.index.query_aabb(query.aabb, &mut scratch)?;
            if written != required {
                return Err(invariant());
            }
            staged.extend(scratch.iter().map(|id| SpatialHit {
                query_ordinal: ordinal as u32,
                object_id: *id,
            }));
        }
        staged.sort_unstable();
        out[..total].copy_from_slice(&staged);
        Ok(total)
    }
}
fn capacity() -> KernelError {
    KernelError::new(ErrorCategory::CapacityExceeded, ErrorDetail::None)
}
fn invariant() -> KernelError {
    KernelError::new(ErrorCategory::InternalInvariant, ErrorDetail::None)
}
