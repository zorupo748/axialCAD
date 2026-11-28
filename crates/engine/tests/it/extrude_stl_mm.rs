//! End-to-end tests for the 3D modeling -> binary STL (mm) workflow:
//! Case A: Default 1 unit = 1 mm new drawing, RECTANG/PLINE/CIRCLE -> EXTRUDE/DMEXTRUDE -> STLOUT/EXPORT.
//! Case B: 40x30x5 plate with Ø 8 mm through-hole (SUBTRACT + STL tolerance validation & seam limitation).
//! Case C: Scale conversion across INSUNITS (mm vs cm) and unitless INSUNITS=0 prompt/refusal.
//! Case D: DXF & DWG roundtrip preservation of INSUNITS + Solid3d, plus repeated STL export idempotency.
//! Case E: Inclined UCS profile + extrusion + STL in mm.
//! Case F: Hostile/invalid inputs rejected atomically without partial solids or corrupt STL files.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use cadcraft_doc::EntityKind;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use cadcraft_engine::geom::Vec3;
use cadcraft_engine::{Input, Session};
use serde_json::json;

static SEQ: AtomicU64 = AtomicU64::new(1);

fn temp_stl_path(label: &str) -> PathBuf {
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let mut dir = std::env::temp_dir();
    dir.push("axialcad_it_stl");
    let _ = fs::create_dir_all(&dir);
    dir.push(format!("{label}_{}_{n}.stl", std::process::id()));
    let _ = fs::remove_file(&dir);
    dir
}

fn init_io_hooks() {
    set_io(IoHooks {
        read: |bytes, name| cadcraft_io::read(bytes, name).map_err(|e| e.to_string()),
        write: |doc, name| cadcraft_io::write(doc, name).map_err(|e| e.to_string()),
        plot: None,
    });
}

#[derive(Debug)]
struct StlReport {
    tri_count: usize,
    min: [f64; 3],
    max: [f64; 3],
    signed_volume: f64,
    open_edges: usize,
    non_manifold_edges: usize,
}

impl StlReport {
    fn size(&self) -> [f64; 3] {
        [self.max[0] - self.min[0], self.max[1] - self.min[1], self.max[2] - self.min[2]]
    }
}

fn read_vec3_le(chunk: &[u8]) -> [f64; 3] {
    let x = f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as f64;
    let y = f32::from_le_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]) as f64;
    let z = f32::from_le_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]) as f64;
    [x, y, z]
}

fn parse_and_validate_binary_stl(bytes: &[u8]) -> StlReport {
    assert!(bytes.len() >= 84, "STL binary too short: {} bytes", bytes.len());
    let tri_count = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
    assert!(tri_count >= 4, "expected non-empty solid mesh, got {tri_count} triangles");
    assert_eq!(bytes.len(), 84 + tri_count * 50, "STL size mismatch");

    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    let mut signed_vol6 = 0.0f64;
    let mut edges: HashMap<([i64; 3], [i64; 3]), usize> = HashMap::new();

    let q = |v: [f64; 3]| -> [i64; 3] {
        let scale = 1e4;
        [(v[0] * scale).round() as i64, (v[1] * scale).round() as i64, (v[2] * scale).round() as i64]
    };

    for i in 0..tri_count {
        let rec = &bytes[84 + i * 50..84 + (i + 1) * 50];
        let n = read_vec3_le(&rec[0..12]);
        let v0 = read_vec3_le(&rec[12..24]);
        let v1 = read_vec3_le(&rec[24..36]);
        let v2 = read_vec3_le(&rec[36..48]);

        for arr in [n, v0, v1, v2] {
            assert!(arr.iter().all(|c| c.is_finite()), "non-finite STL coordinate: {arr:?}");
        }
        for v in [v0, v1, v2] {
            for k in 0..3 {
                min[k] = min[k].min(v[k]);
                max[k] = max[k].max(v[k]);
            }
        }

        let p0 = Vec3::new(v0[0], v0[1], v0[2]);
        let p1 = Vec3::new(v1[0], v1[1], v1[2]);
        let p2 = Vec3::new(v2[0], v2[1], v2[2]);
        let cross = (p1 - p0).cross(p2 - p0);
        let len = cross.len();
        assert!(len > 1e-10, "degenerate triangle #{i}: area2={len}");
        let fn_vec = cross / len;
        let stored_n = Vec3::new(n[0], n[1], n[2]);
        assert!(fn_vec.dot(stored_n) > 0.99, "normal mismatch on triangle #{i}: computed={fn_vec:?}, stored={stored_n:?}");

        signed_vol6 += p0.dot(p1.cross(p2));

        let q0 = q(v0);
        let q1 = q(v1);
        let q2 = q(v2);
        *edges.entry((q0, q1)).or_insert(0) += 1;
        *edges.entry((q1, q2)).or_insert(0) += 1;
        *edges.entry((q2, q0)).or_insert(0) += 1;
    }

    let mut open_edges = 0usize;
    let mut non_manifold_edges = 0usize;
    for (&(a, b), &cnt) in &edges {
        if cnt != 1 {
            non_manifold_edges += cnt.saturating_sub(1);
        }
        if !edges.contains_key(&(b, a)) {
            open_edges += 1;
        }
    }

    StlReport {
        tri_count,
        min,
        max,
        signed_volume: signed_vol6 / 6.0,
        open_edges,
        non_manifold_edges,
    }
}

