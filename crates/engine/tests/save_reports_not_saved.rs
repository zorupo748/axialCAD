//! Saving a drawing with entities of types CADCraft doesn't model (read from another program's
//! file) as DWG says which ones the file doesn't keep, instead of dropping them silently. DXF
//! writes them back as read: nothing to report.

use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use cadcraft_engine::doc::{Block, Common, Drawing, EntityKind, RawTag, Space, Unknown};
use serde_json::json;

fn unknown(kind: &str) -> EntityKind {
    EntityKind::Unknown(Unknown { dxf_type: kind.into(), tags: vec![RawTag { code: 8, value: "0".into() }] })
}

#[test]
fn save_rejects_unexportable_dwg_entities_and_preserves_destination() {
    set_io(IoHooks { read: |_, _| Err("no reader in this test".into()), write: |_, _| Ok(b"0\nEOF\n".to_vec()), plot: None });
    let mut d = Drawing::new_imperial();
    for k in ["MLINE", "3DSOLID", "MLINE"] {
        d.add(&Space::Model, Common::default(), unknown(k)).unwrap();
    }
    let mut b = Block::new("Part");
    let h = d.new_handle();
    b.entities.push(cadcraft_engine::doc::Entity { handle: h, common: Common::default(), kind: unknown("REGION") });
    d.blocks.insert("Part".into(), std::sync::Arc::new(b));
    let mut s = Session::empty();
    s.open_drawing(d, "foreign.dxf", None);

    // 1. Unexportable drawing to DWG returns an explicit error explaining the condition.
    let path = std::env::temp_dir().join(format!("cadcraft-not-saved-{}.dwg", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let err = s.execute("saveas", &json!({ "path": path.to_string_lossy() })).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("Cannot save to DWG without losing data"), "{msg}");
    assert!(msg.contains("1 3DSOLID"), "{msg}");
    assert!(msg.contains("2 MLINE"), "{msg}");
    assert!(msg.contains("1 REGION"), "{msg}");
    // No partial file is left behind.
    assert!(!path.exists(), "Target file must not be created on aborted save");

    // 2. A pre-existing destination file preserves its content when save is aborted.
    let preexisting = b"pre-existing dwg content that must be preserved";
    std::fs::write(&path, preexisting).unwrap();
    let err2 = s.execute("saveas", &json!({ "path": path.to_string_lossy() })).unwrap_err();
    assert!(err2.to_string().contains("Cannot save to DWG without losing data"));
    let contents = std::fs::read(&path).unwrap();
    assert_eq!(contents, preexisting, "Pre-existing file must retain its original contents");
    let _ = std::fs::remove_file(&path);

    // 3. document.bytes with format dwg also refuses to drop unexportable entities.
    let err3 = s.execute("document.bytes", &json!({ "format": "dwg" })).unwrap_err();
    assert!(err3.to_string().contains("Cannot save to DWG without losing data"));

    // 4. DXF keeps them.
    let dxf = std::env::temp_dir().join(format!("cadcraft-not-saved-{}.dxf", std::process::id()));
    let r = s.execute("saveas", &json!({ "path": dxf.to_string_lossy() })).unwrap();
    let _ = std::fs::remove_file(&dxf);
    assert_eq!(r["path"].as_str(), dxf.to_str());

    // 5. Exporting an image isn't a save of the drawing: succeeds.
    let png = std::env::temp_dir().join(format!("cadcraft-not-saved-{}.png", std::process::id()));
    let r = s.execute("saveas", &json!({ "path": png.to_string_lossy() })).unwrap();
    let _ = std::fs::remove_file(&png);
    assert_eq!(r["path"].as_str(), png.to_str());

    // 6. A drawing the file keeps completely (compatible drawing): saveas to DWG succeeds.
    s.open_drawing(Drawing::new_imperial(), "plain.dxf", None);
    let dwg_path = std::env::temp_dir().join(format!("cadcraft-compatible-{}.dwg", std::process::id()));
    let r = s.execute("saveas", &json!({ "path": dwg_path.to_string_lossy() })).unwrap();
    let _ = std::fs::remove_file(&dwg_path);
    assert_eq!(r["path"].as_str(), dwg_path.to_str());

    // document.bytes succeeds on compatible drawing.
    let r = s.execute("document.bytes", &json!({ "format": "dwg" })).unwrap();
    assert!(r.get("data").is_some());
}

#[test]
fn real_dwg_export_fails_on_unexportable_and_preserves_dxf() {
    cadcraft_engine::cmd::file::set_write_as(|d, name, opts| {
        cadcraft_io::write_with(d, name, &cadcraft_io::SaveOptions { version: opts.version, binary: opts.binary })
            .map_err(|e| e.to_string())
    });
    set_io(IoHooks {
        read: |b, n| cadcraft_io::read(b, n).map_err(|e| e.to_string()),
        write: |d, n| cadcraft_io::write(d, n).map_err(|e| e.to_string()),
        plot: None,
    });
    let mut d = Drawing::new_imperial();
    d.add(&Space::Model, Common::default(), unknown("AEC_CUSTOM_WALL")).unwrap();
    let mut s = Session::empty();
    s.open_drawing(d, "foreign.dxf", None);

    // DWG export fails safely with an explicit error.
    let path = std::env::temp_dir().join(format!("cadcraft-real-not-saved-{}.dwg", std::process::id()));
    let preexisting = b"precious dwg data";
    std::fs::write(&path, preexisting).unwrap();
    let err = s.execute("saveas", &json!({ "path": path.to_string_lossy() })).unwrap_err();
    assert!(err.to_string().contains("Cannot save to DWG without losing data"), "{}", err);
    assert!(err.to_string().contains("AEC_CUSTOM_WALL"), "{}", err);
    assert_eq!(std::fs::read(&path).unwrap(), preexisting);
    let _ = std::fs::remove_file(&path);

    // DXF export preserves the unknown entity round-trip.
    let dxf_path = std::env::temp_dir().join(format!("cadcraft-real-saved-{}.dxf", std::process::id()));
    let r = s.execute("saveas", &json!({ "path": dxf_path.to_string_lossy() })).unwrap();
    assert_eq!(r["path"].as_str(), dxf_path.to_str());
    let bytes = std::fs::read(&dxf_path).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("AEC_CUSTOM_WALL"), "DXF must preserve AEC_CUSTOM_WALL");
    let _ = std::fs::remove_file(&dxf_path);
}

