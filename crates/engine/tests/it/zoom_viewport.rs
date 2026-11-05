//! ZOOM inside a layout viewport (MSPACE) works on that viewport, as in AutoCAD: Extents and
//! Window fit the viewport, nX keeps its centre, nXP sets its scale (#370).

use cadcraft_doc::{EntityKind, Viewport};
use cadcraft_engine::Session;
use serde_json::json;

fn viewport(s: &Session, h: &str) -> Viewport {
    let d = s.doc().unwrap();
    let e = d.layout("Layout1").unwrap().entities.iter().find(|e| e.handle.hex() == h).unwrap();
    match &e.kind {
        EntityKind::Viewport(v) => v.clone(),
        _ => panic!("not a viewport"),
    }
}

/// A 1000 × 500 model and a 100 × 80 viewport on Layout1, entered with MSPACE.
fn session() -> (Session, String) {
    let mut s = Session::new();
    s.new_drawing(true);
    s.viewport_px = (1600.0, 900.0);
    s.execute("line", &json!({"points": [[0, 0], [1000, 0]]})).unwrap();
    s.execute("line", &json!({"points": [[0, 0], [0, 500]]})).unwrap();
    s.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
    let r = s.execute("mview", &json!({"p1": [20, 20], "p2": [120, 100]})).unwrap();
    let h = r["viewports"][0].as_str().unwrap().to_string();
    s.execute("viewport.set", &json!({"handle": h, "scale": 1, "center": [0, 0]})).unwrap();
    s.execute("mspace", &json!({"handle": h})).unwrap();
    (s, h)
}

#[test]
fn zoom_inside_a_viewport_acts_on_the_viewport() {
    let (mut s, h) = session();
    let paper = s.state().unwrap().paper_view();

    // Extents fit the viewport (aspect 1.25): 1000 wide needs a view height of 800.
    s.cmdline("zoom e").unwrap();
    let v = viewport(&s, &h);
    assert!(v.view_center.near(cadcraft_geom::Vec2::new(500.0, 250.0), 1e-6), "{:?}", v.view_center);
    assert!(v.view_height >= 800.0 && v.view_height < 900.0, "{}", v.view_height);

    // nX zooms about the viewport centre.
    s.cmdline("zoom 2x").unwrap();
    let v2 = viewport(&s, &h);
    assert!(v2.view_center.near(v.view_center, 1e-6), "{:?}", v2.view_center);
    assert!((v2.view_height - v.view_height / 2.0).abs() < 1e-9);

    // nXP sets the scale (paper units per model unit), whatever the zoom before.
    for (t, scale) in [("zoom 0.1xp", 0.1), ("zoom 1/50xp", 0.02), ("zoom 1/50xp", 0.02)] {
        s.cmdline(t).unwrap();
        let v = viewport(&s, &h);
        assert!((v.height / v.view_height - scale).abs() < 1e-12, "{t}: {}", v.height / v.view_height);
        assert!(v.view_center.near(cadcraft_geom::Vec2::new(500.0, 250.0), 1e-6), "{t}: {:?}", v.view_center);
    }

    // A window fills the viewport: 200 × 100 at aspect 1.25 → height 160.
    s.cmdline("zoom w 400,200 600,300").unwrap();
    let v = viewport(&s, &h);
    assert!(v.view_center.near(cadcraft_geom::Vec2::new(500.0, 250.0), 1e-6), "{:?}", v.view_center);
    assert!((v.view_height - 160.0).abs() < 1e-9, "{}", v.view_height);

    // The JSON form agrees, and the sheet itself never moves.
    s.execute("zoom", &json!({"mode": "scale", "factor": 0.25, "xp": true})).unwrap();
    assert!((viewport(&s, &h).view_height - 320.0).abs() < 1e-9);
    assert_eq!(s.state().unwrap().paper_view(), paper);
}

#[test]
fn a_view_too_large_for_the_viewport_is_refused_and_keeps_the_viewport() {
    // A small paper view makes a finite but huge request map to an infinite model view (#714).
    let (mut s, h) = session();
    s.execute("pspace", &json!({})).unwrap();
    s.execute("view.set", &json!({"center": [70, 60], "height": 0.5})).unwrap();
    s.execute("mspace", &json!({"handle": h})).unwrap();
    let before = viewport(&s, &h);
    assert!(s.execute("view.set", &json!({"center": [0, 0], "height": 1e308})).is_err());
    let after = viewport(&s, &h);
    assert_eq!(after, before);
    assert!(after.view_center.is_finite() && after.view_height.is_finite());
    // A representable view still applies.
    s.execute("view.set", &json!({"center": [0, 0], "height": 1})).unwrap();
    let v = viewport(&s, &h);
    assert!(v.view_height.is_finite() && v.view_height != before.view_height, "{}", v.view_height);
}

