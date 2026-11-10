//! MTEXT contents from scripts and the command line; TEXTEDIT's prompt and options.

use cadcraft_engine::Session;
use cadcraft_engine::doc::{EntityKind, Space};

fn kinds(s: &Session) -> Vec<EntityKind> {
    s.doc().ok().and_then(|d| d.space(&Space::Model)).map(|st| st.iter().map(|e| e.kind.clone()).collect()).unwrap_or_default()
}

fn mtexts(s: &Session) -> Vec<String> {
    kinds(s)
        .into_iter()
        .filter_map(|k| match k {
            EntityKind::MText(t) => Some(t.contents),
            _ => None,
        })
        .collect()
}

fn texts(s: &Session) -> Vec<String> {
    kinds(s)
        .into_iter()
        .filter_map(|k| match k {
            EntityKind::Text(t) => Some(t.value),
            _ => None,
        })
        .collect()
}

#[test]
fn script_mtext_takes_lines_until_an_empty_line() {
    let mut s = Session::new();
    s.script("MTEXT 0,10 20,0\nFirst line\nSecond line\n\nLINE 0,0 5,5\n\n").unwrap();
    assert!(s.running.is_none(), "{}", s.prompt_text());
    assert_eq!(mtexts(&s), vec!["First line\\PSecond line".to_string()]);
    assert_eq!(kinds(&s).len(), 2, "{:?}", kinds(&s));

    // Typed at the command line: spaces stay in the text, Enter on an empty line ends it.
    let mut s = Session::new();
    s.cmdline("MTEXT 0,10 20,0").unwrap();
    s.cmdline("Hello world").unwrap();
    assert!(s.running.is_some());
    s.cmdline("").unwrap();
    assert!(s.running.is_none(), "{}", s.prompt_text());
    assert_eq!(mtexts(&s), vec!["Hello world".to_string()]);
}

#[test]
fn textedit_prompt_mode_and_undo() {
    let mut s = Session::new();
    s.cmdline("TEXT 0,0 1 0 old").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("TEXTEDIT").unwrap();
    let p = s.prompt_text();
    assert!(!p.contains("or or"), "{p}");
    // Edit, then Undo restores the old text.
    s.cmdline("0.5,0.5").unwrap();
    s.cmdline("new").unwrap();
    assert_eq!(texts(&s), vec!["new".to_string()]);
    s.cmdline("u").unwrap();
    assert_eq!(texts(&s), vec!["old".to_string()]);
    // Mode Single: the command ends after one edit.
    s.cmdline("m").unwrap();
    s.cmdline("s").unwrap();
    s.cmdline("0.5,0.5").unwrap();
    s.cmdline("once").unwrap();
    assert!(s.running.is_none(), "{}", s.prompt_text());
    assert_eq!(texts(&s), vec!["once".to_string()]);
}

#[test]
fn text_in_wcs_has_world_z_and_extrusion() {
    let mut s = Session::new();
    s.cmdline("TEXT 10,20 2.5 0 HelloWCS").unwrap();
    s.cmdline("").unwrap();
    let doc = s.doc().unwrap();
    let ent = doc.space(&Space::Model).unwrap().iter().find(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "HelloWCS")).unwrap();
    assert_eq!(ent.common.extrusion, cadcraft_geom::Vec3::Z);
    if let EntityKind::Text(t) = &ent.kind {
        assert_eq!(t.insert, cadcraft_geom::Vec3::new(10.0, 20.0, 0.0));
        assert_eq!(t.rotation, 0.0);
    }
}

#[test]
fn text_in_ucs_view_preserves_plane_and_extrusion() {
    let mut s = Session::new();
    // Set SWISO isometric view
    s.cmdline("-VIEW SWISO").unwrap();
    let cam = s.camera();
    let view_dir = cam.view_dir();
    // Set UCS to View
    s.cmdline("UCS V").unwrap();
    s.cmdline("TEXT 1,2 0.5 0 HelloView").unwrap();
    s.cmdline("").unwrap();
    // Restore UCS World
    s.cmdline("UCS W").unwrap();

    let doc = s.doc().unwrap();
    let ent = doc.space(&Space::Model).unwrap().iter().find(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "HelloView")).unwrap();
    assert!(ent.common.extrusion.near(view_dir, 1e-6), "Extrusion {:?} must match view dir {:?}", ent.common.extrusion, view_dir);
    if let EntityKind::Text(t) = &ent.kind {
        // Strokes in 3D should be perpendicular to view_dir
        let strokes = cadcraft_render::text_strokes_3d(&doc, t, ent.common.extrusion);
        assert!(!strokes.is_empty(), "Text must generate 3D strokes");
        for stroke in &strokes {
            for seg in stroke.windows(2) {
                let v = seg[1] - seg[0];
                assert!(v.dot(view_dir).abs() < 1e-5, "Stroke segment must lie on view plane (dot with normal ~ 0)");
            }
        }
    }
}

#[test]
fn text_in_inclined_ucs_preserves_plane() {
    let mut s = Session::new();
    // Set UCS on YZ plane: origin (0,0,0), X on Y-axis (0,1,0), Y on Z-axis (0,0,1) -> normal is X-axis (1,0,0)
    s.cmdline("UCS 3 0,0,0 0,1,0 0,0,1").unwrap();
    assert!(s.ucs().z_axis.near(cadcraft_geom::Vec3::X, 1e-6));

    s.cmdline("TEXT 5,10 1.0 0 FacadeText").unwrap();
    s.cmdline("").unwrap();
    s.cmdline("UCS W").unwrap();

    let doc = s.doc().unwrap();
    let ent = doc.space(&Space::Model).unwrap().iter().find(|e| matches!(&e.kind, EntityKind::Text(t) if t.value == "FacadeText")).unwrap();
    assert!(ent.common.extrusion.near(cadcraft_geom::Vec3::X, 1e-6));
    if let EntityKind::Text(t) = &ent.kind {
        // Insertion point on YZ plane has X ~ 0
        assert!(t.insert.x.abs() < 1e-6);
        let strokes = cadcraft_render::text_strokes_3d(&doc, t, ent.common.extrusion);
        assert!(!strokes.is_empty());
        for stroke in &strokes {
            for seg in stroke.windows(2) {
                let v = seg[1] - seg[0];
                assert!(v.x.abs() < 1e-5, "Stroke on YZ plane must have x = 0");
            }
        }
    }
}
