//! Tests for User-Defined Hatch (hatch.user, user-defined pattern lines,
//! numeric validations, rendering, DXF roundtrip, and coexistence with predefined patterns).

use cadcraft_doc::{EntityKind, Hatch};
use cadcraft_engine::Session;
use serde_json::json;

fn first_hatch(s: &Session) -> Hatch {
    let d = s.doc().unwrap();
    let hatches: Vec<Hatch> = d.model.iter().filter_map(|e| if let EntityKind::Hatch(h) = &e.kind { Some(h.clone()) } else { None }).collect();
    assert!(!hatches.is_empty(), "expected at least one hatch in document");
    hatches[0].clone()
}

#[test]
fn custom_angle_and_spacing_user_defined_hatch() {
    let mut s = Session::new();
    s.execute("rectang", &json!({ "p1": [0, 0], "p2": [20, 20] })).unwrap();

    // 1. Single line family with custom spacing and angle
    let res = s.execute(
        "hatch.user",
        &json!({
            "points": [[10, 10]],
            "spacing": 2.5,
            "angle": 30.0,
            "double": false
        }),
    );
    assert!(res.is_ok(), "hatch.user should succeed: {:?}", res);

    let h = first_hatch(&s);
    assert_eq!(h.pattern, "_U");
    assert!(!h.solid);
    assert_eq!(h.pattern_lines.len(), 1, "Single line family when double is false");
    let line0 = &h.pattern_lines[0];
    assert_eq!(line0.angle, 0.0);
    assert_eq!(line0.origin, (0.0, 0.0));
    assert_eq!(line0.delta, (0.0, 2.5));
    assert!(line0.dashes.is_empty(), "Continuous pattern lines");
    assert!((h.angle - 30.0f64.to_radians()).abs() < 1e-9);

    // 2. Cross-hatch (double = true) with custom spacing and angle
    let mut s2 = Session::new();
    s2.execute("rectang", &json!({ "p1": [0, 0], "p2": [20, 20] })).unwrap();
    s2.execute(
        "hatch.user",
        &json!({
            "points": [[10, 10]],
            "spacing": 3.75,
            "angle": 45.0,
            "double": true
        }),
    )
    .unwrap();

    let h2 = first_hatch(&s2);
    assert_eq!(h2.pattern, "_U");
    assert_eq!(h2.pattern_lines.len(), 2, "Cross-hatch has 2 line families");
    assert_eq!(h2.pattern_lines[0].angle, 0.0);
    assert_eq!(h2.pattern_lines[0].delta, (0.0, 3.75));
    assert_eq!(h2.pattern_lines[1].angle, 90.0);
    assert_eq!(h2.pattern_lines[1].delta, (0.0, 3.75));
    assert!((h2.angle - 45.0f64.to_radians()).abs() < 1e-9);
}

#[test]
fn valid_closed_boundary_and_rejection_of_invalid_boundary() {
    let mut s = Session::new();

    // Valid closed boundary: circle
    s.execute("circle", &json!({ "center": [0, 0], "radius": 10.0 })).unwrap();
    let res = s.execute("hatch.user", &json!({ "points": [[0, 0]], "spacing": 1.0 }));
    assert!(res.is_ok(), "Closed circle is a valid boundary: {:?}", res);

    // Unclosed boundary: single line
    let mut s2 = Session::new();
    let line_res = s2.execute("line", &json!({ "points": [[0, 0], [10, 10]] })).unwrap();
    let line_handle = line_res["handles"][0].as_str().unwrap();

    let unclosed_res = s2.execute("hatch.user", &json!({ "handles": [line_handle], "spacing": 1.0 }));
    assert!(unclosed_res.is_err(), "Unclosed line boundary must be rejected");

    // Picking outside any boundary
    let outside_res = s2.execute("hatch.user", &json!({ "points": [[500, 500]], "spacing": 1.0 }));
    assert!(outside_res.is_err(), "Picking point outside boundary must be rejected");
}

