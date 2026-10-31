use std::path::PathBuf;

use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, qnew_template, set_io, set_qnew_template};
use serde_json::json;

struct RestoreTemplate(String);

impl Drop for RestoreTemplate {
    fn drop(&mut self) {
        set_qnew_template(&self.0);
    }
}

struct TempDirectory(PathBuf);

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn qnew_loads_a_synthetic_dxf_template_from_disk() {
    set_io(IoHooks {
        read: |bytes, name| cadcraft_io::read(bytes, name).map_err(|e| e.to_string()),
        write: |drawing, name| cadcraft_io::write(drawing, name).map_err(|e| e.to_string()),
        plot: None,
    });
    let _restore = RestoreTemplate(qnew_template());
    let dir = std::env::temp_dir().join(format!("cadcraft-template-load-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create isolated template directory");
    let _cleanup = TempDirectory(dir.clone());
    let path = dir.join("synthetic.dwt");
    let dxf = "0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n0\nLAYER\n2\nSYNTHETIC_LAYER\n70\n0\n62\n3\n0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nSYNTHETIC_LAYER\n10\n1.0\n20\n2.0\n30\n3.0\n11\n4.0\n21\n5.0\n31\n6.0\n0\nENDSEC\n0\nEOF\n";
    std::fs::write(&path, dxf).expect("write synthetic DXF template");
    set_qnew_template(&path.to_string_lossy());

    let mut session = Session::new();
    session.execute("qnew", &json!({})).expect("QNEW reads the configured template");

    let drawing = session.doc().expect("new drawing is active");
    assert!(drawing.layers.iter().any(|layer| layer.name == "SYNTHETIC_LAYER"));
    let line = drawing.model.iter().find_map(|entity| match &entity.kind {
        cadcraft_doc::EntityKind::Line(line) => Some(line),
        _ => None,
    });
    let line = line.expect("template's line was loaded");
    assert_eq!(line.a, cadcraft_geom::Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(line.b, cadcraft_geom::Vec3::new(4.0, 5.0, 6.0));
}