#[test]
fn zoom_in_a_locked_viewport_or_on_the_model_tab_is_unchanged() {
    // A locked viewport keeps its view: ZOOM moves the sheet instead.
    let (mut s, h) = session();
    s.execute("viewport.set", &json!({"handle": h, "locked": true})).unwrap();
    let before = viewport(&s, &h);
    let paper = s.state().unwrap().paper_view();
    s.cmdline("zoom e").unwrap();
    s.cmdline("zoom 0.1xp").unwrap();
    assert_eq!(viewport(&s, &h), before);
    assert_ne!(s.state().unwrap().paper_view(), paper);

    // The Model tab still fits the drawing window; nXP there acts like nX.
    let mut s = Session::new();
    s.new_drawing(true);
    s.viewport_px = (1600.0, 900.0);
    s.execute("line", &json!({"points": [[0, 0], [1000, 0]]})).unwrap();
    s.cmdline("zoom e").unwrap();
    let v = s.state().unwrap().view();
    assert!((v.height - 1000.0 / (1600.0 / 900.0) * 1.05).abs() < 1e-6, "{}", v.height);
    s.cmdline("zoom 2xp").unwrap();
    assert!((s.state().unwrap().view().height - v.height / 2.0).abs() < 1e-9);
}

#[test]
fn pan_in_a_locked_viewport_moves_sheet_without_changing_viewport_or_exploding_zoom() {
    let (mut s, h) = session();
    // Layout1 has a viewport `h`. Set scale to 1:50 (0.02) and center to (500, 250).
    s.execute("viewport.set", &json!({"handle": h, "scale": 0.02, "center": [500, 250]})).unwrap();
    // User is in MSPACE (active viewport).
    s.execute("mspace", &json!({"handle": h})).unwrap();
    // User locks the viewport.
    s.execute("viewport.set", &json!({"handle": h, "locked": true})).unwrap();

    let vp_before = viewport(&s, &h);
    assert_eq!(vp_before.locked, true);
    assert!((vp_before.view_height - vp_before.height / 0.02).abs() < 1e-9);
    let paper_before = s.state().unwrap().paper_view();

    // Now pan repeatedly with middle mouse button drag (e.g. 50 drag steps)
    for step in 0..50 {
        let view = s.state().unwrap().view();
        let scale = 900.0 / view.height.max(1e-12);
        s.pan_step(10.0, -5.0, scale).unwrap();

        let paper_now = s.state().unwrap().paper_view();
        // Paper height MUST NOT change!
        assert!(
            (paper_now.height - paper_before.height).abs() < 1e-6,
            "step {step}: paper height changed from {} to {}",
            paper_before.height,
            paper_now.height
        );
        assert!(paper_now.center.is_finite());
        assert!(paper_now.height.is_finite());

        // Viewport scale and framing MUST NOT change!
        let vp_now = viewport(&s, &h);
        assert_eq!(vp_now.view_center, vp_before.view_center, "step {step}: vp view_center changed");
        assert_eq!(vp_now.view_height, vp_before.view_height, "step {step}: vp view_height changed");
    }

    // Switch to another layout and return
    s.execute("layout.set", &json!({"name": "Layout2"})).unwrap();
    let layout2_paper = s.state().unwrap().paper_view();
    assert!(layout2_paper.height > 10.0);

    // Switch back to Layout1
    s.execute("layout.set", &json!({"name": "Layout1"})).unwrap();
    let vp_after_return = viewport(&s, &h);
    assert_eq!(vp_after_return.view_center, vp_before.view_center);
    assert_eq!(vp_after_return.view_height, vp_before.view_height);

    // Unlock viewport and verify navigation inside the viewport now works
    s.execute("mspace", &json!({"handle": h})).unwrap();
    s.execute("viewport.set", &json!({"handle": h, "locked": false})).unwrap();
    let view = s.state().unwrap().view();
    let scale = 900.0 / view.height.max(1e-12);
    s.pan_step(20.0, 0.0, scale).unwrap();
    let vp_unlocked_panned = viewport(&s, &h);
    assert_ne!(vp_unlocked_panned.view_center, vp_before.view_center);
    assert_eq!(vp_unlocked_panned.view_height, vp_before.view_height);
}
