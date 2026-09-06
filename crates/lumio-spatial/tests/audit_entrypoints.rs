//! Validate actual public operations, not only constructors.
use lumio_kernel::ErrorCategory;
use lumio_spatial::*;
fn bounds() -> Aabb3 {
    Aabb3::new(
        Point3::new(0.0, 0.0, 0.0).unwrap(),
        Point3::new(1.0, 1.0, 1.0).unwrap(),
    )
    .unwrap()
}
#[test]
fn literal_nan_and_inverted_boxes_are_rejected_by_insert_and_query() {
    let bad = [
        Aabb3 {
            min: Point3 {
                x: f32::NAN,
                y: 0.0,
                z: 0.0,
            },
            max: bounds().max,
        },
        Aabb3 {
            min: bounds().max,
            max: bounds().min,
        },
    ];
    let mut context = SpatialContext::new();
    context
        .upsert(SpatialObjectId::from_raw(1), bounds())
        .unwrap();
    let sentinel = SpatialHit {
        query_ordinal: 99,
        object_id: SpatialObjectId::from_raw(99),
    };
    for aabb in bad {
        assert_eq!(
            context
                .upsert(SpatialObjectId::from_raw(2), aabb)
                .unwrap_err()
                .category(),
            ErrorCategory::InvalidArgument
        );
        let mut out = [sentinel];
        assert_eq!(
            context
                .query_aabb_batch(&[AabbQuery { aabb }], &mut out)
                .unwrap_err()
                .category(),
            ErrorCategory::InvalidArgument
        );
        assert_eq!(out, [sentinel]);
    }
}
#[test]
fn result_budget_precedes_staging_and_preserves_out() {
    let mut context = SpatialContext::with_backend(
        Box::new(GridReferenceIndex::new()),
        SpatialQueryLimits {
            max_queries: 2,
            max_hits: 1,
        },
    );
    for id in [1, 2] {
        context
            .upsert(SpatialObjectId::from_raw(id), bounds())
            .unwrap();
    }
    let sentinel = SpatialHit {
        query_ordinal: 99,
        object_id: SpatialObjectId::from_raw(99),
    };
    let mut out = [sentinel; 2];
    assert_eq!(
        context
            .query_aabb_batch(&[AabbQuery { aabb: bounds() }], &mut out)
            .unwrap_err()
            .category(),
        ErrorCategory::CapacityExceeded
    );
    assert_eq!(out, [sentinel; 2]);
}
#[test]
fn reference_capacity_is_live_objects_not_history() {
    let mut index = GridReferenceIndex::with_capacity(1);
    for id in 0..1000 {
        let id = SpatialObjectId::from_raw(id);
        index.upsert(id, bounds()).unwrap();
        assert_eq!(
            index
                .upsert(SpatialObjectId::from_raw(1001), bounds())
                .unwrap_err()
                .category(),
            ErrorCategory::CapacityExceeded
        );
        index.remove(id).unwrap();
    }
}
