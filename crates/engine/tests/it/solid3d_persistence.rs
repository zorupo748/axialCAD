//! Persistence and interoperability audit for 3D solids (Milestones 2B.1 & 2B.2).
//!
//! Verifies:
//! 1. Export as standard 3DFACE geometry without private CADCRAFT_SOLID3D entity in ENTITIES.
//! 2. Preservation of parametric recipe in CADCRAFT XDATA (chunked <= 255 chars).
//! 3. Zero duplicated geometry on DXF read (1 Solid3d restored, no loose 3DFACEs).
//! 4. 1 op (Box), 2 ops (Box+Translate), 10 ops, and 50 ops scaling.
//! 5. Lifecycle test generating box and cylinder via engine commands and verifying standard DXF.

use cadcraft_brep::EvaluateRecipe;
use cadcraft_doc::solid3d::{FeatureNode, FeatureOp, Recipe, SOLID3D_SCHEMA_VERSION, Solid3d, Solid3dStatus};
use cadcraft_doc::{Common, Drawing, Entity, EntityKind};
use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use cadcraft_geom::Vec3;
use serde_json::json;
use std::fs;

fn init_io_hooks() {
    set_io(IoHooks {
        read: |bytes, name| cadcraft_io::read(bytes, name).map_err(|e| e.to_string()),
        write: |doc, name| cadcraft_io::write(doc, name).map_err(|e| e.to_string()),
        plot: None,
    });
}

#[test]
fn test_recipe_length_and_dxf_roundtrip_scaling() {
    init_io_hooks();

    // Case 1: 1 operation (Box)
    let mut recipe_1 = Recipe::new();
    recipe_1.features.push(FeatureNode { id: 1, op: FeatureOp::Box { origin: Vec3::new(0.0, 0.0, 0.0), size: Vec3::new(100.0, 50.0, 25.0) } });

    // Case 2: 2 operations (Box + Translation)
    let mut recipe_2 = recipe_1.clone();
    recipe_2.features.push(FeatureNode { id: 2, op: FeatureOp::Translation { target_id: 1, delta: Vec3::new(15.0, -10.0, 20.0) } });

    // Case 3: 10 operations (Box + 9 sequential translations)
    let mut recipe_10 = recipe_1.clone();
    for i in 2..=10 {
        recipe_10.features.push(FeatureNode { id: i, op: FeatureOp::Translation { target_id: i - 1, delta: Vec3::new(2.0, 1.0, 0.5) } });
    }

    // Case 4: 50 operations (Box + 49 sequential operations)
    let mut recipe_50 = recipe_1.clone();
    for i in 2..=50 {
        recipe_50.features.push(FeatureNode { id: i, op: FeatureOp::Translation { target_id: i - 1, delta: Vec3::new(1.0, 0.5, 0.25) } });
    }

    let cases = vec![
        ("1 operation (Box)", recipe_1),
        ("2 operations (Box + Translation)", recipe_2),
        ("10 operations", recipe_10),
        ("50 operations", recipe_50),
    ];

    for (name, recipe) in cases {
        println!("=== Auditing {name} ===");
        let json_compact = serde_json::to_string(&recipe).expect("serialize compact json");
        let json_len = json_compact.len();
        println!("  JSON serialized length: {json_len} bytes");

        // Verify recipe evaluates successfully in kernel
        let solid_eval = recipe.evaluate().expect("kernel evaluation must succeed");
        let bbox = solid_eval.bounding_box();
        assert!(!bbox.is_empty(), "bounding box must not be empty");

        // Create a drawing with this Solid3d
        let mut doc = Drawing::new_metric();
        let handle = doc.new_handle();
        let entity = Entity {
            handle,
            common: Common::default(),
            kind: EntityKind::Solid3d(Solid3d {
                schema_version: SOLID3D_SCHEMA_VERSION,
                recipe: recipe.clone(),
                bbox,
                status: Solid3dStatus::Evaluated,
                faces: Vec::new(),
            }),
        };
        doc.model.push(entity);

        // Write to DXF bytes
        let dxf_bytes = cadcraft_io::write(&doc, "test.dxf").expect("dxf export must succeed");
        let dxf_str = String::from_utf8_lossy(&dxf_bytes);
        println!("  DXF file total size: {} bytes", dxf_bytes.len());

        // Verify that:
        // 1. NO private CADCRAFT_SOLID3D entity is written in ENTITIES
        // 2. Standard 3DFACE entities ARE written
        // 3. No single line in DXF exceeds 255 bytes
        let mut found_cadcraft_solid = false;
        let mut face_count = 0;
        let mut max_line_len = 0;
        for line in dxf_str.lines() {
            let len = line.len();
            if len > max_line_len {
                max_line_len = len;
            }
            if line.trim() == "CADCRAFT_SOLID3D" {
                found_cadcraft_solid = true;
            }
            if line.trim() == "3DFACE" {
                face_count += 1;
            }
        }
        println!("  DXF 3DFACE count: {face_count}, max line length: {max_line_len} bytes");
        assert!(!found_cadcraft_solid, "Interoperable DXF must NOT write private CADCRAFT_SOLID3D entity");
        assert!(face_count > 0, "Interoperable DXF must contain standard 3DFACE entities");
        assert!(max_line_len <= 256, "No line in DXF may exceed 256 bytes");

        // Read back from DXF
        let read_doc = cadcraft_io::read(&dxf_bytes, "test.dxf").expect("dxf read must succeed");
        assert_eq!(read_doc.model.len(), 1, "must restore exactly 1 entity without duplicate 3DFACEs");

        let first_entity = read_doc.model.iter().next().expect("first entity");
        match &first_entity.kind {
            EntityKind::Solid3d(restored_solid) => {
                assert_eq!(restored_solid.schema_version, SOLID3D_SCHEMA_VERSION);
                assert_eq!(restored_solid.recipe.features.len(), recipe.features.len());
                assert_eq!(restored_solid.recipe, recipe, "recipe must roundtrip identically");

                // Evaluate restored solid from scratch
                let restored_eval = restored_solid.recipe.evaluate().expect("restored recipe must evaluate");
                let restored_bbox = restored_eval.bounding_box();
                assert!((restored_bbox.min.x - bbox.min.x).abs() < 1e-4);
                assert!((restored_bbox.max.x - bbox.max.x).abs() < 1e-4);
                assert!((restored_bbox.min.y - bbox.min.y).abs() < 1e-4);
                assert!((restored_bbox.max.y - bbox.max.y).abs() < 1e-4);
                assert!((restored_bbox.min.z - bbox.min.z).abs() < 1e-4);
                assert!((restored_bbox.max.z - bbox.max.z).abs() < 1e-4);
            }
            other => panic!("Expected EntityKind::Solid3d, got {:?}", other),
        }
    }
}

