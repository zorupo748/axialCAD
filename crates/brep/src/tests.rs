//! Regression tests and geometric validation suite for CADCraft B-Rep.

use std::collections::HashSet;

use cadcraft_geom::Vec3;

use crate::error::BrepError;
use crate::mesh::TriangleMesh;
use crate::recipe::{BooleanKind, EvaluateRecipe, FeatureOp, Recipe};
use crate::snapshot::{BREP_FORMAT_VERSION, KERNEL_REVISION};
use crate::solid::{BrepSolid, SubtractOptions};

#[test]
fn test_box_creation() {
    let b = BrepSolid::box_dimensions(Vec3::new(10.0, 20.0, 30.0), Vec3::new(100.0, 50.0, 25.0)).expect("box should succeed");
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
    let cyl = BrepSolid::cylinder(Vec3::new(0.0, 0.0, 0.0), 10.0, 50.0, Vec3::new(0.0, 0.0, 1.0)).expect("cylinder should succeed");
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
    let profile = vec![Vec3::new(10.0, 10.0, 0.0), Vec3::new(40.0, 10.0, 0.0), Vec3::new(40.0, 30.0, 0.0), Vec3::new(10.0, 30.0, 0.0)];
    let prism = BrepSolid::extrude(&profile, Vec3::new(0.0, 0.0, 25.0)).expect("extrusion should succeed");
    let vol = prism.volume().expect("prism volume");
    let expected_vol = 30.0 * 20.0 * 25.0; // 15,000.0
    assert!((vol - expected_vol).abs() / expected_vol < 0.01);
}

#[test]
fn test_extrusion_negative_direction_and_topology() {
    let profile = vec![Vec3::new(10.0, 10.0, 0.0), Vec3::new(40.0, 10.0, 0.0), Vec3::new(40.0, 30.0, 0.0), Vec3::new(10.0, 30.0, 0.0)];
    let prism = BrepSolid::extrude(&profile, Vec3::new(0.0, 0.0, -25.0)).expect("negative extrusion should succeed");
    let vol = prism.volume().expect("prism volume");
    let expected_vol = 30.0 * 20.0 * 25.0;
    assert!((vol - expected_vol).abs() / expected_vol < 0.01);

    let bbox = prism.bounding_box();
    assert!((bbox.min.z - (-25.0)).abs() < 1e-3);
    assert!((bbox.max.z - 0.0).abs() < 1e-3);

    let mesh = prism.triangulate(0.05).expect("triangulate");
    let topo = mesh.analyze_topology(1e-4);
    assert_eq!(topo.open_edges, 0, "must be a closed manifold solid");
    assert_eq!(topo.non_manifold_edges, 0, "must have zero non-manifold edges");
    assert_eq!(topo.degenerate_faces, 0, "must have zero degenerate faces");
    assert!(topo.consistent_orientation, "normal orientation must be consistent");
    assert!(topo.signed_volume > 0.0, "outward normals must give positive signed volume, got {}", topo.signed_volume);
}

