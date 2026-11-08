use cadcraft_engine::Session;
use cadcraft_geom::Vec3;
use serde_json::json;

#[test]
fn test_ucs_3point_definition_and_properties() {
    let mut s = Session::new();
    assert!(s.ucs().is_world());

    // 1. Define oblique UCS:
    // Origin = (0, 0, 5), X = (10, 0, 5), Y point = (0, 10, 15)
    let res = s.execute(
        "ucs.3point",
        &json!({
            "origin": [0.0, 0.0, 5.0],
            "x_axis": [10.0, 0.0, 5.0],
            "y_pt": [0.0, 10.0, 15.0]
        }),
    );
    assert!(res.is_ok(), "ucs.3point execute should succeed");

    let ucs = s.ucs();
    assert!(!ucs.is_world());
    assert!(ucs.origin.near(Vec3::new(0.0, 0.0, 5.0), 1e-9));
    assert!(ucs.x_axis.near(Vec3::new(1.0, 0.0, 0.0), 1e-9));

    // Expected normal: (0, 10, 10) normalized is (0, 1/sqrt(2), 1/sqrt(2))
    // X x Y = (1, 0, 0) x (0, 1/sqrt(2), 1/sqrt(2)) = (0, -1/sqrt(2), 1/sqrt(2))
    let inv_sqrt2 = 1.0 / 2.0_f64.sqrt();
    let expected_z = Vec3::new(0.0, -inv_sqrt2, inv_sqrt2);
    let expected_y = Vec3::new(0.0, inv_sqrt2, inv_sqrt2);
    assert!(ucs.z_axis.near(expected_z, 1e-6), "Z axis matches normal: {:?}", ucs.z_axis);
    assert!(ucs.y_axis.near(expected_y, 1e-6), "Y axis is orthonormalized: {:?}", ucs.y_axis);
}

#[test]
fn test_draw_circle_and_rectangle_on_oblique_ucs() {
    let mut s = Session::new();

    // Set oblique UCS
    let _ = s.execute(
        "ucs.3point",
        &json!({
            "origin": [0.0, 0.0, 5.0],
            "x_axis": [10.0, 0.0, 5.0],
            "y_pt": [0.0, 10.0, 15.0]
        }),
    ).expect("ucs.3point");

    let ucs = s.ucs();

    // Draw circle with local center (0, 0) and radius 5.0
    let res_c = s.execute(
        "circle",
        &json!({
            "center": [0.0, 0.0],
            "radius": 5.0
        }),
    ).expect("circle");
    let c_handle = res_c["handle"].as_str().expect("circle handle");

    // Draw rectangle with corners (0, 0) to (10, 20)
    let res_r = s.execute(
        "rectang",
        &json!({
            "p1": [0.0, 0.0],
            "p2": [10.0, 20.0]
        }),
    ).expect("rectang");
    let r_handle = res_r["handle"].as_str().expect("rectang handle");

    // Verify entities in document
    let doc = s.doc().expect("doc");
    let c_ent = doc.entity(cadcraft_doc::Handle::parse_hex(c_handle).unwrap()).expect("circle ent");
    let r_ent = doc.entity(cadcraft_doc::Handle::parse_hex(r_handle).unwrap()).expect("rectang ent");

    // Circle verification
    assert!(c_ent.common.extrusion.near(ucs.z_axis, 1e-6));
    if let cadcraft_doc::EntityKind::Circle(c) = &c_ent.kind {
        assert!(c.center.near(Vec3::new(0.0, 0.0, 5.0), 1e-9));
        assert!((c.radius - 5.0).abs() < 1e-9);
    } else {
        panic!("expected circle entity");
    }

    // Rectangle verification
    assert!(r_ent.common.extrusion.near(ucs.z_axis, 1e-6));
    if let cadcraft_doc::EntityKind::LwPolyline(pl) = &r_ent.kind {
        let v3d = pl.vertices_3d(r_ent.common.extrusion);
        assert_eq!(v3d.len(), 4);

        // Verify the 4 3D vertices match the local UCS vertices transformed to world
        let p0_expected = ucs.to_world(Vec3::new(0.0, 0.0, 0.0));
        let p1_expected = ucs.to_world(Vec3::new(10.0, 0.0, 0.0));
        let p2_expected = ucs.to_world(Vec3::new(10.0, 20.0, 0.0));
        let p3_expected = ucs.to_world(Vec3::new(0.0, 20.0, 0.0));

        assert!(v3d[0].near(p0_expected, 1e-6), "v0: {:?} vs {:?}", v3d[0], p0_expected);
        assert!(v3d[1].near(p1_expected, 1e-6), "v1: {:?} vs {:?}", v3d[1], p1_expected);
        assert!(v3d[2].near(p2_expected, 1e-6), "v2: {:?} vs {:?}", v3d[2], p2_expected);
        assert!(v3d[3].near(p3_expected, 1e-6), "v3: {:?} vs {:?}", v3d[3], p3_expected);
    } else {
        panic!("expected lwpolyline entity");
    }

    // Switch to World UCS: verify existing entities do NOT move or rotate!
    let _ = s.execute("ucs.world", &json!({})).expect("ucs.world");
    assert!(s.ucs().is_world());

    let doc_w = s.doc().expect("doc");
    let c_ent_w = doc_w.entity(cadcraft_doc::Handle::parse_hex(c_handle).unwrap()).expect("circle ent");
    let r_ent_w = doc_w.entity(cadcraft_doc::Handle::parse_hex(r_handle).unwrap()).expect("rectang ent");
    assert!(c_ent_w.common.extrusion.near(ucs.z_axis, 1e-6));
    assert!(r_ent_w.common.extrusion.near(ucs.z_axis, 1e-6));
}