fn solid_handles(s: &Session) -> Vec<String> {
    s.doc()
        .unwrap()
        .model
        .iter()
        .filter_map(|e| matches!(e.kind, EntityKind::Solid3d(_)).then(|| e.handle.hex()))
        .collect()
}

fn last_solid_handle(s: &Session) -> String {
    solid_handles(s).into_iter().last().expect("expected at least one Solid3d")
}

#[test]
fn case_a_default_mm_rectang_extrude_and_stl_pipeline() {
    let mut s = Session::new();
    let doc = s.doc().unwrap();
    assert_eq!(doc.header.i64("INSUNITS", 0), 4, "new drawings default to Millimeters (INSUNITS=4)");
    assert_eq!(doc.header.i64("MEASUREMENT", -1), 1, "new drawings default to Metric (MEASUREMENT=1)");
    assert!(s.settings.measureinit, "MEASUREINIT profile setting defaults to 1");

    // 1) Interactive RECTANG + EXTRUDE + STLOUT -> 40x30x5 mm binary STL.
    s.cmdline("RECTANG 0,0 40,30").unwrap();
    let rect_h = cadcraft_doc::Handle::parse_hex(&doc_last_hex(&s)).unwrap();
    s.cmdline("EXTRUDE").unwrap();
    s.input(Input::Pick(vec![rect_h])).unwrap();
    s.cmdline("").unwrap();
    s.cmdline("5").unwrap();
    assert!(s.running.is_none(), "EXTRUDE must finish cleanly, prompt: {}", s.prompt_text());

    let solid_h = last_solid_handle(&s);
    let m = s.execute("query.metrics", &json!({ "handle": solid_h })).unwrap();
    assert!((m["volume"].as_f64().unwrap() - 6000.0).abs() < 1e-3, "metrics={m}");
    assert_eq!(m["boundaries"], 1);

    let stl_path = temp_stl_path("case_a_rect");
    s.cmdline("STLOUT").unwrap();
    s.input(Input::Pick(vec![cadcraft_doc::Handle::parse_hex(&solid_h).unwrap()])).unwrap();
    s.cmdline("").unwrap();
    s.cmdline(&stl_path.to_string_lossy()).unwrap();
    assert!(s.running.is_none(), "STLOUT must finish cleanly, prompt: {}", s.prompt_text());

    let bytes = fs::read(&stl_path).expect("STL file written");
    let rep = parse_and_validate_binary_stl(&bytes);
    assert!(rep.tri_count >= 12);
    assert_eq!(rep.open_edges, 0);
    assert_eq!(rep.non_manifold_edges, 0);
    assert!((rep.signed_volume - 6000.0).abs() < 1e-2, "rep={rep:?}");
    let sz = rep.size();
    assert!((sz[0] - 40.0).abs() < 1e-4 && (sz[1] - 30.0).abs() < 1e-4 && (sz[2] - 5.0).abs() < 1e-4, "size={sz:?}");
    let _ = fs::remove_file(&stl_path);

    // 2) CW PLINE + DMEXTRUDE + EXPORT .stl parity check.
    s.cmdline("PLINE 0,0 0,30 40,30 40,0 c").unwrap();
    let cw_h = doc_last_hex(&s);
    let dm = s.execute("dmextrude", &json!({ "handles": [cw_h], "height": 5 })).unwrap();
    let dm_solid = dm["handle"].as_str().unwrap().to_string();
    let dm_metrics = s.execute("query.metrics", &json!({ "handle": dm_solid })).unwrap();
    assert!((dm_metrics["volume"].as_f64().unwrap() - 6000.0).abs() < 1e-3);

    let exp_path = temp_stl_path("case_a_export");
    let exp_res = s.execute("export", &json!({ "format": "stl", "handles": [dm_solid], "path": exp_path.to_string_lossy() })).unwrap();
    assert_eq!(exp_res["unit"], "Millimeters");
    let exp_rep = parse_and_validate_binary_stl(&fs::read(&exp_path).unwrap());
    assert_eq!((exp_rep.open_edges, exp_rep.non_manifold_edges), (0, 0));
    assert!((exp_rep.signed_volume - 6000.0).abs() < 1e-2);
    let _ = fs::remove_file(&exp_path);

    // 3) Extruded Circle -> exact Cylinder recipe.
    s.cmdline("CIRCLE 20,15 4").unwrap();
    let circ_h = doc_last_hex(&s);
    let cyl = s.execute("extrude", &json!({ "profile": circ_h, "height": 5 })).unwrap();
    let cyl_h = cyl["handle"].as_str().unwrap();
    let cyl_stl = temp_stl_path("case_a_circle_cyl");
    s.execute("stlout", &json!({ "handles": [cyl_h], "path": cyl_stl.to_string_lossy() })).unwrap();
    let cyl_rep = parse_and_validate_binary_stl(&fs::read(&cyl_stl).unwrap());
    assert_eq!((cyl_rep.open_edges, cyl_rep.non_manifold_edges), (0, 0));
    assert!((cyl_rep.signed_volume - std::f64::consts::PI * 16.0 * 5.0).abs() < 5.0, "cyl vol={}", cyl_rep.signed_volume);
    let _ = fs::remove_file(&cyl_stl);
}

