//! Default graph checks run without prototypes; resource tests opt in explicitly.

use std::process::Command;

#[test]
fn core_default_graph_has_no_diagnostics_dependency() {
    for name in [
        "lumio-kernel",
        "lumio-job",
        "lumio-platform",
        "lumio-spatial",
        "lumio-timer",
    ] {
        let output = Command::new("cargo")
            .args([
                "tree",
                "-p",
                name,
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
            assert_ne!(
                line.split_whitespace().next(),
                Some("lumio-diagnostics"),
                "{name} default graph must not include the diagnostics prototype"
            );
        }
    }
}

#[cfg(feature = "prototype")]
#[test]
fn diagnostics_resource_close_drops_late_record() {
    use lumio_diagnostics::{
        BoundedRecorder, DiagnosticsResource, KernelRecordRef, RecordDisposition,
    };
    use lumio_kernel::capability::ConfiguredLimits;
    use lumio_kernel::context::{CancelReason, ContextConfig, ContextResource, KernelContext};
    use lumio_platform::Deadline;
    use std::sync::Arc;

    let resource = Arc::new(DiagnosticsResource::new(
        BoundedRecorder::with_capacity(4, 32).expect("valid recorder"),
    ));
    assert_eq!(
        resource.try_record(KernelRecordRef {
            fields: &["a"],
            payload: b"1",
        }),
        RecordDisposition::Accepted
    );
    let context = KernelContext::create(ContextConfig {
        limits: ConfiguredLimits {
            max_handles: 4,
            max_native_bytes: 64,
            max_jobs_queued: 1,
            max_jobs_running: 1,
            max_completion_items: 1,
        },
        quiesce_deadline: Deadline::NONE,
    })
    .expect("context");
    let registration = context
        .register_resource(Arc::clone(&resource) as Arc<dyn ContextResource>)
        .expect("register diagnostics");
    assert_eq!(registration.name, "diagnostics");
    assert_eq!(resource.name(), "diagnostics");
    context
        .close(CancelReason::ContextClosing, Deadline::NONE)
        .expect("close");
    assert_eq!(
        resource.try_record(KernelRecordRef {
            fields: &["b"],
            payload: b"22",
        }),
        RecordDisposition::DroppedFull
    );
}