#[test]
fn test_box_cylinder_interoperable_dxf_export() {
    init_io_hooks();

    let temp_dir = std::env::temp_dir().join(format!("cadcraft-interop-{}", std::process::id()));
    let _ = fs::create_dir_all(&temp_dir);
    let dxf_path = temp_dir.join("interop_model.dxf");

    let mut session = Session::new();

    // 1. Create a box of dimensions 100 x 50 x 25
    let box_res = session
        .execute(
            "solid.box",
            &json!({
                "origin": [0.0, 0.0, 0.0],
                "size": [100.0, 50.0, 25.0]
            }),
        )
        .expect("execute solid.box");
    assert_eq!(box_res["status"], "ok");

    // 2. Create a cylinder of radius 15, height 60 at (200, 0, 0)
    let cyl_res = session
        .execute(
            "solid.cylinder",
            &json!({
                "center": [200.0, 0.0, 0.0],
                "radius": 15.0,
                "height": 60.0,
                "axis": [0.0, 0.0, 1.0]
            }),
        )
        .expect("execute solid.cylinder");
    assert_eq!(cyl_res["status"], "ok");

    // 3. Export to DXF
    session.execute("saveas", &json!({ "path": dxf_path.to_string_lossy() })).expect("execute saveas");
    assert!(dxf_path.exists(), "dxf file must exist on disk");

    // 4. Verify the raw DXF file content
    let dxf_content = fs::read_to_string(&dxf_path).expect("read dxf file");
    assert!(!dxf_content.contains("CADCRAFT_SOLID3D"), "Must not contain private CADCRAFT_SOLID3D entity");
    assert!(dxf_content.contains("3DFACE"), "Must contain standard 3DFACE entities for external CAD viewing");

    // 5. Read back via CADCraft DXF reader
    session.execute("close", &json!({})).expect("execute close");
    session.execute("open", &json!({ "path": dxf_path.to_string_lossy() })).expect("execute open");

    // 6. Verify that exactly 2 solid entities were restored (no loose duplicate faces in model space)
    let doc = session.doc().expect("session doc");
    assert_eq!(doc.model.len(), 2, "must restore exactly 2 Solid3d entities, no duplicated faces");

    let solids: Vec<&Solid3d> = doc.model.iter().filter_map(|e| if let EntityKind::Solid3d(s) = &e.kind { Some(s) } else { None }).collect();

    assert_eq!(solids.len(), 2, "both solids must be restored as Solid3d");

    // Verify box solid
    let box_solid = solids.iter().find(|s| s.recipe.features.iter().any(|f| matches!(f.op, FeatureOp::Box { .. }))).expect("box solid");
    assert!((box_solid.bbox.min.x - 0.0).abs() < 1e-3);
    assert!((box_solid.bbox.max.x - 100.0).abs() < 1e-3);
    assert!((box_solid.bbox.min.y - 0.0).abs() < 1e-3);
    assert!((box_solid.bbox.max.y - 50.0).abs() < 1e-3);
    assert!((box_solid.bbox.min.z - 0.0).abs() < 1e-3);
    assert!((box_solid.bbox.max.z - 25.0).abs() < 1e-3);

    // Verify cylinder solid
    let cyl_solid = solids.iter().find(|s| s.recipe.features.iter().any(|f| matches!(f.op, FeatureOp::Cylinder { .. }))).expect("cyl solid");
    assert!((cyl_solid.bbox.min.x - 185.0).abs() < 1.0);
    assert!((cyl_solid.bbox.max.x - 215.0).abs() < 1.0);
    assert!((cyl_solid.bbox.min.z - 0.0).abs() < 1e-3);
    assert!((cyl_solid.bbox.max.z - 60.0).abs() < 1e-3);

    println!("=== Interoperable DXF Export Verified ===");
    println!("File saved at: {}", dxf_path.display());
    println!("Total 3DFACE count in DXF: {}", dxf_content.matches("3DFACE").count());
}