#[test]
fn case_b_holed_plate_through_hole_boolean_and_stl_limitation() {
    let mut s = Session::new();
    s.cmdline("RECTANG 0,0 40,30").unwrap();
    let rect_h = doc_last_hex(&s);
    let plate_h = s.execute("extrude", &json!({ "profile": rect_h, "height": 5 })).unwrap()["handle"]
        .as_str()
        .unwrap()
        .to_string();

    // Create oversized through-hole cutter cylinder at plate center (20, 15).
    let hole_h = s
        .execute(
            "cylinder",
            &json!({ "center": [20.0, 15.0, -1.0], "radius": 4.0, "height": 7.0 }),
        )
        .unwrap()["handle"]
        .as_str()
        .unwrap()
        .to_string();

    let sub = s.execute("subtract", &json!({ "from": [plate_h], "remove": [hole_h] })).unwrap();
    let res_h = sub["handle"].as_str().unwrap().to_string();
    let metrics = s.execute("query.metrics", &json!({ "handle": res_h })).unwrap();
    let expected_vol = 40.0 * 30.0 * 5.0 - std::f64::consts::PI * 4.0 * 4.0 * 5.0;
    assert!((metrics["volume"].as_f64().unwrap() - expected_vol).abs() < 15.0, "metrics={metrics}");
    assert_eq!(metrics["boundaries"], 1);

    // At coarser tessellation (tolerance_mm = 0.25), truck produces a closed manifold STL.
    let valid_stl = temp_stl_path("case_b_valid_025");
    s.execute(
        "stlout",
        &json!({ "handles": [res_h], "path": valid_stl.to_string_lossy(), "tolerance_mm": 0.25 }),
    )
    .unwrap();
    let rep = parse_and_validate_binary_stl(&fs::read(&valid_stl).unwrap());
    assert_eq!((rep.open_edges, rep.non_manifold_edges), (0, 0));
    let sz = rep.size();
    assert!((sz[0] - 40.0).abs() < 1e-4 && (sz[1] - 30.0).abs() < 1e-4 && (sz[2] - 5.0).abs() < 1e-4, "size={sz:?}");
    assert!((rep.signed_volume - expected_vol).abs() < 20.0, "vol={}", rep.signed_volume);
    let _ = fs::remove_file(&valid_stl);

    // At fine boolean trim tolerance (0.05 mm), meshing now succeeds with zero defect boundaries:
    let fine_stl = temp_stl_path("case_b_fine_success");
    let fine_res = s
        .execute(
            "stlout",
            &json!({ "handles": [res_h], "path": fine_stl.to_string_lossy(), "tolerance_mm": 0.05 }),
        )
        .unwrap();
    assert_eq!(fine_res["status"], "ok");
    assert_eq!(fine_res["topology"]["open_edges"], 0);
    assert_eq!(fine_res["topology"]["non_manifold_edges"], 0);
    assert!(fine_res["triangles_count"].as_u64().unwrap() >= 700);
    let rep_fine = parse_and_validate_binary_stl(&fs::read(&fine_stl).unwrap());
    assert_eq!((rep_fine.open_edges, rep_fine.non_manifold_edges), (0, 0));
    assert!((rep_fine.signed_volume - expected_vol).abs() < 10.0, "vol={}", rep_fine.signed_volume);
    let _ = fs::remove_file(&fine_stl);
}

