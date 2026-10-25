//! Regression tests and geometric validation suite for CADCraft B-Rep.

use std::collections::HashSet;

use cadcraft_geom::Vec3;

use crate::error::BrepError;
use crate::recipe::{BooleanKind, EvaluateRecipe, FeatureOp, Recipe};
use crate::snapshot::{BREP_FORMAT_VERSION, KERNEL_REVISION};
use crate::solid::{BrepSolid, SubtractOptions};

#[test]
fn test_box_creation() {
    let b = BrepSolid::box_dimensions(Vec3::new(10.0, 20.0, 30.0), Vec3::new(100.0, 50.0, 25.0))
        .expect("box should succeed");
    let bbox = b.bounding_box();
    assert!((bbox.min.x - 10.0).abs() < 1e-4);
    assert!((bbox.min.y - 20.0).abs() < 1e-4);
    assert!((bbox.min.z - 30.0).abs() < 1e-4);
    assert!((bbox.max.x - 110.0).abs() < 1e-4);
    assert!((bbox.max.y - 70.0).abs() < 1e-4);
    assert!((bbox.max.z - 55.0).abs() < 1e-4);
    assert_eq!(b.boundaries_count(), 1);

    // Negative dimensions must fail gracefully
    let err = BrepSolid::box_dimensions(Vec3::ZERO, Vec3::new(-10.0, 50.0, 25.0)).unwrap_err();
    assert!(matches!(err, BrepError::InvalidGeometry(_)));
}

#[test]
fn test_cylinder_creation() {
    let cyl = BrepSolid::cylinder(Vec3::new(0.0, 0.0, 0.0), 10.0, 50.0, Vec3::new(0.0, 0.0, 1.0))
        .expect("cylinder should succeed");
    let bbox = cyl.bounding_box();
    assert!((bbox.min.x - (-10.0)).abs() < 1.0);
    assert!((bbox.max.x - 10.0).abs() < 1.0);
    assert!((bbox.min.z - 0.0).abs() < 1e-4);
    assert!((bbox.max.z - 50.0).abs() < 1e-4);

    let vol = cyl.volume().expect("cylinder volume");
    let expected_vol = std::f64::consts::PI * 100.0 * 50.0;
    assert!((vol - expected_vol).abs() / expected_vol < 0.01);
}

#[test]
fn test_extrusion_closed_polygon() {
    let profile = vec![
        Vec3::new(10.0, 10.0, 0.0),
        Vec3::new(40.0, 10.0, 0.0),
        Vec3::new(40.0, 30.0, 0.0),
        Vec3::new(10.0, 30.0, 0.0),
    ];
    let prism = BrepSolid::extrude(&profile, Vec3::new(0.0, 0.0, 25.0)).expect("extrusion should succeed");
    let vol = prism.volume().expect("prism volume");
    let expected_vol = 30.0 * 20.0 * 25.0; // 15,000.0
    assert!((vol - expected_vol).abs() / expected_vol < 0.01);
}

#[test]
fn test_boolean_coplanar_rejection() {
    let cube = BrepSolid::box_dimensions(Vec3::ZERO, Vec3::new(100.0, 100.0, 50.0)).expect("cube");
    // Coplanar cylinder sharing Z=0 and Z=50 faces
    let cyl = BrepSolid::cylinder(Vec3::new(50.0, 50.0, 0.0), 10.0, 50.0, Vec3::new(0.0, 0.0, 1.0)).expect("cyl");

    let err = cube.subtract(&cyl, SubtractOptions::default()).unwrap_err();
    assert!(
        matches!(err, BrepError::NonTransversalIntersection(_)),
        "coplanar subtraction must return NonTransversalIntersection, got {err:?}"
    );
}

#[test]
fn test_through_hole_boolean_subtraction_and_metrics() {
    let cube = BrepSolid::box_dimensions(Vec3::ZERO, Vec3::new(100.0, 100.0, 50.0)).expect("cube");
    // Tool oversizing: extend cylinder in Z from -5 to 55 (h = 60)
    let cyl = BrepSolid::cylinder(Vec3::new(50.0, 50.0, -5.0), 10.0, 60.0, Vec3::new(0.0, 0.0, 1.0)).expect("cyl");

    let punched = cube.subtract(&cyl, SubtractOptions::default()).expect("through-hole should succeed");
    assert_eq!(punched.boundaries_count(), 1);

    let bbox = punched.bounding_box();
    assert!((bbox.min.x - 0.0).abs() < 1e-3);
    assert!((bbox.max.x - 100.0).abs() < 1e-3);
    assert!((bbox.min.y - 0.0).abs() < 1e-3);
    assert!((bbox.max.y - 100.0).abs() < 1e-3);
    assert!((bbox.min.z - 0.0).abs() < 1e-3);
    assert!((bbox.max.z - 50.0).abs() < 1e-3);

    let expected_vol = 100.0 * 100.0 * 50.0 - std::f64::consts::PI * 100.0 * 50.0; // 484,292.0367 mm³
    let measured_vol = punched.volume().expect("volume");
    let err_rel = (measured_vol - expected_vol).abs() / expected_vol;
    assert!(err_rel < 0.005, "volume error relative {err_rel} should be < 0.5%");
}