#[test]
fn test_ucs_move_origin_and_restore_previous() {
    let mut s = Session::new();

    // 1. Initial 3-point UCS
    let _ = s.execute(
        "ucs.3point",
        &json!({
            "origin": [0.0, 0.0, 5.0],
            "x_axis": [10.0, 0.0, 5.0],
            "y_pt": [0.0, 10.0, 15.0]
        }),
    ).expect("ucs.3point");

    let ucs1 = s.ucs();

    // 2. Move origin without changing orientation
    let _ = s.execute(
        "ucs.origin",
        &json!({
            "origin": [100.0, 200.0, 300.0]
        }),
    ).expect("ucs.origin");

    let ucs2 = s.ucs();
    assert!(ucs2.origin.near(Vec3::new(100.0, 200.0, 300.0), 1e-9));
    assert!(ucs2.x_axis.near(ucs1.x_axis, 1e-9));
    assert!(ucs2.y_axis.near(ucs1.y_axis, 1e-9));
    assert!(ucs2.z_axis.near(ucs1.z_axis, 1e-9));

    // 3. Restore previous UCS
    let _ = s.execute("ucs.previous", &json!({})).expect("ucs.previous");
    let ucs_restored = s.ucs();
    assert!(ucs_restored.origin.near(ucs1.origin, 1e-9));
    assert!(ucs_restored.x_axis.near(ucs1.x_axis, 1e-9));
    assert!(ucs_restored.y_axis.near(ucs1.y_axis, 1e-9));
    assert!(ucs_restored.z_axis.near(ucs1.z_axis, 1e-9));
}

#[test]
fn test_ucs_rejects_coincident_and_collinear_points() {
    let mut s = Session::new();
    let initial_ucs = s.ucs();

    // Coincident origin and X point
    let res_coincident = s.execute(
        "ucs.3point",
        &json!({
            "origin": [5.0, 5.0, 5.0],
            "x_axis": [5.0, 5.0, 5.0],
            "y_pt": [0.0, 10.0, 10.0]
        }),
    );
    assert!(res_coincident.is_err());
    assert_eq!(s.ucs(), initial_ucs);

    // Collinear points
    let res_collinear = s.execute(
        "ucs.3point",
        &json!({
            "origin": [0.0, 0.0, 0.0],
            "x_axis": [10.0, 0.0, 0.0],
            "y_pt": [20.0, 0.0, 0.0]
        }),
    );
    assert!(res_collinear.is_err());
    assert_eq!(s.ucs(), initial_ucs);
}