#[test]
fn test_cw_and_inclined_extrusion_outward_normals_and_through_hole() {
    // Clockwise base polygon extruded in +Z and -Z must still produce outward normals (+volume)
    let cw = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 30.0, 0.0), Vec3::new(40.0, 30.0, 0.0), Vec3::new(40.0, 0.0, 0.0)];
    let solid_pos = BrepSolid::extrude(&cw, Vec3::new(0.0, 0.0, 5.0)).expect("cw +Z extrude");
    let rep_pos = solid_pos.triangulate(0.05).expect("triangulate").validate_for_stl(1e-5).expect("valid STL mesh");
    assert!((rep_pos.signed_volume - 6000.0).abs() < 1e-2);

    let solid_neg = BrepSolid::extrude(&cw, Vec3::new(0.0, 0.0, -5.0)).expect("cw -Z extrude");
    let rep_neg = solid_neg.triangulate(0.05).expect("triangulate").validate_for_stl(1e-5).expect("valid STL mesh");
    assert!((rep_neg.signed_volume - 6000.0).abs() < 1e-2);

    // Inclined profile (tilted 30 degrees around X) extruded along its local normal
    let angle = std::f64::consts::FRAC_PI_6;
    let rot_x = |x: f64, y: f64| Vec3::new(x, y * angle.cos(), y * angle.sin());
    let inclined = vec![rot_x(0.0, 0.0), rot_x(40.0, 0.0), rot_x(40.0, 30.0), rot_x(0.0, 30.0)];
    let normal = Vec3::new(0.0, -angle.sin(), angle.cos()).normalized() * 5.0;
    let solid_inc = BrepSolid::extrude(&inclined, normal).expect("inclined extrude");
    let rep_inc = solid_inc.triangulate(0.05).expect("triangulate").validate_for_stl(1e-5).expect("inclined STL valid");
    assert!((rep_inc.signed_volume - 6000.0).abs() < 1e-2);

    // Standalone cylinder at tol=0.05
    let cyl_only = BrepSolid::cylinder(Vec3::new(20.0, 15.0, 0.0), 4.0, 5.0, Vec3::Z).expect("cyl only");
    let rep_cyl = cyl_only.triangulate(0.05).expect("triangulate cyl").validate_for_stl(1e-5).expect("standalone cylinder STL valid");
    assert!((rep_cyl.signed_volume - std::f64::consts::PI * 16.0 * 5.0).abs() < 2.0);

    // Extruded plate 40x30x5 with through-hole diameter 8mm (radius 4mm) at center (20, 15)
    let hole_cyl = BrepSolid::cylinder(Vec3::new(20.0, 15.0, -1.0), 4.0, 7.0, Vec3::Z).expect("hole cyl");
    let plate_hole = solid_pos.subtract(&hole_cyl, SubtractOptions::default()).expect("subtract hole");
    let rep_coarse = plate_hole.triangulate(0.25).expect("triangulate").validate_for_stl(1e-5).expect("valid STL at tol=0.25");
    let expected_vol = 40.0 * 30.0 * 5.0 - std::f64::consts::PI * 16.0 * 5.0;
    assert!((rep_coarse.signed_volume - expected_vol).abs() < 20.0, "got {}", rep_coarse.signed_volume);

    // Fine meshing at tol=0.05 now succeeds and produces a valid watertight STL mesh!
    let rep_fine = plate_hole.triangulate(0.05).expect("triangulate at 0.05").validate_for_stl(1e-5).expect("valid STL at tol=0.05");
    assert!((rep_fine.signed_volume - expected_vol).abs() < 10.0, "got {}", rep_fine.signed_volume);
}

#[test]
fn test_extrusion_validation_and_rejections() {
    // 1. Self-intersecting bowtie
    let bowtie_profile = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(30.0, 20.0, 0.0), Vec3::new(0.0, 20.0, 0.0), Vec3::new(20.0, 0.0, 0.0)];
    let res = BrepSolid::extrude(&bowtie_profile, Vec3::new(0.0, 0.0, 25.0));
    assert!(res.is_err(), "bowtie profile must be rejected");
    let err = res.unwrap_err();
    assert!(matches!(err, BrepError::InvalidGeometry(_)), "expected InvalidGeometry, got {err:?}");
    let err_str = err.to_string();
    assert!(err_str.contains("self-intersects"), "must report self-intersection: {err_str}");

    // 2. Extrusion direction coplanar with profile plane
    let valid_profile = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(30.0, 0.0, 0.0), Vec3::new(30.0, 20.0, 0.0), Vec3::new(0.0, 20.0, 0.0)];
    let coplanar_res = BrepSolid::extrude(&valid_profile, Vec3::new(10.0, 0.0, 0.0));
    assert!(coplanar_res.is_err(), "coplanar direction must be rejected");
    let err = coplanar_res.unwrap_err();
    assert!(matches!(err, BrepError::InvalidGeometry(_)));
    assert!(err.to_string().contains("coplanar"), "must mention coplanar: {err}");

    // 3. Degenerate collinear vertices (zero area)
    let collinear_profile = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 0.0, 0.0), Vec3::new(20.0, 0.0, 0.0)];
    let col_res = BrepSolid::extrude(&collinear_profile, Vec3::new(0.0, 0.0, 10.0));
    assert!(col_res.is_err(), "collinear zero-area profile must be rejected");

    // 4. Non-planar 3D profile
    let nonplanar_profile = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(10.0, 0.0, 0.0),
        Vec3::new(10.0, 10.0, 0.0),
        Vec3::new(0.0, 10.0, 5.0), // z deviates significantly
    ];
    let np_res = BrepSolid::extrude(&nonplanar_profile, Vec3::new(0.0, 0.0, 10.0));
    assert!(np_res.is_err(), "non-planar profile must be rejected");
    assert!(np_res.unwrap_err().to_string().contains("coplanar"));
}

