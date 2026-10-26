//! Integration tests for 3D solid boolean operations (`solid.union`, `solid.subtract`, `solid.intersect`).

use cadcraft_doc::{EntityKind, LwPolyline};
use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use cadcraft_geom::{PolyVertex, Vec2};
use serde_json::json;
use std::fs;

fn init_io_hooks() {
    set_io(IoHooks {
        read: |bytes, name| cadcraft_io::read(bytes, name).map_err(|e| e.to_string()),
        write: |doc, name| cadcraft_io::write(doc, name).map_err(|e| e.to_string()),
        plot: None,
    });
}

/// Create standard test boxes A and B with known transversal intersection.
/// Box A: [0..100, 0..100, 0..100] -> volume = 1,000,000.
/// Box B: [50..150, -10..110, -10..110] -> volume = 100 * 120 * 120 = 1,440,000.
/// Overlap: [50..100, 0..100, 0..100] -> volume = 50 * 100 * 100 = 500,000.
fn create_test_boxes(session: &mut Session) -> (String, String) {
    let box_a = session
        .execute(
            "solid.box",
            &json!({
                "origin": [0.0, 0.0, 0.0],
                "size": [100.0, 100.0, 100.0]
            }),
        )
        .expect("create box A");
    assert_eq!(box_a["status"], "ok");
    let handle_a = box_a["handle"].as_str().expect("handle A").to_string();

    let box_b = session
        .execute(
            "solid.box",
            &json!({
                "origin": [50.0, -10.0, -10.0],
                "size": [100.0, 120.0, 120.0]
            }),
        )
        .expect("create box B");
    assert_eq!(box_b["status"], "ok");
    let handle_b = box_b["handle"].as_str().expect("handle B").to_string();

    (handle_a, handle_b)
}

#[test]
fn test_solid_union_volume_and_bbox() {
    let mut session = Session::new();
    let (h_a, h_b) = create_test_boxes(&mut session);

    // Expected union volume: 1,000,000 + 1,440,000 - 500,000 = 1,940,000
    let res = session
        .execute(
            "solid.union",
            &json!({
                "target": h_a,
                "tool": h_b
            }),
        )
        .expect("execute solid.union");

    assert_eq!(res["status"], "ok");
    let union_handle = res["handle"].as_str().expect("union handle").to_string();
    assert_eq!(res["bbox"]["min"], json!([0.0, -10.0, -10.0]));
    assert_eq!(res["bbox"]["max"], json!([150.0, 110.0, 110.0]));

    // Query solid
    let q = session.execute("query.solid", &json!({ "handle": union_handle.clone() })).expect("query.solid");
    assert_eq!(q["status"], "ok");
    assert_eq!(q["type"], "Solid3d");
    assert_eq!(q["evaluated"], true);
    assert_eq!(q["features_count"], 3);

    // Query metrics
    let m = session.execute("query.metrics", &json!({ "handle": union_handle })).expect("query.metrics");
    assert_eq!(m["status"], "ok");
    let volume = m["volume"].as_f64().expect("volume");
    assert!((volume - 1_940_000.0).abs() < 100.0, "expected volume 1,940,000, got {volume}");
    assert_eq!(m["boundaries"], 1);
}

#[test]
fn test_solid_subtract_volume_and_bbox() {
    let mut session = Session::new();
    let (h_a, h_b) = create_test_boxes(&mut session);

    // Expected difference volume: 1,000,000 - 500,000 = 500,000
    let res = session
        .execute(
            "solid.subtract",
            &json!({
                "target": h_a,
                "tool": h_b
            }),
        )
        .expect("execute solid.subtract");

    assert_eq!(res["status"], "ok");
    let sub_handle = res["handle"].as_str().expect("sub handle").to_string();
    assert_eq!(res["bbox"]["min"], json!([0.0, 0.0, 0.0]));
    assert_eq!(res["bbox"]["max"], json!([50.0, 100.0, 100.0]));

    let m = session.execute("query.metrics", &json!({ "handle": sub_handle })).expect("query.metrics");
    assert_eq!(m["status"], "ok");
    let volume = m["volume"].as_f64().expect("volume");
    assert!((volume - 500_000.0).abs() < 100.0, "expected volume 500,000, got {volume}");
    assert_eq!(m["boundaries"], 1);
}

