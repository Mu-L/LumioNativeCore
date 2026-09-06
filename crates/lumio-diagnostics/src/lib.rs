//! Experimental bounded recorder. No production RecordPort integration is claimed.
#![forbid(unsafe_code)]
#[cfg(feature = "prototype")]
mod queue;
#[cfg(feature = "prototype")]
mod record;
#[cfg(feature = "prototype")]
mod recorder;
#[cfg(feature = "prototype")]
mod resource;
#[cfg(feature = "prototype")]
pub use queue::RecordQueue;
#[cfg(feature = "prototype")]
pub use record::{KernelRecordRef, OwnedKernelRecord};
#[cfg(feature = "prototype")]
pub use recorder::{BoundedRecorder, RecordDisposition, RecorderCounters};
#[cfg(feature = "prototype")]
pub use resource::DiagnosticsResource;
