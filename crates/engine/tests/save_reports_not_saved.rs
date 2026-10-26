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

fn init_io() {
    cadcraft_engine::cmd::file::set_write_as(|d, name, opts| {
        cadcraft_io::write_with(d, name, &cadcraft_io::SaveOptions { version: opts.version, binary: opts.binary }).map_err(|e| e.to_string())
    });
    set_io(IoHooks {
        read: |b, n| cadcraft_io::read(b, n).map_err(|e| e.to_string()),
        write: |d, n| cadcraft_io::write(d, n).map_err(|e| e.to_string()),
        plot: None,
    });
}

#[test]
fn save_rejects_unexportable_dwg_entities_and_preserves_destination() {
    init_io();
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
    init_io();
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
    init_io();
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

    // Saving to DWG at version 2018 is safely rejected because the drawing has an unrecorded solid and region:
    let mut s = Session::empty();
    s.open_drawing(d, "solids.dxf", None);
    let dwg_path = std::env::temp_dir().join(format!("cadcraft-solids-{}.dwg", std::process::id()));
    let err = s.execute("saveas", &json!({ "path": dwg_path.to_string_lossy(), "version": 2018 })).unwrap_err();
    assert!(err.to_string().contains("Cannot save to DWG without losing data"), "{}", err);
    assert!(err.to_string().contains("1 3DSOLID"), "{}", err);
    assert!(err.to_string().contains("1 REGION"), "{}", err);
    assert!(!dwg_path.exists());
}

#[test]
fn compatible_solids_survive_real_dwg_write_and_are_rejected_before_2013() {
    init_io();
    const SAB_HEX: &str = "414349532042696E61727946696C65000102030405060708090A0B0C0D0E0F456E642D6F662D414349532D64617461";
    let solid = |h: &str| {
        format!(
            "0\n3DSOLID\n5\n{h}\n330\n1F\n100\nAcDbEntity\n8\n0\n100\nAcDbModelerGeometry\n290\n0\n2\n{{00000000-0000-0000-0000-000000000000}}\n100\nAcDb3dSolid\n350\n0\n"
        )
    };
    let record = |h: &str| {
        format!("0\nACDSRECORD\n90\n1\n2\nAcDbDs::ID\n280\n10\n320\n{h}\n2\nASM_Data\n280\n15\n94\n{}\n310\n{SAB_HEX}\n", SAB_HEX.len() / 2)
    };
    let dxf_text = format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n9\n$HANDSEED\n5\n100\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n{}{}0\nENDSEC\n\
         0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n0\nENDSEC\n\
         0\nSECTION\n2\nACDSDATA\n70\n2\n71\n2\n\
         0\nACDSSCHEMA\n90\n1\n1\nAcDb3DSolid_ASM_Data\n2\nAcDbDs::ID\n280\n10\n91\n8\n2\nASM_Data\n280\n15\n91\n0\n\
         {}{}0\nENDSEC\n0\nEOF\n",
        solid("2A"),
        solid("2B"),
        record("2A"),
        record("2B")
    );

    let d = cadcraft_io::read_dxf(dxf_text.as_bytes()).unwrap();
    assert_eq!(d.foreign.data_store_entities().len(), 2);
    let mut s = Session::empty();
    s.open_drawing(d, "solids.dxf", None);

    // 1. Saving to DWG (2018 by default) succeeds and really preserves the ACIS geometry in DWG!
    let dwg_path = std::env::temp_dir().join(format!("cadcraft-solids-kept-{}.dwg", std::process::id()));
    let r = s.execute("saveas", &json!({ "path": dwg_path.to_string_lossy() })).unwrap();
    assert_eq!(r["path"].as_str(), dwg_path.to_str());
    assert!(dwg_path.exists());

    // Verify reading the produced DWG keeps the data store records
    let dwg_bytes = std::fs::read(&dwg_path).unwrap();
    let reloaded = cadcraft_io::read(&dwg_bytes, "check.dwg").unwrap();
    assert_eq!(reloaded.foreign.data_store_entities().len(), 2, "DWG must preserve the ACIS solids data store");
    let _ = std::fs::remove_file(&dwg_path);

    // 2. Saving the same drawing to DWG at version 2010 (pre-2013: no data store) is rejected without modifying destination:
    let r2010_path = std::env::temp_dir().join(format!("cadcraft-solids-r2010-{}.dwg", std::process::id()));
    let preexisting = b"untouched bytes";
    std::fs::write(&r2010_path, preexisting).unwrap();
    let err = s.execute("saveas", &json!({ "path": r2010_path.to_string_lossy(), "version": 2010 })).unwrap_err();
    assert!(err.to_string().contains("Cannot save to DWG without losing data"), "{}", err);
    assert!(err.to_string().contains("2 3DSOLID"), "{}", err);
    assert_eq!(std::fs::read(&r2010_path).unwrap(), preexisting);
    let _ = std::fs::remove_file(&r2010_path);

    // 3. Export to DWG also succeeds and preserves solids
    let export_path = std::env::temp_dir().join(format!("cadcraft-export-solids-{}.dwg", std::process::id()));
    let r_exp = s.execute("export", &json!({ "format": "dwg", "path": export_path.to_string_lossy() })).unwrap();
    assert_eq!(r_exp["format"].as_str(), Some("dwg"));
    assert!(export_path.exists());
    let export_bytes = std::fs::read(&export_path).unwrap();
    let reloaded_exp = cadcraft_io::read(&export_bytes, "check_exp.dwg").unwrap();
    assert_eq!(reloaded_exp.foreign.data_store_entities().len(), 2);
    let _ = std::fs::remove_file(&export_path);
}

