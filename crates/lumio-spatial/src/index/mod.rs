//! Object-safe spatial port; vendor types remain inside adapters.
mod grid_reference;
#[cfg(feature = "rstar-backend")]
mod rstar_adapter;
use crate::types::{Aabb3, SpatialObjectId};
pub use grid_reference::GridReferenceIndex;
use lumio_kernel::error::KernelResult;
#[cfg(feature = "rstar-backend")]
pub use rstar_adapter::RStarIndexAdapter;
pub trait SpatialIndexBackend: Send + Sync + 'static {
    fn upsert(&mut self, id: SpatialObjectId, aabb: Aabb3) -> KernelResult<()>;
    fn remove(&mut self, id: SpatialObjectId) -> KernelResult<()>;
    /// Validate input, sort IDs, and leave out unchanged on any failure.
    fn query_aabb(&self, aabb: Aabb3, out: &mut [SpatialObjectId]) -> KernelResult<usize>;
}