#[test]
fn case_c_mm_vs_cm_scale_and_unitless_rejection_and_prompt() {
    // 1) Compare 40x30x5 mm vs 4x3x0.5 cm -> identical physical mm dimensions in STL.
    let mut s_mm = Session::new();
    let b_mm = s_mm.execute("solid.box", &json!({ "origin": [0, 0, 0], "size": [40, 30, 5] })).unwrap()["handle"]
        .as_str()
        .unwrap()
        .to_string();
    let stl_mm = temp_stl_path("case_c_mm");
    s_mm.execute("stlout", &json!({ "handles": [b_mm], "path": stl_mm.to_string_lossy() })).unwrap();
    let rep_mm = parse_and_validate_binary_stl(&fs::read(&stl_mm).unwrap());

    let mut s_cm = Session::new();
    s_cm.execute("setvar", &json!({ "name": "INSUNITS", "value": 5 })).unwrap(); // Centimeters
    let b_cm = s_cm.execute("solid.box", &json!({ "origin": [0, 0, 0], "size": [4, 3, 0.5] })).unwrap()["handle"]
        .as_str()
        .unwrap()
        .to_string();
    let stl_cm = temp_stl_path("case_c_cm");
    s_cm.execute("stlout", &json!({ "handles": [b_cm], "path": stl_cm.to_string_lossy() })).unwrap();
    let rep_cm = parse_and_validate_binary_stl(&fs::read(&stl_cm).unwrap());

    assert_eq!(rep_mm.open_edges, 0);
    assert_eq!(rep_cm.open_edges, 0);
    for k in 0..3 {
        assert!((rep_mm.size()[k] - rep_cm.size()[k]).abs() < 1e-4, "axis {k}: mm={:?} cm={:?}", rep_mm.size(), rep_cm.size());
    }
    assert!((rep_mm.signed_volume - 6000.0).abs() < 1e-2);
    assert!((rep_cm.signed_volume - 6000.0).abs() < 1e-2);
    let _ = fs::remove_file(&stl_mm);
    let _ = fs::remove_file(&stl_cm);

    // 2) Unitless drawing (INSUNITS=0): refuse guessing automatically, prompt unit interactively, optionally persist.
    let mut s_unitless = Session::new();
    s_unitless.execute("setvar", &json!({ "name": "INSUNITS", "value": 0 })).unwrap();
    let b_u = s_unitless.execute("solid.box", &json!({ "origin": [0, 0, 0], "size": [4, 3, 0.5] })).unwrap()["handle"]
        .as_str()
        .unwrap()
        .to_string();

    let blocked_stl = temp_stl_path("case_c_unitless_blocked");
    let err = s_unitless
        .execute("stlout", &json!({ "handles": [b_u], "path": blocked_stl.to_string_lossy() }))
        .unwrap_err()
        .to_string();
    assert!(err.contains("INSUNITS=0") || err.contains("no known physical units"), "err={err}");
    assert!(!blocked_stl.exists(), "unitless auto-export must not guess or write");

    // Interactive STLOUT asks unit, asks whether to save INSUNITS, converts cm -> mm!
    let prompted_stl = temp_stl_path("case_c_unitless_prompted");
    s_unitless.cmdline("STLOUT").unwrap();
    s_unitless.input(Input::Pick(vec![cadcraft_doc::Handle::parse_hex(&b_u).unwrap()])).unwrap();
    s_unitless.cmdline("").unwrap();
    assert!(s_unitless.prompt_text().contains("Millimeters/Centimeters"), "prompt={}", s_unitless.prompt_text());
    s_unitless.cmdline("Centimeters").unwrap();
    assert!(s_unitless.prompt_text().contains("INSUNITS"), "prompt={}", s_unitless.prompt_text());
    s_unitless.cmdline("Yes").unwrap();
    s_unitless.cmdline(&prompted_stl.to_string_lossy()).unwrap();
    assert!(s_unitless.running.is_none());
    assert_eq!(s_unitless.doc().unwrap().header.i64("INSUNITS", 0), 5);
    let rep_prompted = parse_and_validate_binary_stl(&fs::read(&prompted_stl).unwrap());
    assert!((rep_prompted.signed_volume - 6000.0).abs() < 1e-2);
    let _ = fs::remove_file(&prompted_stl);
}