#[test]
fn extreme_and_invalid_values_are_rejected() {
    let mut s = Session::new();
    s.execute("rectang", &json!({ "p1": [0, 0], "p2": [10, 10] })).unwrap();

    // Spacing <= 0
    let res_zero = s.execute("hatch.user", &json!({ "points": [[5, 5]], "spacing": 0.0 }));
    assert!(res_zero.is_err(), "Zero spacing must be rejected");

    let res_neg = s.execute("hatch.user", &json!({ "points": [[5, 5]], "spacing": -1.5 }));
    assert!(res_neg.is_err(), "Negative spacing must be rejected");

    // Spacing too small (< 1e-4) to prevent UI freezing
    let res_tiny = s.execute("hatch.user", &json!({ "points": [[5, 5]], "spacing": 0.00001 }));
    assert!(res_tiny.is_err(), "Sub-0.0001 spacing must be rejected");

    // Spacing non-numeric
    let res_str = s.execute("hatch.user", &json!({ "points": [[5, 5]], "spacing": "not-a-number" }));
    assert!(res_str.is_err(), "Non-numeric spacing must be rejected");

    // Scale zero or negative
    let res_sc0 = s.execute("hatch.user", &json!({ "points": [[5, 5]], "spacing": 1.0, "scale": 0 }));
    assert!(res_sc0.is_err(), "Zero scale must be rejected");

    let res_sc_neg = s.execute("hatch.user", &json!({ "points": [[5, 5]], "spacing": 1.0, "scale": -2.0 }));
    assert!(res_sc_neg.is_err(), "Negative scale must be rejected");
}

#[test]
fn pattern_rendering_and_budget_protection() {
    let mut s = Session::new();
    s.execute("rectang", &json!({ "p1": [0, 0], "p2": [20, 20] })).unwrap();
    s.execute(
        "hatch.user",
        &json!({
            "points": [[10, 10]],
            "spacing": 2.0,
            "angle": 45.0,
            "double": true
        }),
    )
    .unwrap();

    let d = s.doc().unwrap();
    let h = first_hatch(&s);
    let loops = cadcraft_render::fill_loops(&h, 1e-3);
    assert!(!loops.is_empty(), "Loops must tessellate for rendering");

    let lines = cadcraft_render::pattern_lines(d, &h, &loops, 0.0, None);
    assert!(!lines.is_empty(), "Renderer must generate pattern line segments: got {}", lines.len());

    // Verify bounds of rendered lines stay within or on the boundary
    for seg in &lines {
        for pt in seg {
            assert!(pt.x >= -0.1 && pt.x <= 20.1, "Rendered line pt.x {} in boundary", pt.x);
            assert!(pt.y >= -0.1 && pt.y <= 20.1, "Rendered line pt.y {} in boundary", pt.y);
        }
    }
}

#[test]
fn dxf_save_and_reopen_preserves_user_defined_hatch() {
    let mut s = Session::new();
    s.execute("rectang", &json!({ "p1": [0, 0], "p2": [10, 10] })).unwrap();
    s.execute(
        "hatch.user",
        &json!({
            "points": [[5, 5]],
            "spacing": 2.0,
            "angle": 60.0,
            "double": true
        }),
    )
    .unwrap();

    let doc = s.doc().unwrap();
    let dxf_text = cadcraft_io::write_dxf(doc);

    // Verify group 76 is 0 (User-defined in DXF standard)
    assert!(dxf_text.replace("\r\n", "\n").contains(" 76\n0\n"), "DXF must specify group 76 = 0 for user-defined hatch: {}", dxf_text);

    // Reopen from DXF
    let reloaded = cadcraft_io::read_dxf(dxf_text.as_bytes()).unwrap();
    let reloaded_hatches: Vec<Hatch> =
        reloaded.model.iter().filter_map(|e| if let EntityKind::Hatch(h) = &e.kind { Some(h.clone()) } else { None }).collect();

    assert_eq!(reloaded_hatches.len(), 1, "Hatch preserved across DXF roundtrip");
    let rh = &reloaded_hatches[0];
    assert_eq!(rh.pattern, "_U");
    assert_eq!(rh.pattern_lines.len(), 2, "Pattern lines preserved across DXF");
    assert!((rh.pattern_lines[0].delta.1 - 2.0).abs() < 1e-4, "Line family 0 spacing preserved");
    assert!((rh.pattern_lines[1].delta.1 - 2.0).abs() < 1e-4, "Line family 1 spacing preserved");
    assert!((rh.angle - 60.0f64.to_radians()).abs() < 1e-4, "Angle preserved across DXF");
}

