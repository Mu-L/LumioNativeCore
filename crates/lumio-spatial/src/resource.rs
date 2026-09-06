//! Context-owned index with a read/write fence and actual storage reclamation.
use crate::query::{AabbQuery, SpatialContext, SpatialHit};
use crate::types::{Aabb3, SpatialObjectId};
use lumio_kernel::context::{CancelReason, ContextResource, Deadline, QuiesceReport, QuiesceState};
use lumio_kernel::error::{ErrorCategory, ErrorDetail, KernelError, KernelResult};
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, Ordering};
pub struct SpatialResource {
    inner: RwLock<Option<SpatialContext>>,
    closing: AtomicBool,
    destroyed: AtomicBool,
}
impl Default for SpatialResource {
    fn default() -> Self {
        Self::new()
    }
}
impl SpatialResource {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(Some(SpatialContext::new())),
            closing: AtomicBool::new(false),
            destroyed: AtomicBool::new(false),
        }
    }
    pub fn upsert(&mut self, id: SpatialObjectId, aabb: Aabb3) -> KernelResult<()> {
        self.upsert_shared(id, aabb)
    }
    pub fn upsert_shared(&self, id: SpatialObjectId, aabb: Aabb3) -> KernelResult<()> {
        self.ensure_live()?;
        let mut guard = self
            .inner
            .write()
            .map_err(|_| err(ErrorCategory::InternalInvariant))?;
        self.ensure_live()?;
        guard
            .as_mut()
            .ok_or_else(|| err(ErrorCategory::ContextDestroyed))?
            .upsert(id, aabb)
    }
    pub fn query_aabb_batch(
        &self,
        queries: &[AabbQuery],
        out: &mut [SpatialHit],
    ) -> KernelResult<usize> {
        self.ensure_live()?;
        let guard = self
            .inner
            .read()
            .map_err(|_| err(ErrorCategory::InternalInvariant))?;
        self.ensure_live()?;
        guard
            .as_ref()
            .ok_or_else(|| err(ErrorCategory::ContextDestroyed))?
            .query_aabb_batch(queries, out)
    }
    fn ensure_live(&self) -> KernelResult<()> {
        if self.destroyed.load(Ordering::Acquire) {
            return Err(err(ErrorCategory::ContextDestroyed));
        }
        if self.closing.load(Ordering::Acquire) {
            return Err(err(ErrorCategory::ContextClosing));
        }
        Ok(())
    }
}
fn err(category: ErrorCategory) -> KernelError {
    KernelError::new(category, ErrorDetail::None)
}
impl ContextResource for SpatialResource {
    fn name(&self) -> &'static str {
        "spatial"
    }
    fn cancel_requested(&self, _reason: CancelReason) {
        self.closing.store(true, Ordering::Release);
    }
    fn quiesce(&self, _deadline: Deadline) -> KernelResult<QuiesceReport> {
        self.closing.store(true, Ordering::Release);
        let state = match self.inner.try_write() {
            Ok(_) => QuiesceState::Quiesced,
            Err(std::sync::TryLockError::WouldBlock) => QuiesceState::Pending { remaining: 1 },
            Err(_) => return Err(err(ErrorCategory::InternalInvariant)),
        };
        Ok(QuiesceReport { state })
    }
    fn destroy(&self) -> KernelResult<()> {
        self.closing.store(true, Ordering::Release);
        let old = self
            .inner
            .try_write()
            .map_err(|_| err(ErrorCategory::ContextClosing))?
            .take();
        self.destroyed.store(true, Ordering::Release);
        drop(old);
        Ok(())
    }
}