#[test]
fn case_d_dxf_and_dwg_roundtrip_and_repeated_stl_idempotency() {
    init_io_hooks();

    let mut s = Session::new();
    s.cmdline("RECTANG 0,0 40,30").unwrap();
    let rect_h = doc_last_hex(&s);
    let solid_h = s.execute("extrude", &json!({ "profile": rect_h, "height": 5 })).unwrap()["handle"]
        .as_str()
        .unwrap()
        .to_string();

    // Repeated STL exports must not mutate drawing coordinates, INSUNITS, or subsequent exports.
    for round in 1..=3 {
        let path = temp_stl_path(&format!("case_d_repeat_{round}"));
        let res = s.execute("stlout", &json!({ "handles": [solid_h], "path": path.to_string_lossy() })).unwrap();
        assert_eq!(res["unit"], "Millimeters");
        let rep = parse_and_validate_binary_stl(&fs::read(&path).unwrap());
        assert!((rep.signed_volume - 6000.0).abs() < 1e-2);
        let sz = rep.size();
        assert!((sz[0] - 40.0).abs() < 1e-4 && (sz[1] - 30.0).abs() < 1e-4 && (sz[2] - 5.0).abs() < 1e-4);
        assert_eq!(s.doc().unwrap().header.i64("INSUNITS", 0), 4);
        let m = s.execute("query.metrics", &json!({ "handle": solid_h })).unwrap();
        assert_eq!(m["bbox"]["max"], json!([40.0, 30.0, 5.0]));
        let _ = fs::remove_file(&path);
    }

    // DXF roundtrip preserves INSUNITS=4 & Solid3d dimensions + STL export still valid.
    let doc = s.doc().unwrap();
    let dxf_bytes = cadcraft_io::write(doc, "model.dxf").expect("write dxf");
    let dxf_doc = cadcraft_io::read(&dxf_bytes, "model.dxf").expect("read dxf");
    assert_eq!(dxf_doc.header.i64("INSUNITS", 0), 4);
    assert_eq!(dxf_doc.header.i64("MEASUREMENT", -1), 1);

    let mut s_dxf = Session::new();
    s_dxf.open_drawing(dxf_doc, "model.dxf", Some("model.dxf".into()));
    assert!(!solid_handles(&s_dxf).is_empty(), "DXF reload missing Solid3d: entities={:?}", s_dxf.doc().unwrap().model.len());
    let h_dxf = last_solid_handle(&s_dxf);
    let stl_dxf = temp_stl_path("case_d_after_dxf");
    s_dxf.execute("stlout", &json!({ "handles": [h_dxf], "path": stl_dxf.to_string_lossy() })).unwrap();
    let rep_dxf = parse_and_validate_binary_stl(&fs::read(&stl_dxf).unwrap());
    assert!((rep_dxf.signed_volume - 6000.0).abs() < 1e-2);
    let _ = fs::remove_file(&stl_dxf);

    // DWG roundtrip preserves INSUNITS=4 & Solid3d dimensions + STL export still valid.
    let dwg_bytes = cadcraft_io::write(s.doc().unwrap(), "model.dwg").expect("write dwg");
    let dwg_doc = cadcraft_io::read(&dwg_bytes, "model.dwg").expect("read dwg");
    assert_eq!(dwg_doc.header.i64("INSUNITS", 0), 4);
    let dwg_notes = dwg_doc.read_notes.clone();
    let dwg_kinds: Vec<String> = dwg_doc.model.iter().map(|e| format!("{:?}", std::mem::discriminant(&e.kind))).collect();

    let mut s_dwg = Session::new();
    s_dwg.open_drawing(dwg_doc, "model.dwg", Some("model.dwg".into()));
    assert!(
        !solid_handles(&s_dwg).is_empty(),
        "DWG reload missing Solid3d: entities={}, kinds={dwg_kinds:?}, notes={dwg_notes:?}",
        s_dwg.doc().unwrap().model.len()
    );
    let h_dwg = last_solid_handle(&s_dwg);
    let m_dwg = s_dwg.execute("query.metrics", &json!({ "handle": h_dwg })).unwrap();
    assert!((m_dwg["volume"].as_f64().unwrap() - 6000.0).abs() < 1e-3, "dwg metrics={m_dwg}");
    let stl_dwg = temp_stl_path("case_d_after_dwg");
    s_dwg.execute("stlout", &json!({ "handles": [h_dwg], "path": stl_dwg.to_string_lossy() })).unwrap();
    let rep_dwg = parse_and_validate_binary_stl(&fs::read(&stl_dwg).unwrap());
    assert_eq!((rep_dwg.open_edges, rep_dwg.non_manifold_edges), (0, 0));
    assert!((rep_dwg.signed_volume - 6000.0).abs() < 1e-2);
    let _ = fs::remove_file(&stl_dwg);
}