#[test]
fn test_ucs_dxf_roundtrip_persistence() {
    let mut s = Session::new();

    // Set oblique UCS
    let _ = s.execute(
        "ucs.3point",
        &json!({
            "origin": [10.0, 20.0, 30.0],
            "x_axis": [20.0, 20.0, 30.0],
            "y_pt": [10.0, 30.0, 40.0]
        }),
    ).expect("ucs.3point");

    let ucs_orig = s.ucs();

    // Create circle on UCS
    let _ = s.execute("circle", &json!({ "center": [0.0, 0.0], "radius": 15.0 })).expect("circle");
    // Create rectangle on UCS
    let _ = s.execute("rectang", &json!({ "p1": [0.0, 0.0], "p2": [50.0, 25.0] })).expect("rectang");

    // Write DXF
    let doc = s.doc().expect("doc");
    let dxf_str = cadcraft_io::write_dxf(doc);

    // Verify DXF text contains header variables and groups
    assert!(dxf_str.contains("$UCSORG"), "DXF contains $UCSORG");
    assert!(dxf_str.contains("$UCSXDIR"), "DXF contains $UCSXDIR");
    assert!(dxf_str.contains("$UCSYDIR"), "DXF contains $UCSYDIR");
    assert!(dxf_str.contains("CIRCLE"), "DXF contains CIRCLE");
    assert!(dxf_str.contains("LWPOLYLINE"), "DXF contains LWPOLYLINE");

    // Re-read DXF into a new drawing
    let read_doc = cadcraft_io::read_dxf(dxf_str.as_bytes()).expect("read_dxf");

    // Start a new session with the read document
    let mut s2 = Session::empty();
    s2.open_drawing(read_doc, "loaded.dxf", None);

    let ucs_loaded = s2.ucs();
    assert!(ucs_loaded.origin.near(ucs_orig.origin, 1e-6), "Origin persisted: {:?} vs {:?}", ucs_loaded.origin, ucs_orig.origin);
    assert!(ucs_loaded.x_axis.near(ucs_orig.x_axis, 1e-6), "X axis persisted: {:?} vs {:?}", ucs_loaded.x_axis, ucs_orig.x_axis);
    assert!(ucs_loaded.y_axis.near(ucs_orig.y_axis, 1e-6), "Y axis persisted: {:?} vs {:?}", ucs_loaded.y_axis, ucs_orig.y_axis);
    assert!(ucs_loaded.z_axis.near(ucs_orig.z_axis, 1e-6), "Z axis persisted: {:?} vs {:?}", ucs_loaded.z_axis, ucs_orig.z_axis);
}

#[test]
fn test_ucs_cmdline_interactive_scripting() {
    let mut s = Session::new();

    // 1. Define UCS via command-line typing: "ucs" -> "3p" -> origin -> x point -> y point
    let _ = s.cmdline("ucs");
    assert_eq!(s.running.as_ref().map(|r| r.id.as_str()), Some("ucs"));
    let _ = s.cmdline("3p");
    let _ = s.cmdline("0,0,5");
    let _ = s.cmdline("10,0,5");
    let _ = s.cmdline("0,10,15");
    assert!(s.running.is_none(), "UCS command completed");

    let ucs = s.ucs();
    assert!(ucs.origin.near(Vec3::new(0.0, 0.0, 5.0), 1e-9));

    // 2. Draw circle on UCS via command-line
    let _ = s.cmdline("circle");
    assert_eq!(s.running.as_ref().map(|r| r.id.as_str()), Some("circle"));
    let _ = s.cmdline("0,0");
    let _ = s.cmdline("5");
    assert!(s.running.is_none(), "CIRCLE command completed");

    // 3. Move origin via explicit world coordinates "*10,20,30"
    let _ = s.cmdline("ucs");
    let _ = s.cmdline("m");
    let _ = s.cmdline("*10,20,30");
    assert!(s.ucs().origin.near(Vec3::new(10.0, 20.0, 30.0), 1e-9));

    // 4. Restore previous via "ucs" "p"
    let _ = s.cmdline("ucs");
    let _ = s.cmdline("p");
    assert!(s.ucs().origin.near(Vec3::new(0.0, 0.0, 5.0), 1e-9));

    // 5. Restore world via "ucs" "w"
    let _ = s.cmdline("ucs");
    let _ = s.cmdline("w");
    assert!(s.ucs().is_world());
}