#[test]
fn test_translation_of_boolean_result() {
    let cube = BrepSolid::box_dimensions(Vec3::ZERO, Vec3::new(100.0, 100.0, 50.0)).expect("cube");
    let cyl = BrepSolid::cylinder(Vec3::new(50.0, 50.0, -5.0), 10.0, 60.0, Vec3::new(0.0, 0.0, 1.0)).expect("cyl");
    let punched = cube.subtract(&cyl, SubtractOptions::default()).expect("punched");

    // Translation vector that previously triggered issues in buggy versions (issue #135)
    let delta = Vec3::new(25.0, -15.0, 10.0);
    let moved = punched.translate(delta).expect("translate");
    assert_eq!(moved.boundaries_count(), 1);

    let bbox = moved.bounding_box();
    assert!((bbox.min.x - 25.0).abs() < 1e-3);
    assert!((bbox.max.x - 125.0).abs() < 1e-3);
    assert!((bbox.min.y - (-15.0)).abs() < 1e-3);
    assert!((bbox.max.y - 85.0).abs() < 1e-3);
    assert!((bbox.min.z - 10.0).abs() < 1e-3);
    assert!((bbox.max.z - 60.0).abs() < 1e-3);
}

#[test]
fn test_deterministic_translations_suite() {
    let cube = BrepSolid::box_dimensions(Vec3::ZERO, Vec3::new(100.0, 100.0, 50.0)).expect("cube");
    let cyl = BrepSolid::cylinder(Vec3::new(50.0, 50.0, -5.0), 10.0, 60.0, Vec3::new(0.0, 0.0, 1.0)).expect("cyl");
    let punched = cube.subtract(&cyl, SubtractOptions::default()).expect("punched");

    // Sequence of various translations ensuring no lockups or boundary corruption
    let test_vectors = [
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(100.0, 0.0, 0.0),
        Vec3::new(0.0, -50.0, 0.0),
        Vec3::new(0.0, 0.0, 120.0),
        Vec3::new(-33.3, 44.4, -11.1),
    ];

    let mut current = punched;
    let mut total_offset = Vec3::ZERO;

    for v in test_vectors {
        current = current.translate(v).expect("deterministic translation");
        total_offset = total_offset + v;
        assert_eq!(current.boundaries_count(), 1);

        let bbox = current.bounding_box();
        assert!((bbox.min.x - total_offset.x).abs() < 1e-2);
        assert!((bbox.max.x - (total_offset.x + 100.0)).abs() < 1e-2);
    }
}

#[test]
fn test_triangulation_tolerance_control() {
    let cube = BrepSolid::box_dimensions(Vec3::ZERO, Vec3::new(100.0, 100.0, 50.0)).expect("cube");
    let cyl = BrepSolid::cylinder(Vec3::new(50.0, 50.0, -5.0), 10.0, 60.0, Vec3::new(0.0, 0.0, 1.0)).expect("cyl");
    let punched = cube.subtract(&cyl, SubtractOptions::default()).expect("punched");

    // Safe tolerances must succeed
    for &tol in &[0.5, 0.1, 0.05, 0.02, 0.01] {
        let mesh = punched.triangulate(tol).expect("triangulation should succeed");
        assert!(mesh.is_finite());
        assert!(!mesh.triangles.is_empty());
    }

    // Unsafe tolerance (< 0.01) must be rejected with NumericalConvergenceFailed without crashing
    let err = punched.triangulate(0.005).unwrap_err();
    assert!(
        matches!(err, BrepError::NumericalConvergenceFailed(_)),
        "unsafe fine tolerance should return NumericalConvergenceFailed, got {err:?}"
    );
}

#[test]
fn test_stl_export_and_topology_validation() {
    let cube = BrepSolid::box_dimensions(Vec3::ZERO, Vec3::new(100.0, 100.0, 50.0)).expect("cube");
    let cyl = BrepSolid::cylinder(Vec3::new(50.0, 50.0, -5.0), 10.0, 60.0, Vec3::new(0.0, 0.0, 1.0)).expect("cyl");
    let punched = cube.subtract(&cyl, SubtractOptions::default()).expect("punched");

    let mesh = punched.triangulate(0.5).expect("triangulate");
    let report = mesh.analyze_topology(0.01);

    assert!(report.all_finite, "all values must be finite");
    assert_eq!(report.open_edges, 0, "closed solid must have 0 open edges");
    assert_eq!(report.non_manifold_edges, 0, "manifold solid must have 0 non-manifold edges");
    assert_eq!(report.degenerate_faces, 0, "solid must have 0 degenerate faces");
    assert!(report.consistent_orientation, "normal orientation must be consistent and outward");

    let out_path = std::env::temp_dir().join("cadcraft_test_punched.stl");
    punched.export_stl_binary(&out_path, 0.5).expect("export stl");
    assert!(out_path.exists());
    let _ = std::fs::remove_file(out_path);
}

