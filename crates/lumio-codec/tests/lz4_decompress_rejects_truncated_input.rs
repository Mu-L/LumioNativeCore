//! Missing provider must not claim successful or partial frame parsing.
use lumio_codec::{CodecLimits, Lz4Adapter};
use lumio_kernel::error::ErrorCategory;
#[test]
fn unavailable_lz4_provider_reports_its_actual_capability() {
    let limits = CodecLimits {
        max_input_bytes: 16,
        max_output_bytes: 32,
        max_expansion_ratio: 2,
    };
    assert_eq!(
        Lz4Adapter::decompress_bounded(&[], &limits)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidArgument
    );
    assert_eq!(
        Lz4Adapter::decompress_bounded(&[1], &limits)
            .unwrap_err()
            .category(),
        ErrorCategory::CapabilityUnavailable
    );
    assert_eq!(
        Lz4Adapter::decompress_bounded(&[1; 17], &limits)
            .unwrap_err()
            .category(),
        ErrorCategory::CapacityExceeded
    );
}