#[test]
fn test_ucs_cancellation_and_invalid_point_correction() {
    let mut s = Session::new();
    let initial_ucs = s.ucs();

    // 1. Cancel at Step 1 (Origin)
    let _ = s.cmdline("ucs");
    let _ = s.cmdline("3p");
    let _ = s.input(cadcraft_engine::Input::Cancel);
    assert!(s.running.is_none());
    assert_eq!(s.ucs(), initial_ucs);
    assert_eq!(s.state().unwrap().ucs_history.len(), 0);

    // 2. Cancel at Step 2 (X axis point)
    let _ = s.cmdline("ucs");
    let _ = s.cmdline("3p");
    let _ = s.cmdline("10,20,30");
    let _ = s.input(cadcraft_engine::Input::Cancel);
    assert!(s.running.is_none());
    assert_eq!(s.ucs(), initial_ucs);
    assert_eq!(s.state().unwrap().ucs_history.len(), 0);

    // 3. Cancel at Step 3 (Y axis point)
    let _ = s.cmdline("ucs");
    let _ = s.cmdline("3p");
    let _ = s.cmdline("10,20,30");
    let _ = s.cmdline("20,20,30");
    let _ = s.input(cadcraft_engine::Input::Cancel);
    assert!(s.running.is_none());
    assert_eq!(s.ucs(), initial_ucs);
    assert_eq!(s.state().unwrap().ucs_history.len(), 0);

    // 4. Cancel during move origin
    let _ = s.cmdline("ucs");
    let _ = s.cmdline("m");
    let _ = s.input(cadcraft_engine::Input::Cancel);
    assert!(s.running.is_none());
    assert_eq!(s.ucs(), initial_ucs);

    // 5. Invalid input followed by correction
    let _ = s.cmdline("ucs");
    let _ = s.cmdline("3p");
    let _ = s.cmdline("10,20,30"); // Origin set

    // Coincident X point -> rejected, prompt stays at X axis
    let _ = s.cmdline("10,20,30");
    assert!(s.running.is_some());

    // Valid X point -> accepted
    let _ = s.cmdline("20,20,30");
    assert!(s.running.is_some());

    // Collinear Y point -> rejected, prompt stays at Y
    let _ = s.cmdline("30,20,30");
    assert!(s.running.is_some());

    // Valid Y point -> completes!
    let _ = s.cmdline("10,30,30");
    assert!(s.running.is_none());
    assert!(!s.ucs().is_world());
    assert!(s.ucs().origin.near(Vec3::new(10.0, 20.0, 30.0), 1e-6));
    assert_eq!(s.state().unwrap().ucs_history.len(), 1);
}

#[test]
fn test_ucs_previous_exhaustion_is_safe() {
    let mut s = Session::new();
    assert!(s.ucs().is_world());
    assert_eq!(s.state().unwrap().ucs_history.len(), 0);

    // Restoring previous when history is empty must report gracefully and not panic
    let res = s.execute("ucs.previous", &json!({})).expect("ucs.previous empty");
    assert_eq!(res["restored"], false);
    assert!(s.ucs().is_world());

    // Set one UCS
    let _ = s.execute("ucs.origin", &json!({ "origin": [5.0, 5.0, 5.0] })).expect("origin");
    assert_eq!(s.state().unwrap().ucs_history.len(), 1);

    // Restore it
    let res2 = s.execute("ucs.previous", &json!({})).expect("ucs.previous 1");
    assert_eq!(res2["restored"], true);
    assert!(s.ucs().is_world());

    // History is now empty again
    assert_eq!(s.state().unwrap().ucs_history.len(), 0);
    let res3 = s.execute("ucs.previous", &json!({})).expect("ucs.previous 2");
    assert_eq!(res3["restored"], false);
    assert!(s.ucs().is_world());
}