#[test]
fn case_e_inclined_ucs_extrusion_and_stl() {
    let mut s = Session::new();
    // Tilt UCS 60 deg around X: normal = (0, -sqrt(3)/2, 0.5).
    s.cmdline("ucs 3 0,0,0 1,0,0 0,0.5,0.8660254037844386").unwrap();
    s.cmdline("RECTANG 0,0 40,30").unwrap();
    assert!(!doc_last_hex(&s).is_empty());

    // Extrude along inclined UCS +Z via prompt height 5 (pick at 0,0 on the rectangle edge).
    s.cmdline("EXTRUDE").unwrap();
    s.cmdline("0,0").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("5").unwrap();
    assert!(s.running.is_none(), "prompt={}", s.prompt_text());

    let solid_h = last_solid_handle(&s);
    let m = s.execute("query.metrics", &json!({ "handle": solid_h })).unwrap();
    assert!((m["volume"].as_f64().unwrap() - 6000.0).abs() < 1e-3, "metrics={m}");
    assert_eq!(m["boundaries"], 1);

    let stl_path = temp_stl_path("case_e_inclined_ucs");
    s.execute("stlout", &json!({ "handles": [solid_h], "path": stl_path.to_string_lossy() })).unwrap();
    let rep = parse_and_validate_binary_stl(&fs::read(&stl_path).unwrap());
    assert_eq!((rep.open_edges, rep.non_manifold_edges), (0, 0));
    assert!((rep.signed_volume - 6000.0).abs() < 1e-2, "vol={}", rep.signed_volume);
    // Z extent reflects the 60-degree tilted 30x5 face in 3D space (~ 30*sin(60) + 5*cos(60) = 28.48 mm).
    let z_span = rep.max[2] - rep.min[2];
    assert!(z_span > 25.0, "expected true 3D inclined extent along Z, got z_span={z_span}");
    let _ = fs::remove_file(&stl_path);
}

