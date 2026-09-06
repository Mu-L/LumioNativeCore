//! T-spatial-05 / R-00111: undersized SpatialHit out is unchanged and reports required.

use lumio_kernel::error::{ErrorCategory, ErrorDetail};
use lumio_spatial::{Aabb3, AabbQuery, Point3, SpatialContext, SpatialHit, SpatialObjectId};

fn aabb(min: (f32, f32, f32), max: (f32, f32, f32)) -> Aabb3 {
    Aabb3::new(
        Point3::new(min.0, min.1, min.2).expect("finite min"),
        Point3::new(max.0, max.1, max.2).expect("finite max"),
    )
    .expect("finite AABB")
}

fn id(v: u64) -> SpatialObjectId {
    SpatialObjectId::from_raw(v)
}

fn hit(query_ordinal: u32, object_id: u64) -> SpatialHit {
    SpatialHit {
        query_ordinal,
        object_id: id(object_id),
    }
}

fn assert_no_vendor(name: &str) {
    let lower = name.to_ascii_lowercase();
    for vendor in ["rstar", "kiddo", "parry", "ncollide"] {
        assert!(
            !lower.contains(vendor),
            "SpatialContext batch API leaked vendor type `{vendor}` via {name}"
        );
    }
}

#[test]
fn undersized_output_reports_required_without_overwrite() {
    for name in [
        std::any::type_name::<SpatialContext>(),
        std::any::type_name::<SpatialHit>(),
        std::any::type_name::<AabbQuery>(),
    ] {
        assert_no_vendor(name);
        assert!(
            name.contains("lumio_spatial"),
            "batch type must stay in crate paths: {name}"
        );
    }

    let mut ctx = SpatialContext::new();
    ctx.upsert(id(30), aabb((0.0, 0.0, 0.0), (2.0, 2.0, 2.0)))
        .expect("upsert 30");
    ctx.upsert(id(10), aabb((1.0, 1.0, 1.0), (3.0, 3.0, 3.0)))
        .expect("upsert 10");
    ctx.upsert(id(20), aabb((1.5, 1.5, 1.5), (1.5, 1.5, 1.5)))
        .expect("upsert 20");
    ctx.upsert(id(99), aabb((100.0, 100.0, 100.0), (101.0, 101.0, 101.0)))
        .expect("upsert 99");

    let queries = [AabbQuery {
        aabb: aabb((0.0, 0.0, 0.0), (3.0, 3.0, 3.0)),
    }];
    let sentinel = hit(0xFFFF_FFFF, 0xDEAD);
    let mut out = [sentinel];

    let err = ctx
        .query_aabb_batch(&queries, &mut out)
        .expect_err("undersized out must not overflow");
    assert_eq!(err.category(), ErrorCategory::BufferTooSmall);
    match err.detail() {
        ErrorDetail::RequiredCapacity { required, provided } => {
            assert!(
                *required >= 2,
                "one overlapping AABB query needs N>1 hits, required={required}"
            );
            assert_eq!(*provided, 1);
        }
        other => panic!("unexpected detail: {other:?}"),
    }
    assert_eq!(out[0], sentinel, "overflow must leave out slots unchanged");

    let mut large = [sentinel; 8];
    let n = ctx
        .query_aabb_batch(&queries, &mut large)
        .expect("sized out must succeed");
    let hits = &large[..n];
    assert_eq!(hits, &[hit(0, 10), hit(0, 20), hit(0, 30)]);
    let mut sorted = hits.to_vec();
    sorted.sort();
    assert_eq!(
        hits,
        sorted.as_slice(),
        "hits must sort by (query_ordinal, object_id)"
    );
    assert_eq!(large[n], sentinel, "unused slots stay untouched");
}