#[test]
fn predefined_patterns_remain_intact_and_compatible() {
    let mut s = Session::new();
    // Predefined hatch: ANSI31
    s.execute("rectang", &json!({ "p1": [0, 0], "p2": [10, 10] })).unwrap();
    s.execute("hatch", &json!({ "pattern": "ANSI31", "points": [[5, 5]], "scale": 1.5, "angle": 15.0 })).unwrap();

    // User-defined hatch in same session:
    s.execute("rectang", &json!({ "p1": [20, 0], "p2": [30, 10] })).unwrap();
    s.execute("hatch.user", &json!({ "points": [[25, 5]], "spacing": 1.25, "angle": 0.0 })).unwrap();

    let doc = s.doc().unwrap();
    let hatches: Vec<Hatch> = doc.model.iter().filter_map(|e| if let EntityKind::Hatch(h) = &e.kind { Some(h.clone()) } else { None }).collect();

    assert_eq!(hatches.len(), 2);
    assert!(hatches.iter().any(|h| h.pattern == "ANSI31"));
    assert!(hatches.iter().any(|h| h.pattern == "_U"));

    // DXF roundtrip preserves both
    let dxf_text = cadcraft_io::write_dxf(doc);
    let reloaded = cadcraft_io::read_dxf(dxf_text.as_bytes()).unwrap();
    let reloaded_hatches: Vec<Hatch> =
        reloaded.model.iter().filter_map(|e| if let EntityKind::Hatch(h) = &e.kind { Some(h.clone()) } else { None }).collect();

    assert_eq!(reloaded_hatches.len(), 2);
    assert!(reloaded_hatches.iter().any(|h| h.pattern == "ANSI31"));
    assert!(reloaded_hatches.iter().any(|h| h.pattern == "_U"));
}