#[test]
fn saveas_dwt_rejects_unexportable_entities_and_preserves_destination() {
    init_io();
    let mut d = Drawing::new_imperial();
    d.add(&Space::Model, Common::default(), unknown("AEC_CUSTOM_WALL")).unwrap();
    let mut s = Session::empty();
    s.open_drawing(d, "foreign.dxf", None);

    let path = std::env::temp_dir().join(format!("cadcraft-dwt-safe-{}.dwt", std::process::id()));

    // 1. Unexportable drawing to a pre-existing destination .dwt preserves its content
    let preexisting = b"precious dwt template content that must be preserved";
    std::fs::write(&path, preexisting).unwrap();
    let err = s.execute("saveas", &json!({ "path": path.to_string_lossy() })).unwrap_err();
    assert!(err.to_string().contains("Cannot save to DWG without losing data"), "{}", err);
    assert!(err.to_string().contains("1 AEC_CUSTOM_WALL"), "{}", err);
    let contents = std::fs::read(&path).unwrap();
    assert_eq!(contents, preexisting, "Pre-existing .dwt file must retain its original contents");
    let _ = std::fs::remove_file(&path);

    // 2. Unexportable drawing when destination does not exist does not create the file
    let err2 = s.execute("saveas", &json!({ "path": path.to_string_lossy() })).unwrap_err();
    assert!(err2.to_string().contains("Cannot save to DWG without losing data"), "{}", err2);
    assert!(!path.exists(), "Target .dwt file must not be created on aborted save");

    // 3. Compatible drawing saves to .dwt with success
    s.open_drawing(Drawing::new_imperial(), "plain.dxf", None);
    let r = s.execute("saveas", &json!({ "path": path.to_string_lossy() })).unwrap();
    assert_eq!(r["path"].as_str(), path.to_str());
    assert!(path.exists());
    let _ = std::fs::remove_file(&path);
}