#[test]
fn test_disjoint_intersection() {
    let box_a = BrepSolid::box_dimensions(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 10.0, 10.0)).expect("box a");
    let box_b = BrepSolid::box_dimensions(Vec3::new(100.0, 100.0, 100.0), Vec3::new(10.0, 10.0, 10.0)).expect("box b");
    let solid = box_a.intersect(&box_b).expect("disjoint intersection evaluation");
    assert!(solid.is_empty(), "disjoint intersection must be empty solid");
    assert_eq!(solid.boundaries_count(), 0);
}

#[test]
fn test_boolean_coplanar_rejection() {
    let cube = BrepSolid::box_dimensions(Vec3::ZERO, Vec3::new(100.0, 100.0, 50.0)).expect("cube");
    // Coplanar cylinder sharing Z=0 and Z=50 faces
    let cyl = BrepSolid::cylinder(Vec3::new(50.0, 50.0, 0.0), 10.0, 50.0, Vec3::new(0.0, 0.0, 1.0)).expect("cyl");

    let err = cube.subtract(&cyl, SubtractOptions::default()).unwrap_err();
    assert!(matches!(err, BrepError::NonTransversalIntersection(_)), "coplanar subtraction must return NonTransversalIntersection, got {err:?}");
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
    let test_vectors =
        [Vec3::new(0.0, 0.0, 0.0), Vec3::new(100.0, 0.0, 0.0), Vec3::new(0.0, -50.0, 0.0), Vec3::new(0.0, 0.0, 120.0), Vec3::new(-33.3, 44.4, -11.1)];

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
    assert!(matches!(err, BrepError::NumericalConvergenceFailed(_)), "unsafe fine tolerance should return NumericalConvergenceFailed, got {err:?}");
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
        if trimmed.starts_with('#')
            && trimmed.contains('=')
            && let Some(id_str) = trimmed.split('=').next()
            && !ids.insert(id_str.to_string())
        {
            duplicates.push(id_str.to_string());
        }
    }
    assert!(duplicates.is_empty(), "STEP file contains duplicate entity IDs: {:?}", duplicates);

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
    recipe.add_feature(1, FeatureOp::Box { origin: Vec3::ZERO, size: Vec3::new(100.0, 100.0, 50.0) });
    recipe.add_feature(2, FeatureOp::Cylinder { center: Vec3::new(50.0, 50.0, -5.0), radius: 10.0, height: 60.0, axis: Vec3::new(0.0, 0.0, 1.0) });
    recipe.add_feature(3, FeatureOp::Boolean { kind: BooleanKind::Difference, target_id: 1, tool_id: 2, oversized: true });
    recipe.add_feature(4, FeatureOp::Translation { target_id: 3, delta: Vec3::new(10.0, 20.0, 30.0) });

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