#[test]
fn case_f_rejections_and_atomicity() {
    let mut s = Session::new();

    // Open polyline
    s.cmdline("PLINE 0,0 10,0 10,10").unwrap();
    let open_h = doc_last_hex(&s);
    assert!(s.execute("extrude", &json!({ "profile": open_h, "height": 5 })).is_err());

    // Self-intersecting bowtie polyline
    s.cmdline("PLINE 0,0 10,10 10,0 0,10 c").unwrap();
    let bowtie_h = doc_last_hex(&s);
    assert!(s.execute("extrude", &json!({ "profile": bowtie_h, "height": 5 })).is_err());

    // Closed valid polyline + invalid parameters (zero height, coplanar direction, taper, path)
    s.cmdline("RECTANG 0,0 20,10").unwrap();
    let rect_h = doc_last_hex(&s);
    for bad_params in [
        json!({ "profile": rect_h, "height": 0 }),
        json!({ "profile": rect_h, "direction": [0, 0, 0] }),
        json!({ "profile": rect_h, "direction": [5, 0, 0] }),
        json!({ "profile": rect_h, "height": 5, "taper": 10 }),
        json!({ "profile": rect_h, "height": 5, "path": open_h }),
    ] {
        assert!(s.execute("extrude", &bad_params).is_err(), "expected rejection for {bad_params}");
    }

    // Oblique circle extrusion rejected cleanly
    s.cmdline("CIRCLE 50,50 4").unwrap();
    let circ_h = doc_last_hex(&s);
    assert!(s.execute("extrude", &json!({ "profile": circ_h, "direction": [3, 0, 5] })).is_err());

    // Atomic multi-handle failure (one valid + one open -> NO partial Solid3d added)
    assert!(s.execute("extrude", &json!({ "handles": [rect_h, open_h], "height": 5 })).is_err());
    assert!(solid_handles(&s).is_empty(), "failed atomic extrusion must leave 0 solids");

    // Hostile STL inputs & empty/non-solid selection leave zero files on disk
    let line_h = s.execute("line", &json!({ "points": [[0, 0], [10, 0]] })).unwrap()["handles"][0]
        .as_str()
        .unwrap()
        .to_string();
    let no_solid_stl = temp_stl_path("case_f_no_solid");
    assert!(s.execute("stlout", &json!({ "handles": [line_h], "path": no_solid_stl.to_string_lossy() })).is_err());
    assert!(!no_solid_stl.exists());

    // Create 1 valid solid + verify non-solids are skipped if selection contains mixed entities,
    // and invalid unit/tolerance write nothing.
    let valid_h = s.execute("extrude", &json!({ "profile": rect_h, "height": 5 })).unwrap()["handle"]
        .as_str()
        .unwrap()
        .to_string();

    let bad_tol_stl = temp_stl_path("case_f_bad_tol");
    assert!(
        s.execute("stlout", &json!({ "handles": [valid_h], "path": bad_tol_stl.to_string_lossy(), "tolerance_mm": -1.0 }))
            .is_err()
    );
    assert!(!bad_tol_stl.exists());

    let bad_unit_stl = temp_stl_path("case_f_bad_unit");
    assert!(
        s.execute("stlout", &json!({ "handles": [valid_h], "path": bad_unit_stl.to_string_lossy(), "unit": "parsecs" }))
            .is_err()
    );
    assert!(!bad_unit_stl.exists());

    // Interactive STLOUT with mixed selection (Line + Solid3d) skips non-solid and exports the solid.
    let mixed_stl = temp_stl_path("case_f_mixed_sel");
    s.cmdline("STLOUT").unwrap();
    s.input(Input::Pick(vec![
        cadcraft_doc::Handle::parse_hex(&line_h).unwrap(),
        cadcraft_doc::Handle::parse_hex(&valid_h).unwrap(),
    ]))
    .unwrap();
    s.cmdline("").unwrap();
    s.cmdline(&mixed_stl.to_string_lossy()).unwrap();
    let mixed_rep = parse_and_validate_binary_stl(&fs::read(&mixed_stl).unwrap());
    assert_eq!((mixed_rep.open_edges, mixed_rep.non_manifold_edges), (0, 0));
    assert!((mixed_rep.signed_volume - 1000.0).abs() < 1e-2);
    let _ = fs::remove_file(&mixed_stl);
}