#[test]
fn engine_export_command_rejects_unexportable_dwg_and_preserves_destination() {
    init_io();
    let mut d = Drawing::new_imperial();
    d.add(&Space::Model, Common::default(), unknown("AEC_CUSTOM_WALL")).unwrap();
    let mut s = Session::empty();
    s.open_drawing(d, "foreign.dxf", None);

    let path = std::env::temp_dir().join(format!("cadcraft-export-safe-{}.dwg", std::process::id()));

    // 1. Pre-existing target file with path ending in .dwg (format inferred): fails and preserves existing content
    let preexisting = b"precious export dwg bytes that must not be touched";
    std::fs::write(&path, preexisting).unwrap();
    let err = s.execute("export", &json!({ "path": path.to_string_lossy() })).unwrap_err();
    assert!(err.to_string().contains("Cannot save to DWG without losing data"), "{}", err);
    assert!(err.to_string().contains("1 AEC_CUSTOM_WALL"), "{}", err);
    let contents = std::fs::read(&path).unwrap();
    assert_eq!(contents, preexisting, "Pre-existing file must retain its original contents on export error");
    let _ = std::fs::remove_file(&path);

    // 2. Explicit format dwg: destination does not exist -> fails and does not create the file
    let err2 = s.execute("export", &json!({ "format": "dwg", "path": path.to_string_lossy() })).unwrap_err();
    assert!(err2.to_string().contains("Cannot save to DWG without losing data"), "{}", err2);
    assert!(!path.exists(), "Target file must not be created on aborted export");

    // 3. Export in base64 mode (no path, format dwg): fails with the same error
    let err3 = s.execute("export", &json!({ "format": "dwg" })).unwrap_err();
    assert!(err3.to_string().contains("Cannot save to DWG without losing data"), "{}", err3);

    // 4. Export to DXF succeeds and preserves the unknown entity
    let dxf_path = std::env::temp_dir().join(format!("cadcraft-export-saved-{}.dxf", std::process::id()));
    let r = s.execute("export", &json!({ "format": "dxf", "path": dxf_path.to_string_lossy() })).unwrap();
    assert!(r.get("path").is_some());
    let dxf_bytes = std::fs::read(&dxf_path).unwrap();
    let text = String::from_utf8_lossy(&dxf_bytes);
    assert!(text.contains("AEC_CUSTOM_WALL"), "DXF export must preserve unknown entities");
    let _ = std::fs::remove_file(&dxf_path);

    // 5. Export compatible drawing to DWG succeeds
    s.open_drawing(Drawing::new_imperial(), "plain.dxf", None);
    let r = s.execute("export", &json!({ "format": "dwg", "path": path.to_string_lossy() })).unwrap();
    assert!(r.get("path").is_some());
    assert!(path.exists());
    let _ = std::fs::remove_file(&path);

    // 6. Explicit format dxf with a .dwg path does not reject unknown entities and writes DXF bytes
    let mut d_strange = Drawing::new_imperial();
    d_strange.add(&Space::Model, Common::default(), unknown("AEC_CUSTOM_WALL")).unwrap();
    s.open_drawing(d_strange, "foreign.dxf", None);
    let strange_path = std::env::temp_dir().join(format!("cadcraft-strange-{}.dwg", std::process::id()));
    let r = s.execute("export", &json!({ "format": "dxf", "path": strange_path.to_string_lossy() })).unwrap();
    assert_eq!(r["format"].as_str(), Some("dxf"));
    let strange_bytes = std::fs::read(&strange_path).unwrap();
    let strange_text = String::from_utf8_lossy(&strange_bytes);
    assert!(strange_text.contains("AEC_CUSTOM_WALL"), "DXF format must preserve unknown entities even if path ends in .dwg");
    let _ = std::fs::remove_file(&strange_path);
}