#[test]
fn four_cases_user_defined_hatch_geometry() {
    // Large closed rectangle 100x100
    // Test Case A: angle 0°, spacing 5, scale 1, double false
    let mut s_a = Session::new();
    s_a.execute("rectang", &json!({ "p1": [0, 0], "p2": [100, 100] })).unwrap();
    s_a.execute("hatch.user", &json!({ "points": [[50, 50]], "angle": 0.0, "spacing": 5.0, "scale": 1.0, "double": false })).unwrap();
    let doc_a = s_a.doc().unwrap();
    let h_a = first_hatch(&s_a);
    let loops_a = cadcraft_render::fill_loops(&h_a, 1e-3);
    let lines_a = cadcraft_render::pattern_lines(doc_a, &h_a, &loops_a, 0.0, None);

    // Test Case B: angle 45°, spacing 5, scale 1, double false
    let mut s_b = Session::new();
    s_b.execute("rectang", &json!({ "p1": [0, 0], "p2": [100, 100] })).unwrap();
    s_b.execute("hatch.user", &json!({ "points": [[50, 50]], "angle": 45.0, "spacing": 5.0, "scale": 1.0, "double": false })).unwrap();
    let doc_b = s_b.doc().unwrap();
    let h_b = first_hatch(&s_b);
    let loops_b = cadcraft_render::fill_loops(&h_b, 1e-3);
    let lines_b = cadcraft_render::pattern_lines(doc_b, &h_b, &loops_b, 0.0, None);

    // Test Case C: angle 0°, spacing 15, scale 1, double false
    let mut s_c = Session::new();
    s_c.execute("rectang", &json!({ "p1": [0, 0], "p2": [100, 100] })).unwrap();
    s_c.execute("hatch.user", &json!({ "points": [[50, 50]], "angle": 0.0, "spacing": 15.0, "scale": 1.0, "double": false })).unwrap();
    let doc_c = s_c.doc().unwrap();
    let h_c = first_hatch(&s_c);
    let loops_c = cadcraft_render::fill_loops(&h_c, 1e-3);
    let lines_c = cadcraft_render::pattern_lines(doc_c, &h_c, &loops_c, 0.0, None);

    // Test Case D: angle 0°, spacing 5, scale 1, double true
    let mut s_d = Session::new();
    s_d.execute("rectang", &json!({ "p1": [0, 0], "p2": [100, 100] })).unwrap();
    s_d.execute("hatch.user", &json!({ "points": [[50, 50]], "angle": 0.0, "spacing": 5.0, "scale": 1.0, "double": true })).unwrap();
    let doc_d = s_d.doc().unwrap();
    let h_d = first_hatch(&s_d);
    let loops_d = cadcraft_render::fill_loops(&h_d, 1e-3);
    let lines_d = cadcraft_render::pattern_lines(doc_d, &h_d, &loops_d, 0.0, None);

    // 1. Verify entity stored values
    assert_eq!(h_a.pattern, "_U");
    assert_eq!(h_a.scale, 1.0);
    assert!((h_a.angle - 0.0).abs() < 1e-9);
    assert_eq!(h_a.pattern_lines.len(), 1);
    assert_eq!(h_a.pattern_lines[0].delta, (0.0, 5.0));

    assert_eq!(h_b.pattern, "_U");
    assert_eq!(h_b.scale, 1.0);
    assert!((h_b.angle - 45.0f64.to_radians()).abs() < 1e-9);
    assert_eq!(h_b.pattern_lines.len(), 1);
    assert_eq!(h_b.pattern_lines[0].delta, (0.0, 5.0));

    assert_eq!(h_c.pattern, "_U");
    assert_eq!(h_c.scale, 1.0);
    assert!((h_c.angle - 0.0).abs() < 1e-9);
    assert_eq!(h_c.pattern_lines.len(), 1);
    assert_eq!(h_c.pattern_lines[0].delta, (0.0, 15.0));

    assert_eq!(h_d.pattern, "_U");
    assert_eq!(h_d.scale, 1.0);
    assert!((h_d.angle - 0.0).abs() < 1e-9);
    assert_eq!(h_d.pattern_lines.len(), 2);
    assert_eq!(h_d.pattern_lines[0].delta, (0.0, 5.0));
    assert_eq!(h_d.pattern_lines[1].delta, (0.0, 5.0));

    // 2. Case B has lines with a different orientation than Case A
    // In Case A (angle 0°), all segments are horizontal: dy = 0, dx > 0
    assert!(!lines_a.is_empty());
    for seg in &lines_a {
        assert_eq!(seg.len(), 2);
        assert!((seg[0].y - seg[1].y).abs() < 1e-6, "Case A line must be horizontal: {:?}", seg);
    }
    // In Case B (angle 45°), segments have slope 1.0: (p1.y - p0.y) / (p1.x - p0.x) ≈ 1.0
    assert!(!lines_b.is_empty());
    for seg in &lines_b {
        assert_eq!(seg.len(), 2);
        let dx = seg[1].x - seg[0].x;
        let dy = seg[1].y - seg[0].y;
        let slope = (dy / dx).abs();
        assert!((slope - 1.0).abs() < 1e-4, "Case B line must be at 45 degrees, slope ~ 1.0, got {}: {:?}", slope, seg);
    }

    // 3. Case C has a different spacing than Case A
    // Case A has spacing 5 -> on [0, 100] there are 20 interior/boundary scanlines (y = 0, 5, 10, ..., 95)
    // Case C has spacing 15 -> on [0, 100] there are 7 interior/boundary scanlines (y = 0, 15, ..., 90)
    assert_eq!(lines_a.len(), 20, "Case A line count with spacing 5");
    assert_eq!(lines_c.len(), 7, "Case C line count with spacing 15");
    // Verify distance between consecutive lines in Case A is 5 and in Case C is 15
    let mut a_ys: Vec<f64> = lines_a.iter().map(|seg| seg[0].y).collect();
    a_ys.sort_by(f64::total_cmp);
    for window in a_ys.windows(2) {
        assert!((window[1] - window[0] - 5.0).abs() < 1e-4, "Spacing between lines in A should be 5.0");
    }
    let mut c_ys: Vec<f64> = lines_c.iter().map(|seg| seg[0].y).collect();
    c_ys.sort_by(f64::total_cmp);
    for window in c_ys.windows(2) {
        assert!((window[1] - window[0] - 15.0).abs() < 1e-4, "Spacing between lines in C should be 15.0");
    }

    // 4. Case D contains the two families of lines (cross-hatching)
    // One family horizontal (dy == 0), one family vertical (dx == 0)
    let horizontal_count = lines_d.iter().filter(|seg| (seg[0].y - seg[1].y).abs() < 1e-4).count();
    let vertical_count = lines_d.iter().filter(|seg| (seg[0].x - seg[1].x).abs() < 1e-4).count();
    assert_eq!(horizontal_count, 20, "Case D must have 20 horizontal lines");
    assert_eq!(vertical_count, 21, "Case D must have 21 vertical lines");
    assert_eq!(lines_d.len(), 41, "Case D must have 41 total lines across both families");
}