#[test]
fn test_step_export_no_duplicate_ids() {
    let cube = BrepSolid::box_dimensions(Vec3::ZERO, Vec3::new(100.0, 100.0, 50.0)).expect("cube");
    let cyl = BrepSolid::cylinder(Vec3::new(50.0, 50.0, -5.0), 10.0, 60.0, Vec3::new(0.0, 0.0, 1.0)).expect("cyl");
    let punched = cube.subtract(&cyl, SubtractOptions::default()).expect("punched");

    let out_path = std::env::temp_dir().join("cadcraft_test_punched.stp");
    punched.export_step(&out_path).expect("export step");
    assert!(out_path.exists());

    let content = std::fs::read_to_string(&out_path).expect("read step");
    assert!(content.contains("ISO-10303-21;"));
    assert!(content.contains("END-ISO-10303-21;"));

    // Verify commit e704a992: ensure NO duplicate entity IDs (#123=...) in STEP output
    let mut ids = HashSet::new();
    let mut duplicates = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') && trimmed.contains('=') {
            if let Some(id_str) = trimmed.split('=').next() {
                if !ids.insert(id_str.to_string()) {
                    duplicates.push(id_str.to_string());
                }
            }
        }
    }
    assert!(
        duplicates.is_empty(),
        "STEP file contains duplicate entity IDs: {:?}",
        duplicates
    );

    let _ = std::fs::remove_file(out_path);
}

#[test]
fn test_brep_snapshot_versioning() {
    let cube = BrepSolid::box_dimensions(Vec3::ZERO, Vec3::new(10.0, 10.0, 10.0)).expect("cube");
    let snapshot = cube.to_snapshot().expect("snapshot");

    assert_eq!(snapshot.format_version, BREP_FORMAT_VERSION);
    assert_eq!(snapshot.kernel_revision, KERNEL_REVISION);
    assert!(!snapshot.payload.is_empty());

    // Successful restore
    let restored = BrepSolid::from_snapshot(&snapshot).expect("restore");
    assert_eq!(restored.boundaries_count(), 1);

    // Incompatible kernel revision rejection
    let mut bad_kernel_snapshot = snapshot.clone();
    bad_kernel_snapshot.kernel_revision = "0000000000000000000000000000000000000000".to_string();
    let err = BrepSolid::from_snapshot(&bad_kernel_snapshot).unwrap_err();
    assert!(matches!(err, BrepError::UnsupportedKernelVersion { .. }));

    // Incompatible format version rejection
    let mut bad_version_snapshot = snapshot.clone();
    bad_version_snapshot.format_version = 999;
    let err = BrepSolid::from_snapshot(&bad_version_snapshot).unwrap_err();
    assert!(matches!(err, BrepError::UnsupportedKernelVersion { .. }));
}

#[test]
fn test_recipe_evaluation_reconstructs_solid() {
    let mut recipe = Recipe::new();
    recipe.add_feature(
        1,
        FeatureOp::Box {
            origin: Vec3::ZERO,
            size: Vec3::new(100.0, 100.0, 50.0),
        },
    );
    recipe.add_feature(
        2,
        FeatureOp::Cylinder {
            center: Vec3::new(50.0, 50.0, -5.0),
            radius: 10.0,
            height: 60.0,
            axis: Vec3::new(0.0, 0.0, 1.0),
        },
    );
    recipe.add_feature(
        3,
        FeatureOp::Boolean {
            kind: BooleanKind::Difference,
            target_id: 1,
            tool_id: 2,
            oversized: true,
        },
    );
    recipe.add_feature(
        4,
        FeatureOp::Translation {
            target_id: 3,
            delta: Vec3::new(10.0, 20.0, 30.0),
        },
    );

    let evaluated = recipe.evaluate().expect("recipe evaluation should succeed");
    assert_eq!(evaluated.boundaries_count(), 1);

    let bbox = evaluated.bounding_box();
    assert!((bbox.min.x - 10.0).abs() < 1e-2);
    assert!((bbox.max.x - 110.0).abs() < 1e-2);
    assert!((bbox.min.y - 20.0).abs() < 1e-2);
    assert!((bbox.max.y - 120.0).abs() < 1e-2);
    assert!((bbox.min.z - 30.0).abs() < 1e-2);
    assert!((bbox.max.z - 80.0).abs() < 1e-2);
}