#[test]
fn test_ucs_geometry_snaps_on_inclined_entities() {
    let mut s = Session::new();

    // Create 3D geometry in space
    let _ = s.execute(
        "line",
        &json!({
            "points": [[0.0, 0.0, 10.0], [10.0, 0.0, 10.0]]
        }),
    ).expect("line 1");

    let _ = s.execute(
        "line",
        &json!({
            "points": [[0.0, 0.0, 10.0], [0.0, 20.0, 30.0]]
        }),
    ).expect("line 2");

    // Camera in 3D orbit
    let _ = s.execute("view.swiso", &json!({})).expect("view.swiso");
    let cam = s.camera();
    let ap = 0.5;

    // Use 3D endpoint snap to find:
    // 1) Origin = (0, 0, 10)
    let doc = s.doc().unwrap();
    let q_orig = cam.project_point(Vec3::new(0.0, 0.0, 10.0));
    let hit_orig = cadcraft_engine::snap::snap_3d_endpoints(doc, &cadcraft_doc::Space::Model, &cam, q_orig, ap, cadcraft_engine::snap::mode::END, 0).expect("snap orig");
    let orig = hit_orig.point3.unwrap();
    assert!(orig.near(Vec3::new(0.0, 0.0, 10.0), 1e-6));

    // 2) X axis point = (10, 0, 10)
    let q_x = cam.project_point(Vec3::new(10.0, 0.0, 10.0));
    let hit_x = cadcraft_engine::snap::snap_3d_endpoints(doc, &cadcraft_doc::Space::Model, &cam, q_x, ap, cadcraft_engine::snap::mode::END, 0).expect("snap x");
    let pt_x = hit_x.point3.unwrap();
    assert!(pt_x.near(Vec3::new(10.0, 0.0, 10.0), 1e-6));

    // 3) Y side point = (0, 20, 30)
    let q_y = cam.project_point(Vec3::new(0.0, 20.0, 30.0));
    let hit_y = cadcraft_engine::snap::snap_3d_endpoints(doc, &cadcraft_doc::Space::Model, &cam, q_y, ap, cadcraft_engine::snap::mode::END, 0).expect("snap y");
    let pt_y = hit_y.point3.unwrap();
    assert!(pt_y.near(Vec3::new(0.0, 20.0, 30.0), 1e-6));

    // Define UCS by these snapped 3D points
    let _ = s.execute(
        "ucs.3point",
        &json!({
            "origin": [orig.x, orig.y, orig.z],
            "x_axis": [pt_x.x, pt_x.y, pt_x.z],
            "y_pt": [pt_y.x, pt_y.y, pt_y.z]
        }),
    ).expect("ucs.3point");

    assert!(!s.ucs().is_world());
    assert!(s.ucs().origin.near(orig, 1e-6));

    // Now draw circle and rectangle on this oblique UCS
    let _ = s.execute("circle", &json!({ "center": [0.0, 0.0], "radius": 5.0 })).expect("circle");
    let _ = s.execute("rectang", &json!({ "p1": [0.0, 0.0], "p2": [8.0, 12.0] })).expect("rectang");

    // Verify 3D snap on the newly drawn inclined circle:
    // Center snap must find exact 3D center in WCS:
    let doc = s.doc().unwrap();
    let q_c = cam.project_point(orig);
    let hit_cen = cadcraft_engine::snap::snap_3d_endpoints(doc, &cadcraft_doc::Space::Model, &cam, q_c, ap, cadcraft_engine::snap::mode::CEN, 0).expect("circle cen");
    assert!(hit_cen.point3.unwrap().near(orig, 1e-6));

    // Quadrant snap on inclined circle:
    let (ax, _, _) = cadcraft_geom::Mat4::ocs_basis(s.ucs().z_axis);
    let quad_pt = orig + ax * 5.0;
    let q_quad = cam.project_point(quad_pt);
    let hit_qua = cadcraft_engine::snap::snap_3d_endpoints(doc, &cadcraft_doc::Space::Model, &cam, q_quad, ap, cadcraft_engine::snap::mode::QUA, 0).expect("circle quad");
    assert!(hit_qua.point3.unwrap().near(quad_pt, 1e-6));

    // Verify 3D snap on the inclined rectangle:
    // Endpoint snap on corners:
    let r_p1_w = s.ucs().to_world(Vec3::new(8.0, 12.0, 0.0));
    let q_rp1 = cam.project_point(r_p1_w);
    let hit_rend = cadcraft_engine::snap::snap_3d_endpoints(doc, &cadcraft_doc::Space::Model, &cam, q_rp1, ap, cadcraft_engine::snap::mode::END, 0).expect("rect corner");
    assert!(hit_rend.point3.unwrap().near(r_p1_w, 1e-6));

    // Midpoint snap on edge:
    let r_mid_w = s.ucs().to_world(Vec3::new(4.0, 0.0, 0.0));
    let q_rmid = cam.project_point(r_mid_w);
    let hit_rmid = cadcraft_engine::snap::snap_3d_endpoints(doc, &cadcraft_doc::Space::Model, &cam, q_rmid, ap, cadcraft_engine::snap::mode::MID, 0).expect("rect mid");
    assert!(hit_rmid.point3.unwrap().near(r_mid_w, 1e-6));
}

