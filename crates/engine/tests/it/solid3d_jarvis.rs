//! End-to-end JARVIS structured command integration test for 3D solids (Milestone 2B).
//!
//! Executes the full lifecycle strictly via `Session::execute` with JSON params:
//! 1. Create a box of known dimensions (`solid.box`).
//! 2. Query its handle and bounding box (`query.solid`).
//! 3. Translate it (`solid.translate`).
//! 4. Query its new position (`query.solid` and `query.metrics`).
//! 5. Save the drawing to DXF (`saveas`), close it (`close`), and reopen it (`open`).
//! 6. Verify that the solid entity, its dimensions, and recipe are preserved.
//! 7. Export it to STL (`export.stl`) and verify the STL file exists and contains a valid mesh.

use std::fs;
use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use serde_json::json;

#[test]
fn test_solid3d_jarvis_e2e_lifecycle() {
    // 0. Set up real DXF file I/O hooks
    set_io(IoHooks {
        read: |bytes, name| cadcraft_io::read(bytes, name).map_err(|e| e.to_string()),
        write: |doc, name| cadcraft_io::write(doc, name).map_err(|e| e.to_string()),
        plot: None,
    });

    let temp_dir = std::env::temp_dir().join(format!("cadcraft-jarvis-3d-{}", std::process::id()));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let dxf_path = temp_dir.join("jarvis_model.dxf");
    let stl_path = temp_dir.join("jarvis_model.stl");

    let mut session = Session::new();

    // 1. Create box of dimensions 100 x 50 x 25 at origin (10, 20, 30)
    let box_res = session.execute(
        "solid.box",
        &json!({
            "origin": [10.0, 20.0, 30.0],
            "size": [100.0, 50.0, 25.0]
        }),
    ).expect("execute solid.box");

    assert_eq!(box_res["status"], "ok");
    let handle = box_res["handle"].as_str().expect("handle string").to_string();
    assert_eq!(box_res["bbox"]["min"], json!([10.0, 20.0, 30.0]));
    assert_eq!(box_res["bbox"]["max"], json!([110.0, 70.0, 55.0]));

    // 2. Query handle and bounding box
    let q1 = session.execute(
        "query.solid",
        &json!({ "handle": handle.clone() }),
    ).expect("execute query.solid");

    assert_eq!(q1["status"], "ok");
    assert_eq!(q1["handle"], handle);
    assert_eq!(q1["type"], "Solid3d");
    assert_eq!(q1["evaluated"], true);
    assert_eq!(q1["bbox"]["min"], json!([10.0, 20.0, 30.0]));
    assert_eq!(q1["bbox"]["max"], json!([110.0, 70.0, 55.0]));
    assert_eq!(q1["features_count"], 1);

    // Also query metrics
    let m1 = session.execute(
        "query.metrics",
        &json!({ "handle": handle.clone() }),
    ).expect("execute query.metrics");
    assert_eq!(m1["status"], "ok");
    let volume = m1["volume"].as_f64().expect("volume f64");
    assert!((volume - 125000.0).abs() < 1e-2, "volume expected 125000, got {volume}");
    assert_eq!(m1["boundaries"], 1);

    // 3. Translate solid by (15, -10, 20)
    let trans_res = session.execute(
        "solid.translate",
        &json!({
            "handle": handle.clone(),
            "delta": [15.0, -10.0, 20.0]
        }),
    ).expect("execute solid.translate");
    assert_eq!(trans_res["status"], "ok");
    assert_eq!(trans_res["bbox"]["min"], json!([25.0, 10.0, 50.0]));
    assert_eq!(trans_res["bbox"]["max"], json!([125.0, 60.0, 75.0]));

    // 4. Query position again
    let q2 = session.execute(
        "query.solid",
        &json!({ "handle": handle.clone() }),
    ).expect("execute query.solid after translate");
    assert_eq!(q2["bbox"]["min"], json!([25.0, 10.0, 50.0]));
    assert_eq!(q2["bbox"]["max"], json!([125.0, 60.0, 75.0]));
    assert_eq!(q2["features_count"], 2);

    // 5. Save the drawing to DXF, close it, and reopen it
    session.execute(
        "saveas",
        &json!({ "path": dxf_path.to_string_lossy() }),
    ).expect("execute saveas");
    assert!(dxf_path.exists(), "dxf file must exist on disk");

    session.execute("close", &json!({})).expect("execute close");

    let open_res = session.execute(
        "open",
        &json!({ "path": dxf_path.to_string_lossy() }),
    ).expect("execute open");
    assert_eq!(open_res["entities"].as_u64().unwrap_or(0), 1);

    // 6. Verify entity and dimensions are preserved
    let q3 = session.execute(
        "query.solid",
        &json!({ "handle": handle.clone() }),
    ).expect("execute query.solid after reopen");
    assert_eq!(q3["status"], "ok");
    assert_eq!(q3["handle"], handle);
    assert_eq!(q3["type"], "Solid3d");
    assert_eq!(q3["bbox"]["min"], json!([25.0, 10.0, 50.0]));
    assert_eq!(q3["bbox"]["max"], json!([125.0, 60.0, 75.0]));
    assert_eq!(q3["features_count"], 2);

    let m2 = session.execute(
        "query.metrics",
        &json!({ "handle": handle.clone() }),
    ).expect("execute query.metrics after reopen");
    let reopened_volume = m2["volume"].as_f64().expect("volume f64");
    assert!((reopened_volume - 125000.0).abs() < 1e-2);
    assert_eq!(m2["boundaries"], 1);

    // 7. Export to STL and verify file exists and contains valid binary mesh
    let stl_res = session.execute(
        "export.stl",
        &json!({
            "handles": [handle],
            "path": stl_path.to_string_lossy(),
            "tolerance": 0.05
        }),
    ).expect("execute export.stl");
    assert_eq!(stl_res["status"], "ok");
    assert_eq!(stl_res["solids_count"], 1);
    let triangles_count = stl_res["triangles_count"].as_u64().expect("triangles_count");
    assert_eq!(triangles_count, 12, "box has 12 triangles");

    assert!(stl_path.exists(), "stl file must exist on disk");
    let stl_bytes = fs::read(&stl_path).expect("read exported stl");
    // Binary STL has 80 bytes header + 4 bytes triangle count + 50 bytes per triangle
    let expected_stl_size = 80 + 4 + 12 * 50;
    assert_eq!(stl_bytes.len(), expected_stl_size, "stl binary file size must match format spec");

    // Clean up temp dir
    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_solid3d_cylinder_and_validation() {
    let mut session = Session::new();

    // 1. Valid cylinder: radius 10, height 50
    let cyl_res = session.execute(
        "solid.cylinder",
        &json!({
            "center": [0.0, 0.0, 0.0],
            "radius": 10.0,
            "height": 50.0,
            "axis": [0.0, 0.0, 1.0]
        }),
    ).expect("execute solid.cylinder");
    assert_eq!(cyl_res["status"], "ok");
    let handle = cyl_res["handle"].as_str().expect("handle").to_string();

    let m = session.execute("query.metrics", &json!({ "handle": handle })).expect("query.metrics");
    let volume = m["volume"].as_f64().expect("volume");
    let expected_volume = std::f64::consts::PI * 100.0 * 50.0;
    assert!((volume - expected_volume).abs() < 50.0, "expected ~{expected_volume}, got {volume}");

    // 2. Structured error validation for bad inputs
    let bad_box = session.execute(
        "solid.box",
        &json!({
            "origin": [0.0, 0.0, 0.0],
            "size": [-10.0, 50.0, 25.0]
        }),
    );
    assert!(bad_box.is_err(), "negative box dimensions must be rejected");

    let bad_cyl = session.execute(
        "solid.cylinder",
        &json!({
            "radius": -5.0,
            "height": 20.0
        }),
    );
    assert!(bad_cyl.is_err(), "negative cylinder radius must be rejected");

    let missing_handle = session.execute(
        "query.solid",
        &json!({ "handle": "DEADBEEF" }),
    );
    assert!(missing_handle.is_err(), "non-existent handle must return structured error");
}

#[test]
fn test_solid3d_undo_redo() {
    let mut session = Session::new();

    // Create box
    let box_res = session.execute(
        "solid.box",
        &json!({
            "origin": [0.0, 0.0, 0.0],
            "size": [10.0, 10.0, 10.0]
        }),
    ).expect("execute solid.box");
    let handle = box_res["handle"].as_str().expect("handle").to_string();

    // Translate box
    session.execute(
        "solid.translate",
        &json!({
            "handle": handle.clone(),
            "delta": [50.0, 0.0, 0.0]
        }),
    ).expect("execute solid.translate");

    let q_after = session.execute("query.solid", &json!({ "handle": handle.clone() })).expect("query.solid");
    assert_eq!(q_after["bbox"]["min"], json!([50.0, 0.0, 0.0]));

    // Undo translation
    session.undo().expect("undo translation");
    let q_undo = session.execute("query.solid", &json!({ "handle": handle.clone() })).expect("query.solid");
    assert_eq!(q_undo["bbox"]["min"], json!([0.0, 0.0, 0.0]));

    // Redo translation
    session.redo().expect("redo translation");
    let q_redo = session.execute("query.solid", &json!({ "handle": handle.clone() })).expect("query.solid");
    assert_eq!(q_redo["bbox"]["min"], json!([50.0, 0.0, 0.0]));

    // Undo translation and creation
    session.undo().expect("undo translation");
    session.undo().expect("undo box creation");
    assert!(session.execute("query.solid", &json!({ "handle": handle.clone() })).is_err(), "entity must not exist after undoing creation");
}

#[test]
fn test_solid_extrude_e2e() {
    use cadcraft_doc::{EntityKind, FeatureOp, Handle, LwPolyline};
    use cadcraft_geom::{PolyVertex, Vec2};

    set_io(IoHooks {
        read: |bytes, name| cadcraft_io::read(bytes, name).map_err(|e| e.to_string()),
        write: |doc, name| cadcraft_io::write(doc, name).map_err(|e| e.to_string()),
        plot: None,
    });

    let temp_dir = std::env::temp_dir().join(format!("cadcraft-extrude-test-{}", std::process::id()));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let dxf_path = temp_dir.join("extrude_rect.dxf");
    let stl_path = temp_dir.join("extrude_rect.stl");

    let mut session = Session::new();

    // 1. Create a closed rectangular 2D polyline of 30 x 20
    let poly = LwPolyline {
        vertices: vec![
            PolyVertex::new(Vec2::new(0.0, 0.0)),
            PolyVertex::new(Vec2::new(30.0, 0.0)),
            PolyVertex::new(Vec2::new(30.0, 20.0)),
            PolyVertex::new(Vec2::new(0.0, 20.0)),
        ],
        closed: true,
        const_width: 0.0,
        elevation: 0.0,
        plinegen: false,
    };
    let poly_handle = session.add_entity(EntityKind::LwPolyline(poly)).expect("add polyline");

    // 2. Extrude by height 25
    let ext_res = session.execute(
        "solid.extrude",
        &json!({
            "profile": poly_handle.hex(),
            "height": 25.0
        }),
    ).expect("execute solid.extrude");

    assert_eq!(ext_res["status"], "ok");
    let solid_handle = ext_res["handle"].as_str().expect("solid handle").to_string();
    assert_eq!(ext_res["bbox"]["min"], json!([0.0, 0.0, 0.0]));
    assert_eq!(ext_res["bbox"]["max"], json!([30.0, 20.0, 25.0]));

    // 3. Check query.metrics: expected volume = 30 * 20 * 25 = 15,000
    let m = session.execute(
        "query.metrics",
        &json!({ "handle": solid_handle.clone() }),
    ).expect("execute query.metrics");
    assert_eq!(m["status"], "ok");
    let volume = m["volume"].as_f64().expect("volume f64");
    assert!((volume - 15000.0).abs() < 1e-1, "volume expected 15000, got {volume}");

    // 4. Verify recipe contains FeatureOp::Extrusion
    let q = session.execute(
        "query.solid",
        &json!({ "handle": solid_handle.clone() }),
    ).expect("execute query.solid");
    assert_eq!(q["features_count"], 1);

    let doc = session.doc().expect("session doc");
    let ent = doc.entity(Handle::parse_hex(&solid_handle).expect("parse hex")).expect("entity");
    let EntityKind::Solid3d(s) = &ent.kind else { panic!("not a solid") };
    assert!(matches!(s.recipe.features[0].op, FeatureOp::Extrusion { .. }));

    // 5. Save and reopen DXF, preserving the entity
    session.execute(
        "saveas",
        &json!({ "path": dxf_path.to_string_lossy() }),
    ).expect("execute saveas");
    assert!(dxf_path.exists());

    session.execute("close", &json!({})).expect("execute close");
    session.execute(
        "open",
        &json!({ "path": dxf_path.to_string_lossy() }),
    ).expect("execute open");

    let reopened_q = session.execute("query.solid", &json!({ "handle": solid_handle.clone() })).expect("query after reopen");
    assert_eq!(reopened_q["status"], "ok");
    assert_eq!(reopened_q["bbox"]["min"], json!([0.0, 0.0, 0.0]));
    assert_eq!(reopened_q["bbox"]["max"], json!([30.0, 20.0, 25.0]));

    // 6. Test export.stl compatibility
    let stl_res = session.execute(
        "export.stl",
        &json!({
            "handles": [solid_handle.clone()],
            "path": stl_path.to_string_lossy()
        }),
    ).expect("execute export.stl");
    assert_eq!(stl_res["status"], "ok");
    assert!(stl_path.exists());

    // 7. Verify rejection of an open polyline
    let open_poly = LwPolyline {
        vertices: vec![
            PolyVertex::new(Vec2::new(0.0, 0.0)),
            PolyVertex::new(Vec2::new(10.0, 0.0)),
            PolyVertex::new(Vec2::new(10.0, 10.0)),
        ],
        closed: false,
        const_width: 0.0,
        elevation: 0.0,
        plinegen: false,
    };
    let open_handle = session.add_entity(EntityKind::LwPolyline(open_poly)).expect("add open poly");
    let err_res = session.execute(
        "solid.extrude",
        &json!({
            "profile": open_handle.hex(),
            "height": 20.0
        }),
    );
    assert!(err_res.is_err(), "open polyline must be rejected");
    let err_msg = err_res.unwrap_err().to_string();
    assert!(err_msg.contains("closed"), "error must mention closed: {err_msg}");
}
