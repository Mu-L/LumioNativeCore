//! Byte helpers; compression experiments require the prototype feature.
#![forbid(unsafe_code)]
mod bounds;
mod checksum;
pub use bounds::CodecLimits;
pub use checksum::checksum_bytes;
#[cfg(feature = "prototype")]
mod compression;
#[cfg(feature = "prototype")]
mod resource;
#[cfg(feature = "prototype")]
pub use compression::{Lz4Adapter, ZstdAdapter};
#[cfg(feature = "prototype")]
pub use resource::{CodecResource, CodecWorkspace};
