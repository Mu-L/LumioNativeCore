//! Expansion policy is independent of a decoder that is not installed.
use lumio_codec::{CodecLimits, ZstdAdapter};
use lumio_kernel::error::ErrorCategory;
#[test]
fn expansion_policy_is_enforced_without_inventing_a_frame_size() {
    let limits = CodecLimits {
        max_input_bytes: 16,
        max_output_bytes: 32,
        max_expansion_ratio: 2,
    };
    assert!(limits.expansion_would_exceed(8, 17));
    assert!(!limits.expansion_would_exceed(8, 16));
    assert!(limits.expansion_would_exceed(16, 33));
    assert_eq!(
        ZstdAdapter::decompress_bounded(&[1; 8], &limits)
            .unwrap_err()
            .category(),
        ErrorCategory::CapabilityUnavailable
    );
    assert_eq!(
        ZstdAdapter::decompress_bounded(&[1; 17], &limits)
            .unwrap_err()
            .category(),
        ErrorCategory::CapacityExceeded
    );
}