#[test]
fn extrude_switches_to_conceptual_and_preserves_wireframe_toggle_and_cancel() {
    let mut s = Session::new();
    assert_eq!(s.settings.vscurrent, "2dWireframe");

    // A: RECTANG 40x30 -> EXTRUDE 5 switches from 2dWireframe to Conceptual without changing camera
    s.cmdline("RECTANG 0,0 40,30").unwrap();
    let view_before = s.state().unwrap().view();
    s.cmdline("EXTRUDE ALL  5").unwrap();
    assert_eq!(s.settings.vscurrent, "Conceptual");
    assert_eq!(s.state().unwrap().view(), view_before, "EXTRUDE must not alter camera/view");

    // C: VS W returns to 2dWireframe
    s.cmdline("VS W").unwrap();
    assert_eq!(s.settings.vscurrent, "2dWireframe");

    // B: Cancelled or failed EXTRUDE preserves 2dWireframe
    s.cmdline("LINE 0,0 10,0 ").unwrap();
    let line_h = s.doc().unwrap().model.iter().last().unwrap().handle;
    assert!(s.execute("extrude", &json!({ "profile": line_h.hex(), "height": 5 })).is_err());
    assert_eq!(s.settings.vscurrent, "2dWireframe", "failed EXTRUDE must keep previous visual style");

    s.cmdline("RECTANG 0,0 10,10").unwrap();
    let rect2_h = s.doc().unwrap().model.iter().last().unwrap().handle;
    s.cmdline("EXTRUDE").unwrap();
    s.input(Input::Pick(vec![rect2_h])).unwrap();
    s.cmdline("").unwrap();
    s.input(Input::Cancel).unwrap();
    assert_eq!(s.settings.vscurrent, "2dWireframe", "cancelled EXTRUDE must keep previous visual style");

    // D: EXT alias behaves identically to EXTRUDE
    s.cmdline("EXT").unwrap();
    s.input(Input::Pick(vec![rect2_h])).unwrap();
    s.cmdline("").unwrap();
    s.cmdline("5").unwrap();
    assert_eq!(s.settings.vscurrent, "Conceptual", "EXT alias must activate Conceptual");
}

fn doc_last_hex(s: &Session) -> String {
    s.doc().unwrap().model.last().unwrap().handle.hex()
}