#[test]
fn test_binary_stl_header_and_layout_invariants() {
    // Deterministic in-memory mesh fixture with fractional f64 values to check f32 conversion,
    // explicit normals, signed vertex positions, and 2-triangle serialization.
    let mesh = TriangleMesh {
        positions: vec![
            Vec3::new(-1.25, 2.5, 3.123456789),
            Vec3::new(40.0, 0.0, -0.5),
            Vec3::new(40.0, 30.0, 5.0),
            Vec3::new(0.0, 30.0, 5.0),
        ],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        normals: vec![Vec3::new(0.0, -0.5, 0.8660254037844386), Vec3::new(0.125, 0.25, 0.96024)],
    };

    // Independent binary STL reference encoding (never calls TriangleMesh::write_stl_binary).
    let mut expected_header = [0u8; 80];
    let title = b"axialCAD binary STL";
    expected_header[..title.len()].copy_from_slice(title);

    let mut expected_body = Vec::new();
    expected_body.extend_from_slice(&(mesh.triangles.len() as u32).to_le_bytes());
    for (idx, tri) in mesh.triangles.iter().enumerate() {
        let n = mesh.normals[idx];
        let p0 = mesh.positions[tri[0]];
        let p1 = mesh.positions[tri[1]];
        let p2 = mesh.positions[tri[2]];
        for v in [n, p0, p1, p2] {
            expected_body.extend_from_slice(&(v.x as f32).to_le_bytes());
            expected_body.extend_from_slice(&(v.y as f32).to_le_bytes());
            expected_body.extend_from_slice(&(v.z as f32).to_le_bytes());
        }
        expected_body.extend_from_slice(&0u16.to_le_bytes());
    }

    let actual = mesh.to_stl_binary_bytes().expect("serialize deterministic fixture");

    // A. Exact 80-byte header with zero padding.
    assert_eq!(&actual[..80], &expected_header[..]);
    assert!(actual[title.len()..80].iter().all(|&b| b == 0));

    // B. u32 little-endian triangle count.
    let count_le = u32::from_le_bytes([actual[80], actual[81], actual[82], actual[83]]) as usize;
    assert_eq!(count_le, mesh.triangles.len());

    // C. Total byte size is 84 + 50 * triangles.
    assert_eq!(actual.len(), 84 + 50 * mesh.triangles.len());

    // D & E. Normals, vertices (after f32 conversion), attributes (=0), and body bytes from offset 80
    // match the independent binary reference byte-for-byte.
    assert_eq!(&actual[80..], &expected_body[..]);

    // Also verify plate 40x30x5 mm B-Rep serialization layout (12 triangles, 684 bytes).
    let rect = vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(40.0, 0.0, 0.0), Vec3::new(40.0, 30.0, 0.0), Vec3::new(0.0, 30.0, 0.0)];
    let plate = BrepSolid::extrude(&rect, Vec3::new(0.0, 0.0, 5.0)).expect("extrude plate");
    let plate_mesh = plate.triangulate(0.05).expect("triangulate plate");
    plate_mesh.validate_for_stl(1e-5).expect("plate mesh valid for STL");
    let plate_bytes = plate_mesh.to_stl_binary_bytes().expect("serialize plate");
    assert_eq!(&plate_bytes[..80], &expected_header[..]);
    assert_eq!(u32::from_le_bytes([plate_bytes[80], plate_bytes[81], plate_bytes[82], plate_bytes[83]]), 12);
    assert_eq!(plate_bytes.len(), 684);
}