#[test]
fn test_line_full_3d_depth_wcs_and_ucs() {
    let mut s = Session::new();

    // 1. In WCS:
    // Vertical line in Z by coordinates: (0,0,0) to (0,0,10)
    s.cmdline("line").unwrap();
    s.cmdline("0,0,0").unwrap();
    s.cmdline("0,0,10").unwrap();
    s.cmdline("").unwrap();

    // Oblique line with X, Y, Z variation: (0,0,0) to (10,20,30)
    s.cmdline("line").unwrap();
    s.cmdline("0,0,0").unwrap();
    s.cmdline("10,20,30").unwrap();
    s.cmdline("").unwrap();

    let doc = s.doc().unwrap();
    let l1 = doc.model.iter().nth(0).unwrap();
    let l2 = doc.model.iter().nth(1).unwrap();
    if let (cadcraft_doc::EntityKind::Line(a), cadcraft_doc::EntityKind::Line(b)) = (&l1.kind, &l2.kind) {
        assert!(a.a.near(Vec3::new(0.0, 0.0, 0.0), 1e-6));
        assert!(a.b.near(Vec3::new(0.0, 0.0, 10.0), 1e-6), "vertical Z line preserved: {:?}", a.b);
        assert!(b.a.near(Vec3::new(0.0, 0.0, 0.0), 1e-6));
        assert!(b.b.near(Vec3::new(10.0, 20.0, 30.0), 1e-6), "oblique 3D line preserved: {:?}", b.b);
    } else {
        panic!("expected lines");
    }

    // 2. Define inclined UCS at A=(0,0,5), B=(10,0,5), C=(0,10,15)
    s.execute("ucs.3point", &json!({
        "origin": [0.0, 0.0, 5.0],
        "x_axis": [10.0, 0.0, 5.0],
        "y_pt": [0.0, 10.0, 15.0]
    })).unwrap();

    let ucs = s.ucs();
    assert!(!ucs.is_world());

    // Line with endpoints of different local Z:
    // Local P1 = (0, 0, 2) -> World A + 2 * Z_ucs
    // Local P2 = (5, 5, 8) -> World A + 5*X_ucs + 5*Y_ucs + 8*Z_ucs
    s.cmdline("line").unwrap();
    s.cmdline("0,0,2").unwrap();
    s.cmdline("5,5,8").unwrap();
    s.cmdline("").unwrap();

    let doc = s.doc().unwrap();
    let l3 = doc.model.iter().nth(2).unwrap();
    if let cadcraft_doc::EntityKind::Line(c) = &l3.kind {
        let loc_a = ucs.to_local(c.a);
        let loc_b = ucs.to_local(c.b);
        assert!((loc_a.z - 2.0).abs() < 1e-5, "local Z1 preserved: {:?}", loc_a);
        assert!((loc_b.z - 8.0).abs() < 1e-5, "local Z2 preserved: {:?}", loc_b);
    } else {
        panic!("expected line 3");
    }

    // 3. Draw circle on inclined UCS: center 0,0 (which is A=(0,0,5)), radius 4.0
    s.cmdline("circle").unwrap();
    s.cmdline("0,0").unwrap();
    s.cmdline("4").unwrap();

    let doc = s.doc().unwrap();
    let c_ent = doc.model.iter().nth(3).unwrap();
    if let cadcraft_doc::EntityKind::Circle(c) = &c_ent.kind {
        assert!(c.center.near(Vec3::new(0.0, 0.0, 5.0), 1e-6), "circle center at A: {:?}", c.center);
        assert!((c.radius - 4.0).abs() < 1e-6);
        assert!(c_ent.common.extrusion.near(ucs.z_axis, 1e-6), "circle normal is ucs.z_axis");
    } else {
        panic!("expected circle");
    }

    // 4. Return to World:
    s.execute("ucs.world", &json!({})).unwrap();
    assert!(s.ucs().is_world());

    // Verify existing entities did not move or deform:
    let doc = s.doc().unwrap();
    let c_after = doc.model.iter().nth(3).unwrap();
    if let cadcraft_doc::EntityKind::Circle(c) = &c_after.kind {
        assert!(c.center.near(Vec3::new(0.0, 0.0, 5.0), 1e-6), "center preserved in World: {:?}", c.center);
        assert!(c_after.common.extrusion.near(ucs.z_axis, 1e-6), "normal preserved in World: {:?}", c_after.common.extrusion);
    }
}