#[test]
fn templates_report_what_dwg_does_not_keep() {
    use cadcraft_engine::cmd::file::drops_unknown;
    assert!(drops_unknown("a.dwg") && drops_unknown("B.DWT"));
    assert!(!drops_unknown("a.dxf"));
}

#[test]
fn solids_whose_geometry_the_data_store_keeps_are_not_reported_from_2013() {
    use cadcraft_engine::cmd::file::{Version, not_saved_as};
    use cadcraft_engine::doc::Handle;
    use cadcraft_engine::doc::foreign::Foreign;
    let mut d = Drawing::new_imperial();
    let kept = d.add(&Space::Model, Common::default(), unknown("3DSOLID")).unwrap();
    d.add(&Space::Model, Common::default(), unknown("3DSOLID")).unwrap();
    d.add(&Space::Model, Common::default(), unknown("REGION")).unwrap();
    let record = |h: Handle| {
        vec![
            RawTag { code: 0, value: "ACDSRECORD".into() },
            RawTag { code: 320, value: h.hex() },
            RawTag { code: 2, value: "ASM_Data".into() },
            RawTag { code: 310, value: "00".into() },
        ]
    };
    d.foreign = std::sync::Arc::new(Foreign { data_store: record(kept), ..Foreign::default() });
    // The solid with a data store record keeps its geometry from 2013 on; the others don't.
    assert_eq!(not_saved_as(&d, Version::R2018), [("3DSOLID".to_string(), 1), ("REGION".to_string(), 1)].into_iter().collect());
    // Before 2013 a file has no data store: every solid loses its geometry.
    assert_eq!(not_saved_as(&d, Version::R2010), [("3DSOLID".to_string(), 2), ("REGION".to_string(), 1)].into_iter().collect());
}
