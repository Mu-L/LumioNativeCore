//! Check Cargo's dependency graph, not vendor words in test target names.
use lumio_codec::CodecLimits;
use lumio_kernel::error::ErrorCategory;
use std::process::Command;
const FORBIDDEN_VENDORS: &[&str] = &["zstd", "lz4", "lz4_flex", "flate2", "brotli", "snap"];
fn assert_zero_rejected(limits: CodecLimits) {
    assert_eq!(
        limits.validate().unwrap_err().category(),
        ErrorCategory::InvalidArgument
    );
}
#[test]
fn default_build_has_no_codec_vendor_dependencies() {
    let output = Command::new("cargo")
        .args([
            "tree",
            "-p",
            "lumio-codec",
            "--no-default-features",
            "-e",
            "normal",
            "--prefix",
            "none",
        ])
        .output()
        .expect("cargo tree");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(name) = line.split_whitespace().next() {
            assert!(
                !FORBIDDEN_VENDORS.contains(&name),
                "unexpected default supplier {name}"
            );
        }
    }
    let ok = CodecLimits {
        max_input_bytes: 1024,
        max_output_bytes: 2048,
        max_expansion_ratio: 2,
    };
    ok.validate().unwrap();
    assert_zero_rejected(CodecLimits {
        max_input_bytes: 0,
        ..ok
    });
    assert_zero_rejected(CodecLimits {
        max_output_bytes: 0,
        ..ok
    });
    assert_zero_rejected(CodecLimits {
        max_expansion_ratio: 0,
        ..ok
    });
    assert!(ok.expansion_would_exceed(10, 21));
    assert!(!ok.expansion_would_exceed(10, 20));
}