#[test]
fn test_solid_intersect_volume_and_bbox() {
    let mut session = Session::new();
    let (h_a, h_b) = create_test_boxes(&mut session);

    // Expected intersection volume: 500,000
    // Bbox: [50..100, 0..100, 0..100]
    let res = session
        .execute(
            "solid.intersect",
            &json!({
                "handles": [h_a, h_b]
            }),
        )
        .expect("execute solid.intersect");

    assert_eq!(res["status"], "ok");
    let int_handle = res["handle"].as_str().expect("int handle").to_string();
    assert_eq!(res["bbox"]["min"], json!([50.0, 0.0, 0.0]));
    assert_eq!(res["bbox"]["max"], json!([100.0, 100.0, 100.0]));

    let m = session.execute("query.metrics", &json!({ "handle": int_handle })).expect("query.metrics");
    assert_eq!(m["status"], "ok");
    let volume = m["volume"].as_f64().expect("volume");
    assert!((volume - 500_000.0).abs() < 100.0, "expected volume 500,000, got {volume}");
    assert_eq!(m["boundaries"], 1);
}

#[test]
fn test_solid_booleans_undo_redo() {
    let mut session = Session::new();
    let (h_a, h_b) = create_test_boxes(&mut session);

    let res = session
        .execute(
            "solid.union",
            &json!({
                "target": h_a.clone(),
                "tool": h_b.clone()
            }),
        )
        .expect("execute union");
    let union_handle = res["handle"].as_str().expect("handle").to_string();

    // Verify union entity exists
    assert!(session.execute("query.solid", &json!({ "handle": union_handle.clone() })).is_ok());

    // Undo boolean operation
    session.undo().expect("undo union");
    assert!(session.execute("query.solid", &json!({ "handle": union_handle.clone() })).is_err(), "union solid must not exist after undo");

    // Original operand entities are intact
    assert!(session.execute("query.solid", &json!({ "handle": h_a.clone() })).is_ok());
    assert!(session.execute("query.solid", &json!({ "handle": h_b.clone() })).is_ok());

    // Redo boolean operation
    session.redo().expect("redo union");
    assert!(session.execute("query.solid", &json!({ "handle": union_handle.clone() })).is_ok(), "union solid must exist after redo");
}

