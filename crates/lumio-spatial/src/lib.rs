//! Bounded AABB spatial kernels. Optimized backend and oracle share only the port.
#![forbid(unsafe_code)]
mod index;
mod query;
mod resource;
mod types;
#[cfg(feature = "rstar-backend")]
pub use index::RStarIndexAdapter;
pub use index::{GridReferenceIndex, SpatialIndexBackend};
pub use query::{AabbQuery, SpatialContext, SpatialHit, SpatialQueryLimits};
pub use resource::SpatialResource;
pub use types::{Aabb3, Point3, SpatialObjectId};