#[test]
fn dxf_rejects_pre_2013_with_acis_data_store_and_preserves_destination() {
    init_io();
    const SAB_HEX: &str = "414349532042696E61727946696C65000102030405060708090A0B0C0D0E0F456E642D6F662D414349532D64617461";
    let solid = |h: &str| {
        format!(
            "0\n3DSOLID\n5\n{h}\n330\n1F\n100\nAcDbEntity\n8\n0\n100\nAcDbModelerGeometry\n290\n0\n2\n{{00000000-0000-0000-0000-000000000000}}\n100\nAcDb3dSolid\n350\n0\n"
        )
    };
    let record = |h: &str| {
        format!("0\nACDSRECORD\n90\n1\n2\nAcDbDs::ID\n280\n10\n320\n{h}\n2\nASM_Data\n280\n15\n94\n{}\n310\n{SAB_HEX}\n", SAB_HEX.len() / 2)
    };
    let dxf_text = format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n9\n$HANDSEED\n5\n100\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n{}{}0\nENDSEC\n\
         0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n0\nENDSEC\n\
         0\nSECTION\n2\nACDSDATA\n70\n2\n71\n2\n\
         0\nACDSSCHEMA\n90\n1\n1\nAcDb3DSolid_ASM_Data\n2\nAcDbDs::ID\n280\n10\n91\n8\n2\nASM_Data\n280\n15\n91\n0\n\
         {}{}0\nENDSEC\n0\nEOF\n",
        solid("2A"),
        solid("2B"),
        record("2A"),
        record("2B")
    );

    let d = cadcraft_io::read_dxf(dxf_text.as_bytes()).unwrap();
    assert_eq!(d.foreign.data_store_entities().len(), 2);
    let mut s = Session::empty();
    s.open_drawing(d, "solids.dxf", None);

    // 1. Rechazar un DXF anterior a R2013 con sólidos que dependan de ACDSDATA, preservando los bytes de un destino existente:
    let r2010_path = std::env::temp_dir().join(format!("cadcraft-dxf-r2010-{}.dxf", std::process::id()));
    let preexisting = b"preexisting dxf bytes that must not be overwritten";
    std::fs::write(&r2010_path, preexisting).unwrap();

    let err = s.execute("saveas", &json!({ "path": r2010_path.to_string_lossy(), "version": 2010 })).unwrap_err();
    assert!(err.to_string().contains("Cannot save to DXF 2010 without losing data"), "{}", err);
    assert!(err.to_string().contains("2 3DSOLID"), "{}", err);
    assert_eq!(std::fs::read(&r2010_path).unwrap(), preexisting);
    let _ = std::fs::remove_file(&r2010_path);

    // document.bytes con formato dxf y version 2000 también rechaza:
    let err_bytes = s.execute("document.bytes", &json!({ "format": "dxf", "version": 2000 })).unwrap_err();
    assert!(err_bytes.to_string().contains("Cannot save to DXF 2000 without losing data"), "{}", err_bytes);

    // 2. Permitir un DXF R2018 con esos mismos sólidos:
    let r2018_path = std::env::temp_dir().join(format!("cadcraft-dxf-r2018-{}.dxf", std::process::id()));
    let r = s.execute("saveas", &json!({ "path": r2018_path.to_string_lossy(), "version": 2018 })).unwrap();
    assert_eq!(r["path"].as_str(), r2018_path.to_str());
    assert!(r2018_path.exists());
    let r2018_bytes = std::fs::read(&r2018_path).unwrap();
    let reloaded_r2018 = cadcraft_io::read_dxf(&r2018_bytes).unwrap();
    assert_eq!(reloaded_r2018.foreign.data_store_entities().len(), 2);
    let _ = std::fs::remove_file(&r2018_path);

    // document.bytes con formato dxf en version 2018 tiene éxito:
    let r_bytes = s.execute("document.bytes", &json!({ "format": "dxf", "version": 2018 })).unwrap();
    assert!(r_bytes.get("data").is_some());

    // 3. Seguir permitiendo entidades personalizadas que DXF sí conserva, incluso si se pide una versión anterior (p. ej. R2000):
    let mut d_custom = Drawing::new_imperial();
    d_custom.add(&Space::Model, Common::default(), unknown("AEC_CUSTOM_WALL")).unwrap();
    let mut s_custom = Session::empty();
    s_custom.open_drawing(d_custom, "custom.dxf", None);

    let r2000_custom_path = std::env::temp_dir().join(format!("cadcraft-dxf-custom-r2000-{}.dxf", std::process::id()));
    let r_custom = s_custom.execute("saveas", &json!({ "path": r2000_custom_path.to_string_lossy(), "version": 2000 })).unwrap();
    assert_eq!(r_custom["path"].as_str(), r2000_custom_path.to_str());
    assert!(r2000_custom_path.exists());
    let custom_bytes = std::fs::read(&r2000_custom_path).unwrap();
    let custom_text = String::from_utf8_lossy(&custom_bytes);
    assert!(custom_text.contains("AEC_CUSTOM_WALL"), "DXF R2000 must preserve AEC_CUSTOM_WALL");
    let _ = std::fs::remove_file(&r2000_custom_path);

    // document.bytes con formato dxf y version 2000 sobre entidad personalizada tiene éxito:
    let r_custom_bytes = s_custom.execute("document.bytes", &json!({ "format": "dxf", "version": 2000 })).unwrap();
    assert!(r_custom_bytes.get("data").is_some());
}