#[test]
fn test_solid_boolean_dxf_roundtrip_persistence() {
    init_io_hooks();

    let temp_dir = std::env::temp_dir().join(format!("cadcraft-bool-dxf-{}", std::process::id()));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let dxf_path = temp_dir.join("boolean_model.dxf");

    let mut session = Session::new();
    let (h_a, h_b) = create_test_boxes(&mut session);

    let res = session
        .execute(
            "solid.subtract",
            &json!({
                "target": h_a,
                "tool": h_b,
                "delete_operands": true
            }),
        )
        .expect("execute subtract with delete_operands");
    let sub_handle = res["handle"].as_str().expect("handle").to_string();

    // Save DXF
    session.execute("saveas", &json!({ "path": dxf_path.to_string_lossy() })).expect("saveas");
    assert!(dxf_path.exists());

    // Close and reopen
    session.execute("close", &json!({})).expect("close");
    let open_res = session.execute("open", &json!({ "path": dxf_path.to_string_lossy() })).expect("open");
    assert_eq!(open_res["entities"].as_u64().unwrap_or(0), 1);

    // Query solid and metrics after reopening
    let q = session.execute("query.solid", &json!({ "handle": sub_handle.clone() })).expect("query.solid after reopen");
    assert_eq!(q["status"], "ok");
    assert_eq!(q["evaluated"], true);
    assert_eq!(q["features_count"], 3);
    assert_eq!(q["bbox"]["min"], json!([0.0, 0.0, 0.0]));
    assert_eq!(q["bbox"]["max"], json!([50.0, 100.0, 100.0]));

    let m = session.execute("query.metrics", &json!({ "handle": sub_handle })).expect("query.metrics after reopen");
    assert_eq!(m["status"], "ok");
    let volume = m["volume"].as_f64().expect("volume");
    assert!((volume - 500_000.0).abs() < 100.0, "volume after reopen should be 500,000, got {volume}");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_solid_boolean_validation_and_errors() {
    let mut session = Session::new();
    let (h_a, h_b) = create_test_boxes(&mut session);

    // 1. Missing target/tool
    let err_missing = session.execute("solid.union", &json!({}));
    assert!(err_missing.is_err());
    let msg = err_missing.unwrap_err().to_string();
    assert!(msg.contains("missing `target`"), "must complain about target: {msg}");

    // 2. Incomplete handles array
    let err_handles = session.execute("solid.union", &json!({ "handles": [h_a.clone()] }));
    assert!(err_handles.is_err());
    let msg = err_handles.unwrap_err().to_string();
    assert!(msg.contains("exactly 2"), "must complain about array length: {msg}");

    // 3. Non-existent handle
    let err_nonexistent = session.execute("solid.subtract", &json!({ "target": "DEADBEEF", "tool": h_b.clone() }));
    assert!(err_nonexistent.is_err());
    let msg = err_nonexistent.unwrap_err().to_string();
    assert!(msg.contains("not found"), "must state not found: {msg}");

    // 4. Same entity as target and tool
    let err_same = session.execute("solid.intersect", &json!({ "target": h_a.clone(), "tool": h_a.clone() }));
    assert!(err_same.is_err());
    let msg = err_same.unwrap_err().to_string();
    assert!(msg.contains("distinct"), "must state distinct: {msg}");

    // 5. Incompatible entity type (LwPolyline)
    let poly = LwPolyline {
        vertices: vec![PolyVertex::new(Vec2::new(0.0, 0.0)), PolyVertex::new(Vec2::new(10.0, 0.0)), PolyVertex::new(Vec2::new(10.0, 10.0))],
        closed: true,
        const_width: 0.0,
        elevation: 0.0,
        plinegen: false,
    };
    let poly_handle = session.add_entity(EntityKind::LwPolyline(poly)).expect("add polyline");

    let err_incompatible = session.execute("solid.union", &json!({ "target": poly_handle.hex(), "tool": h_b }));
    assert!(err_incompatible.is_err());
    let msg = err_incompatible.unwrap_err().to_string();
    assert!(msg.contains("not a 3D Solid"), "must report incompatible entity: {msg}");
}

#[test]
fn test_solid_boolean_geometric_failure_atomicity() {
    let mut session = Session::new();

    // Create a cube [0..100, 0..100, 0..50]
    let box_res = session
        .execute(
            "solid.box",
            &json!({
                "origin": [0.0, 0.0, 0.0],
                "size": [100.0, 100.0, 50.0]
            }),
        )
        .expect("create box");
    let h_box = box_res["handle"].as_str().expect("handle").to_string();

    // Create a cylinder sharing top (Z=50) and bottom (Z=0) coplanar faces
    let cyl_res = session
        .execute(
            "solid.cylinder",
            &json!({
                "center": [50.0, 50.0, 0.0],
                "radius": 10.0,
                "height": 50.0,
                "axis": [0.0, 0.0, 1.0]
            }),
        )
        .expect("create cylinder");
    let h_cyl = cyl_res["handle"].as_str().expect("handle").to_string();

    let doc_before = session.doc().expect("doc").clone();
    let entity_count_before = doc_before.entity_count();

    // Attempt coplanar subtraction - must fail with kernel's non-transversal error
    let fail_res = session.execute(
        "solid.subtract",
        &json!({
            "target": h_box.clone(),
            "tool": h_cyl.clone()
        }),
    );
    assert!(fail_res.is_err(), "coplanar subtraction must fail");
    let err_msg = fail_res.unwrap_err().to_string();
    assert!(
        err_msg.contains("coplanar") || err_msg.contains("non-transversal") || err_msg.contains("faces are coincident"),
        "error message must preserve kernel explanation: {err_msg}"
    );

    // Verify atomicity: document is completely unchanged
    let doc_after = session.doc().expect("doc");
    assert_eq!(doc_after.entity_count(), entity_count_before, "no partial entities created");

    // Operands are untouched
    let q_box = session.execute("query.solid", &json!({ "handle": h_box })).expect("query box");
    assert_eq!(q_box["status"], "ok");
    let q_cyl = session.execute("query.solid", &json!({ "handle": h_cyl })).expect("query cyl");
    assert_eq!(q_cyl["status"], "ok");
}

#[test]
fn test_operands_behavior_contract() {
    let mut session = Session::new();
    let (h_a, h_b) = create_test_boxes(&mut session);

    // 1. By default: non-destructive (delete_operands = false)
    let res_keep = session
        .execute(
            "solid.union",
            &json!({
                "target": h_a.clone(),
                "tool": h_b.clone()
            }),
        )
        .expect("execute union");
    assert_eq!(res_keep["status"], "ok");
    assert!(res_keep.get("deleted").is_none());

    // Both original entities still exist and can be queried
    assert!(session.execute("query.solid", &json!({ "handle": h_a.clone() })).is_ok());
    assert!(session.execute("query.solid", &json!({ "handle": h_b.clone() })).is_ok());

    // 2. With delete_operands: true
    let res_delete = session
        .execute(
            "solid.intersect",
            &json!({
                "target": h_a.clone(),
                "tool": h_b.clone(),
                "delete_operands": true
            }),
        )
        .expect("execute intersect with delete_operands");
    assert_eq!(res_delete["status"], "ok");
    assert_eq!(res_delete["deleted"], json!([h_a, h_b]));

    // Original entities no longer exist
    assert!(session.execute("query.solid", &json!({ "handle": h_a })).is_err());
    assert!(session.execute("query.solid", &json!({ "handle": h_b })).is_err());
}

#[test]
fn test_chained_booleans_multi_feature_recipe_evaluation() {
    init_io_hooks();

    let temp_dir = std::env::temp_dir().join(format!("cadcraft-chained-bool-{}", std::process::id()));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let dxf_path = temp_dir.join("chained_boolean.dxf");

    let mut session = Session::new();

    // Solid A: Box 100 x 100 x 100 at origin (0, 0, 0) -> Vol = 1,000,000
    let box_a = session
        .execute(
            "solid.box",
            &json!({
                "origin": [0.0, 0.0, 0.0],
                "size": [100.0, 100.0, 100.0]
            }),
        )
        .expect("create box A");
    let h_a = box_a["handle"].as_str().expect("h_a").to_string();

    // Solid B: Cutting box [50..150, -10..110, -10..110] -> cuts X in [50, 100]
    let box_b = session
        .execute(
            "solid.box",
            &json!({
                "origin": [50.0, -10.0, -10.0],
                "size": [100.0, 120.0, 120.0]
            }),
        )
        .expect("create box B");
    let h_b = box_b["handle"].as_str().expect("h_b").to_string();

    // Boolean 1: Subtract box B from box A -> C (leaves X: [0..50], Y: [0..100], Z: [0..100])
    // Vol(C) = 50 * 100 * 100 = 500,000
    let cut_c = session
        .execute(
            "solid.subtract",
            &json!({
                "target": h_a,
                "tool": h_b,
                "delete_operands": true
            }),
        )
        .expect("cut C");
    let h_c = cut_c["handle"].as_str().expect("h_c").to_string();

    let q_c = session.execute("query.solid", &json!({ "handle": h_c.clone() })).expect("q_c");
    assert_eq!(q_c["features_count"], 3); // Box A + Box B + Difference

    // Solid D: Cutting box [-10..110, 50..150, -10..110] -> cuts Y in [50, 100]
    let box_d = session
        .execute(
            "solid.box",
            &json!({
                "origin": [-10.0, 50.0, -10.0],
                "size": [120.0, 100.0, 120.0]
            }),
        )
        .expect("box D");
    let h_d = box_d["handle"].as_str().expect("h_d").to_string();

    // Boolean 2: Subtract box D from cut C -> E (leaves X: [0..50], Y: [0..50], Z: [0..100])
    // Chained boolean merges 3-feature recipe + 1-feature recipe + Difference = 5 features
    // Expected Vol(E) = 50 * 50 * 100 = 250,000
    let cut_e = session
        .execute(
            "solid.subtract",
            &json!({
                "target": h_c,
                "tool": h_d,
                "delete_operands": true
            }),
        )
        .expect("cut E");
    let h_e = cut_e["handle"].as_str().expect("h_e").to_string();

    let q_e = session.execute("query.solid", &json!({ "handle": h_e.clone() })).expect("q_e");
    assert_eq!(q_e["features_count"], 5); // (Box + Box + Diff) + Box + Diff = 5 features

    let m_e = session.execute("query.metrics", &json!({ "handle": h_e.clone() })).expect("m_e");
    let vol_e = m_e["volume"].as_f64().expect("vol_e");
    assert!((vol_e - 250_000.0).abs() < 100.0, "chained boolean expected ~250,000, got {vol_e}");

    // Save and reopen DXF, evaluating the merged 5-feature recipe completely from scratch
    session.execute("saveas", &json!({ "path": dxf_path.to_string_lossy() })).expect("saveas");
    session.execute("close", &json!({})).expect("close");
    session.execute("open", &json!({ "path": dxf_path.to_string_lossy() })).expect("open");

    let q_reopen = session.execute("query.solid", &json!({ "handle": h_e.clone() })).expect("query after reopen");
    assert_eq!(q_reopen["features_count"], 5);
    assert_eq!(q_reopen["evaluated"], true);
    assert_eq!(q_reopen["bbox"]["min"], json!([0.0, 0.0, 0.0]));
    assert_eq!(q_reopen["bbox"]["max"], json!([50.0, 50.0, 100.0]));

    let m_reopen = session.execute("query.metrics", &json!({ "handle": h_e.clone() })).expect("metrics after reopen");
    let vol_reopen = m_reopen["volume"].as_f64().expect("vol_reopen");
    assert!((vol_reopen - 250_000.0).abs() < 100.0);

    // Translate the re-evaluated chained solid: appends 6th feature (Translation)
    session
        .execute(
            "solid.translate",
            &json!({
                "handle": h_e.clone(),
                "delta": [10.0, 20.0, 30.0]
            }),
        )
        .expect("translate chained solid");

    let q_trans = session.execute("query.solid", &json!({ "handle": h_e.clone() })).expect("query after translate");
    assert_eq!(q_trans["features_count"], 6);
    assert_eq!(q_trans["bbox"]["min"], json!([10.0, 20.0, 30.0]));
    assert_eq!(q_trans["bbox"]["max"], json!([60.0, 70.0, 130.0]));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_conflicting_parameters_rejected() {
    let mut session = Session::new();
    let (h_a, h_b) = create_test_boxes(&mut session);

    // Conflicting target and from
    let err = session.execute(
        "solid.subtract",
        &json!({
            "target": h_a.clone(),
            "from": h_b.clone(),
            "tool": h_b.clone()
        }),
    );
    assert!(err.is_err());
    assert!(err.unwrap_err().to_string().contains("conflicting"));

    // Conflicting oversized and oversize_through
    let err2 = session.execute(
        "solid.subtract",
        &json!({
            "target": h_a.clone(),
            "tool": h_b.clone(),
            "oversized": true,
            "oversize_through": false
        }),
    );
    assert!(err2.is_err());
    assert!(err2.unwrap_err().to_string().contains("conflicting"));

    // Handles array with wrong length (e.g. 3)
    let err3 = session.execute(
        "solid.union",
        &json!({
            "handles": [h_a.clone(), h_b.clone(), "10C"]
        }),
    );
    assert!(err3.is_err());
    assert!(err3.unwrap_err().to_string().contains("exactly 2"));
}

#[test]
fn test_disjoint_intersection_empty_result_atomicity() {
    let mut session = Session::new();

    // Box A: [0..10, 0..10, 0..10]
    let box_a = session.execute("solid.box", &json!({ "origin": [0.0, 0.0, 0.0], "size": [10.0, 10.0, 10.0] })).expect("box a");
    let h_a = box_a["handle"].as_str().expect("h_a").to_string();

    // Box B: [100..110, 100..110, 100..110] (completely disjoint)
    let box_b = session.execute("solid.box", &json!({ "origin": [100.0, 100.0, 100.0], "size": [10.0, 10.0, 10.0] })).expect("box b");
    let h_b = box_b["handle"].as_str().expect("h_b").to_string();

    let doc_before = session.doc().expect("doc").clone();
    let entity_count = doc_before.entity_count();

    // Intersect disjoint solids with delete_operands: true
    let res = session.execute(
        "solid.intersect",
        &json!({
            "target": h_a.clone(),
            "tool": h_b.clone(),
            "delete_operands": true
        }),
    );
    assert!(res.is_err(), "disjoint intersection must fail as empty solid");
    let msg = res.unwrap_err().to_string();
    assert!(msg.contains("empty solid"), "must report empty solid: {msg}");

    // Verify atomicity: operands are NOT deleted and no new entity was created
    let doc_after = session.doc().expect("doc");
    assert_eq!(doc_after.entity_count(), entity_count);
    assert!(session.execute("query.solid", &json!({ "handle": h_a })).is_ok());
    assert!(session.execute("query.solid", &json!({ "handle": h_b })).is_ok());
}

#[test]
fn test_solid_booleans_undo_redo_with_delete_operands() {
    let mut session = Session::new();
    let (h_a, h_b) = create_test_boxes(&mut session);

    // Subtract with delete_operands: true
    let res = session
        .execute(
            "solid.subtract",
            &json!({
                "target": h_a.clone(),
                "tool": h_b.clone(),
                "delete_operands": true
            }),
        )
        .expect("execute subtract with delete_operands");
    let sub_handle = res["handle"].as_str().expect("handle").to_string();

    // Operands deleted, subtracted solid exists
    assert!(session.execute("query.solid", &json!({ "handle": sub_handle.clone() })).is_ok());
    assert!(session.execute("query.solid", &json!({ "handle": h_a.clone() })).is_err());
    assert!(session.execute("query.solid", &json!({ "handle": h_b.clone() })).is_err());

    // Undo: operands restored, subtracted solid removed
    session.undo().expect("undo subtract");
    assert!(session.execute("query.solid", &json!({ "handle": sub_handle.clone() })).is_err());
    assert!(session.execute("query.solid", &json!({ "handle": h_a.clone() })).is_ok());
    assert!(session.execute("query.solid", &json!({ "handle": h_b.clone() })).is_ok());

    // Redo: operands removed again, subtracted solid restored
    session.redo().expect("redo subtract");
    assert!(session.execute("query.solid", &json!({ "handle": sub_handle.clone() })).is_ok());
    assert!(session.execute("query.solid", &json!({ "handle": h_a })).is_err());
    assert!(session.execute("query.solid", &json!({ "handle": h_b })).is_err());
}

#[test]
fn test_solid_booleans_stl_export() {
    let temp_dir = std::env::temp_dir().join(format!("cadcraft-bool-stl-{}", std::process::id()));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let union_stl_path = temp_dir.join("union.stl");
    let subtract_stl_path = temp_dir.join("subtract.stl");

    let mut session = Session::new();
    let (h_a, h_b) = create_test_boxes(&mut session);

    // 1. Union solid
    let res_union = session.execute("solid.union", &json!({ "target": h_a.clone(), "tool": h_b.clone() })).expect("union");
    let union_h = res_union["handle"].as_str().expect("union handle").to_string();

    // Export union to STL
    let stl_union_res =
        session.execute("export.stl", &json!({ "handle": union_h, "path": union_stl_path.to_string_lossy() })).expect("export union STL");
    assert_eq!(stl_union_res["status"], "ok");
    assert!(union_stl_path.exists());

    let bytes = fs::read(&union_stl_path).expect("read union stl");
    assert!(bytes.len() >= 84);
    let tri_count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
    assert_eq!(tri_count, stl_union_res["triangles_count"].as_u64().unwrap() as usize);
    assert!(tri_count > 0);

    // 2. Subtract solid (hollow cut)
    let res_sub = session.execute("solid.subtract", &json!({ "target": h_a, "tool": h_b })).expect("subtract");
    let sub_h = res_sub["handle"].as_str().expect("sub handle").to_string();

    let stl_sub_res =
        session.execute("export.stl", &json!({ "handle": sub_h, "path": subtract_stl_path.to_string_lossy() })).expect("export subtract STL");
    assert_eq!(stl_sub_res["status"], "ok");
    assert!(subtract_stl_path.exists());

    let sub_bytes = fs::read(&subtract_stl_path).expect("read subtract stl");
    assert!(sub_bytes.len() >= 84);
    let sub_tri_count = u32::from_le_bytes(sub_bytes[80..84].try_into().unwrap()) as usize;
    assert_eq!(sub_tri_count, stl_sub_res["triangles_count"].as_u64().unwrap() as usize);
    assert!(sub_tri_count > 0);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_autocad_standard_commands_dispatch_to_solids_and_reject_mixed() {
    // 1. UNION command on solids
    let mut session = Session::new();
    let (h_a, h_b) = create_test_boxes(&mut session);

    let res_union = session.execute("union", &json!({ "handles": [h_a.clone(), h_b.clone()] })).expect("standard union command");
    let union_h = res_union["handle"].as_str().expect("union handle");

    // Operands deleted, union solid exists and has expected volume
    assert!(session.execute("query.solid", &json!({ "handle": h_a })).is_err());
    assert!(session.execute("query.solid", &json!({ "handle": h_b })).is_err());
    let m = session.execute("query.metrics", &json!({ "handle": union_h })).expect("metrics");
    let vol = m["volume"].as_f64().expect("vol");
    assert!((vol - 1_940_000.0).abs() < 100.0);

    // 2. SUBTRACT command on solids
    let mut session = Session::new();
    let (h_a, h_b) = create_test_boxes(&mut session);

    let res_sub = session.execute("subtract", &json!({ "from": [h_a.clone()], "remove": [h_b.clone()] })).expect("standard subtract command");
    let sub_h = res_sub["handle"].as_str().expect("sub handle");

    assert!(session.execute("query.solid", &json!({ "handle": h_a })).is_err());
    assert!(session.execute("query.solid", &json!({ "handle": h_b })).is_err());
    let m_sub = session.execute("query.metrics", &json!({ "handle": sub_h })).expect("metrics");
    let vol_sub = m_sub["volume"].as_f64().expect("vol");
    assert!((vol_sub - 500_000.0).abs() < 100.0);

    // 3. INTERSECT command on solids
    let mut session = Session::new();
    let (h_a, h_b) = create_test_boxes(&mut session);

    let res_int = session.execute("intersect", &json!({ "handles": [h_a.clone(), h_b.clone()] })).expect("standard intersect command");
    let int_h = res_int["handle"].as_str().expect("int handle");

    assert!(session.execute("query.solid", &json!({ "handle": h_a })).is_err());
    assert!(session.execute("query.solid", &json!({ "handle": h_b })).is_err());
    let m_int = session.execute("query.metrics", &json!({ "handle": int_h })).expect("metrics");
    let vol_int = m_int["volume"].as_f64().expect("vol");
    assert!((vol_int - 500_000.0).abs() < 100.0);

    // 4. Mixing solids and regions must be cleanly rejected
    let mut session = Session::new();
    let (h_solid, _) = create_test_boxes(&mut session);

    let rect = session.execute("rectang", &json!({ "p1": [0.0, 0.0], "p2": [50.0, 50.0] })).expect("rectang");
    let rect_h = rect["handle"].as_str().expect("rect handle").to_string();
    session.execute("region", &json!({ "handles": [rect_h.clone()] })).expect("region");

    let err_mixed = session.execute("union", &json!({ "handles": [h_solid, rect_h] }));
    assert!(err_mixed.is_err(), "mixing solid and region must fail");
    let msg = err_mixed.unwrap_err().to_string();
    assert!(msg.contains("Cannot combine solids with regions"), "expected clear message: {msg}");
}