#[test]
fn test_validation_suite_cases_a_through_g() {
    let pi = std::f64::consts::PI;

    // Case A: Standalone cylinder at 0.05 mm
    let cyl_a = BrepSolid::cylinder(Vec3::new(20.0, 15.0, 0.0), 4.0, 5.0, Vec3::Z).expect("case A cylinder");
    let mesh_a = cyl_a.triangulate(0.05).expect("case A triangulate");
    let rep_a = mesh_a.validate_for_stl(1e-5).expect("case A STL valid");
    assert_eq!(rep_a.open_edges, 0);
    assert_eq!(rep_a.non_manifold_edges, 0);
    assert_eq!(rep_a.degenerate_faces, 0);
    assert!(rep_a.consistent_orientation);
    let vol_a_expected = pi * 16.0 * 5.0; // ~251.327
    assert!((rep_a.signed_volume - vol_a_expected).abs() < 2.0);

    // Case B: Solid extruded plate 40x30x5 at 0.05 mm
    let rect_b = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(40.0, 0.0, 0.0),
        Vec3::new(40.0, 30.0, 0.0),
        Vec3::new(0.0, 30.0, 0.0),
    ];
    let plate_b = BrepSolid::extrude(&rect_b, Vec3::new(0.0, 0.0, 5.0)).expect("case B extrude");
    let mesh_b = plate_b.triangulate(0.05).expect("case B triangulate");
    let rep_b = mesh_b.validate_for_stl(1e-5).expect("case B STL valid");
    assert_eq!(rep_b.open_edges, 0);
    assert_eq!(rep_b.non_manifold_edges, 0);
    assert_eq!(rep_b.degenerate_faces, 0);
    assert!(rep_b.consistent_orientation);
    assert!((rep_b.signed_volume - 6000.0).abs() < 1e-2);

    // Case C: Holed plate 40x30x5 with hole R=4 at tol=0.25 mm
    let hole_tool = BrepSolid::cylinder(Vec3::new(20.0, 15.0, -1.0), 4.0, 7.0, Vec3::Z).expect("hole tool");
    let plate_c = plate_b.subtract(&hole_tool, SubtractOptions { tolerance: 0.25 }).expect("case C subtract");
    let mesh_c = plate_c.triangulate(0.25).expect("case C triangulate");
    let rep_c = mesh_c.validate_for_stl(1e-5).expect("case C STL valid");
    assert_eq!(rep_c.open_edges, 0);
    assert_eq!(rep_c.non_manifold_edges, 0);
    assert_eq!(rep_c.degenerate_faces, 0);
    assert!(rep_c.consistent_orientation);
    let vol_holed_expected = 6000.0 - vol_a_expected; // ~5748.67
    assert!((rep_c.signed_volume - vol_holed_expected).abs() < 20.0);

    // Case D: Holed plate 40x30x5 with hole R=4 at fine tol=0.05 mm (Target case)
    let plate_d = plate_b.subtract(&hole_tool, SubtractOptions { tolerance: 0.05 }).expect("case D subtract");
    let mesh_d = plate_d.triangulate(0.05).expect("case D triangulate");
    let rep_d = mesh_d.validate_for_stl(1e-5).expect("case D STL valid");
    assert_eq!(rep_d.open_edges, 0, "must have zero open edges at 0.05 mm");
    assert_eq!(rep_d.non_manifold_edges, 0, "must have zero non-manifold edges at 0.05 mm");
    assert_eq!(rep_d.degenerate_faces, 0, "must have zero degenerate faces");
    assert!(rep_d.consistent_orientation, "must have consistent outward orientation");
    assert!((rep_d.signed_volume - vol_holed_expected).abs() < 10.0, "signed volume must match analytical");

    // Check maximum radial deviation on hole (<= 0.05 mm)
    let mut max_rad_dev = 0.0f64;
    let mut hole_vertex_count = 0;
    for pos in &mesh_d.positions {
        if pos.z >= -1e-4 && pos.z <= 5.0 + 1e-4 {
            let r = ((pos.x - 20.0).powi(2) + (pos.y - 15.0).powi(2)).sqrt();
            if (r - 4.0).abs() < 0.2 {
                let dev = (r - 4.0).abs();
                if dev > max_rad_dev {
                    max_rad_dev = dev;
                }
                hole_vertex_count += 1;
            }
        }
    }
    assert!(hole_vertex_count >= 16, "must have vertices around hole, got {hole_vertex_count}");
    assert!(max_rad_dev <= 0.05, "max radial deviation {max_rad_dev} exceeds tolerance 0.05 mm");

    // Validate the binary STL export in memory (no files outside the repo).
    let stl_bytes = mesh_d.to_stl_binary_bytes().expect("serialize STL");
    assert!(stl_bytes.len() >= 84, "binary STL must contain a header and triangle count");
    let stl_triangles = u32::from_le_bytes([stl_bytes[80], stl_bytes[81], stl_bytes[82], stl_bytes[83]]) as usize;
    assert_eq!(stl_triangles, mesh_d.triangles.len(), "binary STL triangle count must match the mesh");
    assert_eq!(stl_bytes.len(), 84 + 50 * stl_triangles, "binary STL length must match its triangle count");

    // Case E: Off-center hole at (15, 10), radius 3, height 7 at tol=0.05 mm
    let off_tool = BrepSolid::cylinder(Vec3::new(15.0, 10.0, -1.0), 3.0, 7.0, Vec3::Z).expect("off-center tool");
    let plate_e = plate_b.subtract(&off_tool, SubtractOptions { tolerance: 0.05 }).expect("case E subtract");
    let mesh_e = plate_e.triangulate(0.05).expect("case E triangulate");
    let rep_e = mesh_e.validate_for_stl(1e-5).expect("case E STL valid");
    assert_eq!(rep_e.open_edges, 0);
    assert_eq!(rep_e.non_manifold_edges, 0);
    assert_eq!(rep_e.degenerate_faces, 0);
    assert!(rep_e.consistent_orientation);
    let vol_e_expected = 6000.0 - pi * 9.0 * 5.0; // ~5858.63
    assert!((rep_e.signed_volume - vol_e_expected).abs() < 10.0);

    // Case F: Square plate 50x50x10 with two through holes (R=4 at (15, 25), R=5 at (35, 25)) at tol=0.05 mm
    let square_profile = vec![
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(50.0, 0.0, 0.0),
        Vec3::new(50.0, 50.0, 0.0),
        Vec3::new(0.0, 50.0, 0.0),
    ];
    let plate_f_solid = BrepSolid::extrude(&square_profile, Vec3::new(0.0, 0.0, 10.0)).expect("case F extrude");
    let tool_f1 = BrepSolid::cylinder(Vec3::new(15.0, 25.0, -1.0), 4.0, 12.0, Vec3::Z).expect("tool F1");
    let tool_f2 = BrepSolid::cylinder(Vec3::new(35.0, 25.0, -1.0), 5.0, 12.0, Vec3::Z).expect("tool F2");
    let plate_f = plate_f_solid
        .subtract(&tool_f1, SubtractOptions { tolerance: 0.05 })
        .expect("case F subtract 1")
        .subtract(&tool_f2, SubtractOptions { tolerance: 0.05 })
        .expect("case F subtract 2");
    let mesh_f = plate_f.triangulate(0.05).expect("case F triangulate");
    let rep_f = mesh_f.validate_for_stl(1e-5).expect("case F STL valid");
    assert_eq!(rep_f.open_edges, 0);
    assert_eq!(rep_f.non_manifold_edges, 0);
    assert_eq!(rep_f.degenerate_faces, 0);
    assert!(rep_f.consistent_orientation);
    let vol_f_expected = 50.0 * 50.0 * 10.0 - (pi * 16.0 * 10.0) - (pi * 25.0 * 10.0); // 25000 - 502.65 - 785.40 = ~23711.95
    assert!((rep_f.signed_volume - vol_f_expected).abs() < 30.0);

    // Case G: Inclined prism and inclined through cylinder (non-canonical axis alignment)
    let angle = std::f64::consts::FRAC_PI_4; // 45 deg tilt
    let rot = |v: Vec3| Vec3::new(v.x, v.y * angle.cos() - v.z * angle.sin(), v.y * angle.sin() + v.z * angle.cos());
    let inc_profile = vec![rot(Vec3::new(0.0, 0.0, 0.0)), rot(Vec3::new(40.0, 0.0, 0.0)), rot(Vec3::new(40.0, 30.0, 0.0)), rot(Vec3::new(0.0, 30.0, 0.0))];
    let inc_dir = rot(Vec3::new(0.0, 0.0, 5.0));
    let inc_solid = BrepSolid::extrude(&inc_profile, inc_dir).expect("inc extrude");
    let inc_cyl_origin = rot(Vec3::new(20.0, 15.0, -1.0));
    let inc_cyl_axis = rot(Vec3::Z).normalized();
    let inc_tool = BrepSolid::cylinder(inc_cyl_origin, 4.0, 7.0, inc_cyl_axis).expect("inc cylinder");
    let inc_holed = inc_solid.subtract(&inc_tool, SubtractOptions { tolerance: 0.05 }).expect("inc subtract");
    let mesh_g = inc_holed.triangulate(0.05).expect("inc triangulate");
    let rep_g = mesh_g.validate_for_stl(1e-5).expect("inc STL valid");
    assert_eq!(rep_g.open_edges, 0);
    assert_eq!(rep_g.non_manifold_edges, 0);
    assert_eq!(rep_g.degenerate_faces, 0);
    assert!(rep_g.consistent_orientation);
    assert!((rep_g.signed_volume - vol_holed_expected).abs() < 15.0);
}