#[test]
fn cannot_bypass_dwg_protection_from_block_editor() {
    init_io();
    let mut d = Drawing::new_imperial();
    // Host drawing contains an unexportable entity in model space:
    d.add(&Space::Model, Common::default(), unknown("AEC_CUSTOM_WALL")).unwrap();
    // Host drawing also has a block named "CleanBlock" containing only normal entities:
    let mut b = Block::new("CleanBlock");
    b.entities.push(cadcraft_engine::doc::Entity {
        handle: d.new_handle(),
        common: Common::default(),
        kind: EntityKind::Line(cadcraft_engine::doc::Line {
            a: cadcraft_engine::geom::Vec3::new(0.0, 0.0, 0.0),
            b: cadcraft_engine::geom::Vec3::new(10.0, 10.0, 0.0),
        }),
    });
    d.blocks.insert("CleanBlock".into(), std::sync::Arc::new(b));

    let mut s = Session::empty();
    s.open_drawing(d, "host.dxf", None);

    // Open Block Editor on CleanBlock. Now s.doc() is the clean block, but s.state().drawing() is the host drawing.
    s.execute("bedit", &json!({ "name": "CleanBlock" })).unwrap();
    assert_eq!(s.state().unwrap().block_editor(), Some("CleanBlock"));

    // Attempting to save as DWG from inside the Block Editor must NOT bypass the protection:
    let dwg_path = std::env::temp_dir().join(format!("cadcraft-bedit-bypass-{}.dwg", std::process::id()));
    let preexisting = b"untouched bedit dwg content";
    std::fs::write(&dwg_path, preexisting).unwrap();

    let err = s.execute("saveas", &json!({ "path": dwg_path.to_string_lossy() })).unwrap_err();
    assert!(err.to_string().contains("Cannot save to DWG without losing data"), "{}", err);
    assert!(err.to_string().contains("1 AEC_CUSTOM_WALL"), "{}", err);
    assert_eq!(std::fs::read(&dwg_path).unwrap(), preexisting);
    let _ = std::fs::remove_file(&dwg_path);

    // document.bytes format dwg from inside Block Editor is also blocked:
    let err2 = s.execute("document.bytes", &json!({ "format": "dwg" })).unwrap_err();
    assert!(err2.to_string().contains("Cannot save to DWG without losing data"), "{}", err2);
    assert!(err2.to_string().contains("1 AEC_CUSTOM_WALL"), "{}", err2);

    // Normal valid save when host drawing is clean works properly:
    let mut s_clean = Session::empty();
    let mut d_clean = Drawing::new_imperial();
    let mut b_clean = Block::new("CleanBlock");
    b_clean.entities.push(cadcraft_engine::doc::Entity {
        handle: d_clean.new_handle(),
        common: Common::default(),
        kind: EntityKind::Line(cadcraft_engine::doc::Line {
            a: cadcraft_engine::geom::Vec3::new(0.0, 0.0, 0.0),
            b: cadcraft_engine::geom::Vec3::new(10.0, 10.0, 0.0),
        }),
    });
    d_clean.blocks.insert("CleanBlock".into(), std::sync::Arc::new(b_clean));
    s_clean.open_drawing(d_clean, "clean_host.dxf", None);
    s_clean.execute("bedit", &json!({ "name": "CleanBlock" })).unwrap();

    let clean_dwg = std::env::temp_dir().join(format!("cadcraft-bedit-clean-{}.dwg", std::process::id()));
    let _r = s_clean.execute("saveas", &json!({ "path": clean_dwg.to_string_lossy() })).unwrap();
    assert!(clean_dwg.exists());
    let _ = std::fs::remove_file(&clean_dwg);
}