#[test]
fn hatchedit_modifies_user_defined_hatch_parameters() {
    let mut s = Session::new();
    s.execute("rectang", &json!({ "p1": [0, 0], "p2": [50, 50] })).unwrap();
    let res = s.execute("hatch.user", &json!({ "points": [[25, 25]], "spacing": 5.0, "angle": 0.0, "double": false })).unwrap();
    let handle = res["handle"].as_str().unwrap();

    // 1. Toggle double: true
    s.execute("hatchedit", &json!({ "handle": handle, "double": true })).unwrap();
    let h1 = first_hatch(&s);
    assert_eq!(h1.pattern, "_U");
    assert_eq!(h1.pattern_lines.len(), 2, "hatchedit with double: true must produce 2 line families");
    assert!((h1.pattern_lines[0].delta.1 - 5.0).abs() < 1e-6);
    assert!((h1.pattern_lines[1].delta.1 - 5.0).abs() < 1e-6);

    // 2. Change spacing to 12.0
    s.execute("hatchedit", &json!({ "handle": handle, "spacing": 12.0 })).unwrap();
    let h2 = first_hatch(&s);
    assert_eq!(h2.pattern_lines.len(), 2, "Double cross-hatch remains preserved");
    assert!((h2.pattern_lines[0].delta.1 - 12.0).abs() < 1e-6);
    assert!((h2.pattern_lines[1].delta.1 - 12.0).abs() < 1e-6);

    // 3. Change angle to 30.0
    s.execute("hatchedit", &json!({ "handle": handle, "angle": 30.0 })).unwrap();
    let h3 = first_hatch(&s);
    assert!((h3.angle - 30.0f64.to_radians()).abs() < 1e-6);

    // 4. Change scale to 2.5
    s.execute("hatchedit", &json!({ "handle": handle, "scale": 2.5 })).unwrap();
    let h4 = first_hatch(&s);
    assert!((h4.scale - 2.5).abs() < 1e-6);
}

#[test]
fn dxf_read_user_defined_hatch_without_group_78() {
    let dxf_content = r#"0
SECTION
2
ENTITIES
0
HATCH
8
0
10
0.0
20
0.0
30
0.0
210
0.0
220
0.0
230
1.0
2
_U
70
0
71
0
91
1
92
3
72
0
73
1
93
4
10
0.0
20
0.0
10
50.0
20
0.0
10
50.0
20
50.0
10
0.0
20
50.0
97
0
75
0
76
0
52
0.0
41
7.5
77
1
98
0
0
ENDSEC
0
EOF
"#;
    let doc = cadcraft_io::read_dxf(dxf_content.as_bytes()).unwrap();
    let hatches: Vec<Hatch> = doc.model.iter().filter_map(|e| if let EntityKind::Hatch(h) = &e.kind { Some(h.clone()) } else { None }).collect();

    assert_eq!(hatches.len(), 1, "Must read hatch without group 78");
    let h = &hatches[0];
    assert_eq!(h.pattern, "_U");
    assert_eq!(h.pattern_lines.len(), 2, "Must reconstruct 2 line families from group 77 = 1");
    assert!((h.pattern_lines[0].delta.1 - 7.5).abs() < 1e-6, "Line spacing reconstructed from group 41");
    assert!((h.pattern_lines[1].delta.1 - 7.5).abs() < 1e-6, "Cross line spacing reconstructed from group 41");

    // Rendering does not fall back to ANSI31
    let loops = cadcraft_render::fill_loops(h, 1e-3);
    let lines = cadcraft_render::pattern_lines(&doc, h, &loops, 0.0, None);
    assert!(!lines.is_empty(), "Must render lines from reconstructed definition");
}
