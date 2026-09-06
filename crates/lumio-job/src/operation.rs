//! Rust-only executable operation registry. No managed callbacks or ABI IDs.
use crate::id::OperationId;
use crate::worker::JobExecution;
use lumio_kernel::error::{ErrorCategory, ErrorDetail, KernelError, KernelResult};
use std::collections::HashMap;
use std::sync::Arc;

pub trait TypedKernel: Send + Sync + 'static {
    fn operation_id(&self) -> OperationId;
    /// Write at most output.len() bytes and return the initialized length.
    /// Long kernels must check control.check_cancelled() at bounded intervals.
    /// A metadata-only registration is explicitly unavailable, never successful.
    fn execute(
        &self,
        _input: &[u8],
        _output: &mut [u8],
        _control: &JobExecution,
    ) -> KernelResult<usize> {
        Err(KernelError::new(
            ErrorCategory::CapabilityUnavailable,
            ErrorDetail::StaticMessage("operation has no executable kernel"),
        ))
    }
}
#[derive(Default)]
pub struct OperationRegistry {
    kernels: HashMap<OperationId, Arc<dyn TypedKernel>>,
}
impl OperationRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn register(&mut self, kernel: Arc<dyn TypedKernel>) -> KernelResult<()> {
        let id = kernel.operation_id();
        if self.kernels.contains_key(&id) {
            return Err(KernelError::new(
                ErrorCategory::InvalidArgument,
                ErrorDetail::None,
            ));
        }
        self.kernels.insert(id, kernel);
        Ok(())
    }
    pub fn get(&self, id: OperationId) -> Option<Arc<dyn TypedKernel>> {
        self.kernels.get(&id).cloned()
    }
}