#[test]
fn export_command_rejects_dxf_pre_2013_with_acis_data_store_and_preserves_destination() {
    init_io();
    const SAB_HEX: &str = "414349532042696E61727946696C65000102030405060708090A0B0C0D0E0F456E642D6F662D414349532D64617461";
    let solid = |h: &str| {
        format!(
            "0\n3DSOLID\n5\n{h}\n330\n1F\n100\nAcDbEntity\n8\n0\n100\nAcDbModelerGeometry\n290\n0\n2\n{{00000000-0000-0000-0000-000000000000}}\n100\nAcDb3dSolid\n350\n0\n"
        )
    };
    let record = |h: &str| {
        format!("0\nACDSRECORD\n90\n1\n2\nAcDbDs::ID\n280\n10\n320\n{h}\n2\nASM_Data\n280\n15\n94\n{}\n310\n{SAB_HEX}\n", SAB_HEX.len() / 2)
    };
    let dxf_text = format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n9\n$HANDSEED\n5\n100\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n\
         0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n0\nENDSEC\n\
         0\nSECTION\n2\nACDSDATA\n70\n1\n71\n1\n\
         0\nACDSSCHEMA\n90\n1\n1\nAcDb3DSolid_ASM_Data\n2\nAcDbDs::ID\n280\n10\n91\n8\n2\nASM_Data\n280\n15\n91\n0\n\
         {}0\nENDSEC\n0\nEOF\n",
        solid("2A"),
        record("2A")
    );

    let mut d = cadcraft_io::read_dxf(dxf_text.as_bytes()).unwrap();
    // Configure drawing as pre-2013 (R2000):
    d.header.set_str("ACADVER", "AC1015");
    let mut s = Session::empty();
    s.open_drawing(d, "solids_r2000.dxf", None);

    let dest = std::env::temp_dir().join(format!("cadcraft-export-dxf-r2000-{}.dxf", std::process::id()));
    let preexisting = b"preexisting export dxf content";
    std::fs::write(&dest, preexisting).unwrap();

    // 1. export to DXF when drawing is pre-2013 and has ACDSDATA is rejected:
    let err = s.execute("export", &json!({ "format": "dxf", "path": dest.to_string_lossy() })).unwrap_err();
    assert!(err.to_string().contains("Cannot save to DXF 2000 without losing data"), "{}", err);
    assert!(err.to_string().contains("1 3DSOLID"), "{}", err);
    assert_eq!(std::fs::read(&dest).unwrap(), preexisting);
    let _ = std::fs::remove_file(&dest);

    // export base64 is also rejected:
    let err2 = s.execute("export", &json!({ "format": "dxf" })).unwrap_err();
    assert!(err2.to_string().contains("Cannot save to DXF 2000 without losing data"), "{}", err2);

    // 2. export to DXF when drawing is R2018 succeeds:
    s.doc_mut().unwrap().header.set_str("ACADVER", "AC1032");
    let r = s.execute("export", &json!({ "format": "dxf", "path": dest.to_string_lossy() })).unwrap();
    assert_eq!(r["format"].as_str(), Some("dxf"));
    assert!(dest.exists());
    let _ = std::fs::remove_file(&dest);
}