#[test]
fn test_rectang_interactive_preview_on_oblique_ucs_matches_committed_geometry() {
    let mut s = Session::new();

    // Set oblique UCS
    let _ = s.execute(
        "ucs.3point",
        &json!({
            "origin": [0.0, 0.0, 5.0],
            "x_axis": [10.0, 0.0, 5.0],
            "y_pt": [0.0, 10.0, 15.0]
        }),
    ).expect("ucs.3point");

    let ucs = s.ucs();

    // Start RECTANG command
    s.start("rectang").expect("start rectang");

    // First point at local (0, 0)
    s.input(cadcraft_engine::Input::Point(cadcraft_geom::Vec2::new(0.0, 0.0))).expect("first corner");

    // Get preview at cursor (10, 20)
    let previews = s.preview(cadcraft_geom::Vec2::new(10.0, 20.0));
    assert_eq!(previews.len(), 1, "preview has 1 entity");

    let p_ent = &previews[0];
    assert!(p_ent.common.extrusion.near(ucs.z_axis, 1e-6), "preview extrusion must match UCS normal");

    let cadcraft_doc::EntityKind::LwPolyline(pl_prev) = &p_ent.kind else {
        panic!("expected lwpolyline in preview");
    };
    let prev_v3d = pl_prev.vertices_3d(p_ent.common.extrusion);

    // Confirm second corner at (10, 20)
    s.input(cadcraft_engine::Input::Point(cadcraft_geom::Vec2::new(10.0, 20.0))).expect("other corner");

    // Verify committed entity in doc
    let doc = s.doc().expect("doc");
    let last_ent = doc.model.iter().last().expect("last entity");
    assert!(last_ent.common.extrusion.near(ucs.z_axis, 1e-6));

    let cadcraft_doc::EntityKind::LwPolyline(pl_comm) = &last_ent.kind else {
        panic!("expected lwpolyline in committed entity");
    };
    let comm_v3d = pl_comm.vertices_3d(last_ent.common.extrusion);

    assert_eq!(prev_v3d.len(), comm_v3d.len());
    for i in 0..prev_v3d.len() {
        assert!(prev_v3d[i].near(comm_v3d[i], 1e-6), "vertex {} preview {:?} must match committed {:?}", i, prev_v3d[i], comm_v3d[i]);
    }
}

