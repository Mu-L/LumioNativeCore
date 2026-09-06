//! Explicitly unavailable experimental provider. No fabricated frame sizes.
use crate::bounds::CodecLimits;
use lumio_kernel::error::{ErrorCategory, ErrorDetail, KernelError, KernelResult};
pub struct ZstdAdapter;
impl ZstdAdapter {
    pub fn decompress_bounded(input: &[u8], limits: &CodecLimits) -> KernelResult<Vec<u8>> {
        limits.validate()?;
        if input.is_empty() {
            return Err(KernelError::new(
                ErrorCategory::InvalidArgument,
                ErrorDetail::None,
            ));
        }
        if input.len() as u64 > limits.max_input_bytes {
            return Err(KernelError::new(
                ErrorCategory::CapacityExceeded,
                ErrorDetail::LimitExceeded {
                    limit: limits.max_input_bytes,
                    requested: input.len() as u64,
                },
            ));
        }
        Err(KernelError::new(
            ErrorCategory::CapabilityUnavailable,
            ErrorDetail::StaticMessage("zstd provider is not installed"),
        ))
    }
}