#[test]
fn export_from_block_editor_uses_block_document_and_version() {
    init_io();
    const SAB_HEX: &str = "414349532042696E61727946696C65000102030405060708090A0B0C0D0E0F456E642D6F662D414349532D64617461";
    let solid = |h: &str| {
        format!(
            "0\n3DSOLID\n5\n{h}\n330\n1F\n100\nAcDbEntity\n8\n0\n100\nAcDbModelerGeometry\n290\n0\n2\n{{00000000-0000-0000-0000-000000000000}}\n100\nAcDb3dSolid\n350\n0\n"
        )
    };
    let record = |h: &str| {
        format!("0\nACDSRECORD\n90\n1\n2\nAcDbDs::ID\n280\n10\n320\n{h}\n2\nASM_Data\n280\n15\n94\n{}\n310\n{SAB_HEX}\n", SAB_HEX.len() / 2)
    };
    let dxf_text = format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1032\n9\n$HANDSEED\n5\n100\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n{}0\nENDSEC\n\
         0\nSECTION\n2\nOBJECTS\n0\nDICTIONARY\n5\nC\n330\n0\n100\nAcDbDictionary\n0\nENDSEC\n\
         0\nSECTION\n2\nACDSDATA\n70\n1\n71\n1\n\
         0\nACDSSCHEMA\n90\n1\n1\nAcDb3DSolid_ASM_Data\n2\nAcDbDs::ID\n280\n10\n91\n8\n2\nASM_Data\n280\n15\n91\n0\n\
         {}0\nENDSEC\n0\nEOF\n",
        solid("2A"),
        record("2A")
    );

    let d_solids = cadcraft_io::read_dxf(dxf_text.as_bytes()).unwrap();

    // Setup host drawing (R2018):
    let mut host = Drawing::new_imperial();
    host.header.set_str("ACADVER", "AC1032");

    // Block with a solid:
    let mut b = Block::new("SolidBlock");
    b.entities.push((**d_solids.model.iter().next().unwrap()).clone());
    host.blocks.insert("SolidBlock".into(), std::sync::Arc::new(b));
    host.foreign = d_solids.foreign;

    let mut s = Session::empty();
    s.open_drawing(host, "host_r2018.dxf", None);

    // Open Block Editor on SolidBlock:
    s.execute("bedit", &json!({ "name": "SolidBlock" })).unwrap();
    assert_eq!(s.state().unwrap().block_editor(), Some("SolidBlock"));

    // Case 1: Host drawing is R2018, but the block document being edited has its version set to R2000 (pre-2013).
    s.doc_mut().unwrap().header.set_str("ACADVER", "AC1015");

    let dxf_path = std::env::temp_dir().join(format!("cadcraft-bedit-export-dxf-{}.dxf", std::process::id()));
    let preexisting = b"untouched bedit export dxf";
    std::fs::write(&dxf_path, preexisting).unwrap();

    // Exporting DXF from inside BEDIT must use the block document's version (R2000), NOT host's (R2018),
    // and therefore reject the export and preserve the destination file:
    let err = s.execute("export", &json!({ "format": "dxf", "path": dxf_path.to_string_lossy() })).unwrap_err();
    assert!(err.to_string().contains("Cannot save to DXF 2000 without losing data"), "{}", err);
    assert_eq!(std::fs::read(&dxf_path).unwrap(), preexisting);
    let _ = std::fs::remove_file(&dxf_path);

    // Case 2: Now set block document version to R2018 (matching host or independent R2018):
    s.doc_mut().unwrap().header.set_str("ACADVER", "AC1032");
    // Also set host drawing version to R2000 to test the opposite: host is R2000, block is R2018!
    std::sync::Arc::make_mut(s.state_mut().unwrap().drawing_arc_mut()).header.set_str("ACADVER", "AC1015");
    assert_eq!(s.state().unwrap().drawing().header.get("ACADVER").and_then(cadcraft_doc::HVal::as_str), Some("AC1015"));
    assert_eq!(s.doc().unwrap().header.get("ACADVER").and_then(cadcraft_doc::HVal::as_str), Some("AC1032"));

    // Exporting DXF of the R2018 block document must succeed and NOT be falsely rejected by host's R2000 version:
    let r = s.execute("export", &json!({ "format": "dxf", "path": dxf_path.to_string_lossy() })).unwrap();
    assert_eq!(r["format"].as_str(), Some("dxf"));
    assert!(dxf_path.exists());
    let _ = std::fs::remove_file(&dxf_path);

    // Case 3: Host has an unexportable entity (AEC_CUSTOM_WALL), but block is clean:
    s.execute("bclose", &json!({ "save": false })).unwrap();
    let mut host_unexportable = Drawing::new_imperial();
    host_unexportable.add(&Space::Model, Common::default(), unknown("AEC_CUSTOM_WALL")).unwrap();
    let mut clean_b = Block::new("CleanBlock");
    clean_b.entities.push(cadcraft_engine::doc::Entity {
        handle: host_unexportable.new_handle(),
        common: Common::default(),
        kind: EntityKind::Line(cadcraft_engine::doc::Line {
            a: cadcraft_engine::geom::Vec3::new(0.0, 0.0, 0.0),
            b: cadcraft_engine::geom::Vec3::new(10.0, 10.0, 0.0),
        }),
    });
    host_unexportable.blocks.insert("CleanBlock".into(), std::sync::Arc::new(clean_b));

    let mut s2 = Session::empty();
    s2.open_drawing(host_unexportable, "host_unexportable.dxf", None);
    s2.execute("bedit", &json!({ "name": "CleanBlock" })).unwrap();

    let clean_dwg_path = std::env::temp_dir().join(format!("cadcraft-bedit-export-clean-{}.dwg", std::process::id()));
    // Exporting the clean block to DWG succeeds:
    let r_clean = s2.execute("export", &json!({ "format": "dwg", "path": clean_dwg_path.to_string_lossy() })).unwrap();
    assert_eq!(r_clean["format"].as_str(), Some("dwg"));
    assert!(clean_dwg_path.exists());
    let _ = std::fs::remove_file(&clean_dwg_path);

    // But saveas from inside BEDIT STILL rejects because it saves the host drawing:
    let err_saveas = s2.execute("saveas", &json!({ "path": clean_dwg_path.to_string_lossy() })).unwrap_err();
    assert!(err_saveas.to_string().contains("Cannot save to DWG without losing data"), "{}", err_saveas);
    assert!(!clean_dwg_path.exists());

    // Case 4: The block itself has an unexportable entity (AEC_CUSTOM_WALL):
    s2.doc_mut().unwrap().add(&Space::Model, Common::default(), unknown("AEC_CUSTOM_WALL")).unwrap();
    let preexisting_dwg = b"preexisting dwg bytes";
    std::fs::write(&clean_dwg_path, preexisting_dwg).unwrap();
    let err_block_dwg = s2.execute("export", &json!({ "format": "dwg", "path": clean_dwg_path.to_string_lossy() })).unwrap_err();
    assert!(err_block_dwg.to_string().contains("Cannot save to DWG without losing data"), "{}", err_block_dwg);
    assert_eq!(std::fs::read(&clean_dwg_path).unwrap(), preexisting_dwg);
    let _ = std::fs::remove_file(&clean_dwg_path);
}
