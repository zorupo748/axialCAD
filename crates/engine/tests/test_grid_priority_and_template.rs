//! Tests for grid priority in DXF and template loading.

use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use cadcraft_doc::Drawing;
use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use serde_json::json;

fn setup_io() {
    set_io(IoHooks {
        read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: None,
    });
}

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
    fs::create_dir_all(&dir).expect("create unique temp dir");
    dir
}

fn make_dxf_with_grid(header_grid: Option<i32>, vport_grid: Option<i32>) -> Vec<u8> {
    make_dxf_with_grid_and_limits(header_grid, vport_grid, None)
}

fn make_dxf_with_grid_and_limits(header_grid: Option<i32>, vport_grid: Option<i32>, lim_max: Option<(f64, f64)>) -> Vec<u8> {
    let mut s = String::from("0\nSECTION\n2\nHEADER\n");
    if let Some(hg) = header_grid {
        s.push_str(&format!("9\n$GRIDMODE\n70\n{}\n", hg));
    }
    if let Some((x, y)) = lim_max {
        s.push_str(&format!("9\n$LIMMAX\n10\n{}\n20\n{}\n", x, y));
    }
    s.push_str("0\nENDSEC\n0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nVPORT\n");
    if let Some(vg) = vport_grid {
        s.push_str(&format!("0\nVPORT\n2\n*ACTIVE\n70\n0\n76\n{}\n", vg));
    }
    s.push_str("0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nENDSEC\n0\nEOF\n");
    s.into_bytes()
}

#[test]
fn test_dxf_grid_priority_cases() {
    setup_io();

    // Caso 1: Solo HEADER ($GRIDMODE = 0)
    let dxf1 = make_dxf_with_grid(Some(0), None);
    let doc1 = cadcraft_io::read(&dxf1, "case1.dxf").unwrap();
    assert_eq!(doc1.header.grid_mode(), Some(false), "Caso 1: Solo HEADER 0 -> Grid OFF");

    // Caso 2: Solo VPORT *ACTIVE (76 = 0)
    let dxf2 = make_dxf_with_grid(None, Some(0));
    let doc2 = cadcraft_io::read(&dxf2, "case2.dxf").unwrap();
    assert_eq!(doc2.header.grid_mode(), Some(false), "Caso 2: Solo VPORT 0 -> Grid OFF");

    // Caso 3: Ambos coincidentes (1 y 1)
    let dxf3 = make_dxf_with_grid(Some(1), Some(1));
    let doc3 = cadcraft_io::read(&dxf3, "case3.dxf").unwrap();
    assert_eq!(doc3.header.grid_mode(), Some(true), "Caso 3: Ambos 1 -> Grid ON");

    // Caso 4: Contradictorios (HEADER = 1, VPORT = 0) -> VPORT *ACTIVE PREVALECE
    let dxf4 = make_dxf_with_grid(Some(1), Some(0));
    let doc4 = cadcraft_io::read(&dxf4, "case4.dxf").unwrap();
    assert_eq!(doc4.header.grid_mode(), Some(false), "Caso 4: VPORT 0 debe prevalecer sobre HEADER 1");

    // Caso 5: Ninguno especificado
    let dxf5 = make_dxf_with_grid(None, None);
    let doc5 = cadcraft_io::read(&dxf5, "case5.dxf").unwrap();
    assert_eq!(doc5.header.grid_mode(), None, "Caso 5: Ninguno -> None");
}

#[test]
fn test_template_loads_with_grid_off_and_limits_ref_height() {
    setup_io();
    let test_dir = unique_temp_dir("axialcad-grid-template");
    let tmp_path = test_dir.join("synthetic_template_grid_off.dwt");
    let dxf_bytes = make_dxf_with_grid_and_limits(Some(0), Some(0), Some((210.0, 297.0)));
    fs::write(&tmp_path, dxf_bytes).unwrap();

    let mut s = Session::new();
    s.execute("new", &json!({ "template": tmp_path.to_str().unwrap() })).unwrap();

    // 1. Estado en documento
    let d = s.doc().unwrap();
    assert_eq!(d.header.grid_mode(), Some(false), "La plantilla debe importar con grid_mode == Some(false)");

    // 2. Estado efectivo en Session y drafting_settings
    let ds = s.drafting_settings();
    assert_eq!(ds.gridmode, false, "drafting_settings().gridmode debe ser false al crear desde la plantilla");
    assert!((s.state().unwrap().ref_height - 297.0).abs() < 1.0, "ref_height inicial debe inicializarse al encuadre de límites (297.0)");

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_template_with_grid_on_loads_with_grid_on() {
    setup_io();
    let test_dir = unique_temp_dir("axialcad-grid-on");
    let tmp_path = test_dir.join("synthetic_template_grid_on.dxf");
    let dxf_on = make_dxf_with_grid(Some(1), Some(1));
    fs::write(&tmp_path, &dxf_on).unwrap();

    let mut s = Session::new();
    s.execute("new", &json!({ "template": tmp_path.to_str().unwrap() })).unwrap();

    let d = s.doc().unwrap();
    assert_eq!(d.header.grid_mode(), Some(true));
    assert_eq!(s.drafting_settings().gridmode, true);

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_manual_toggle_and_save_reopen_persists_grid_without_altering_snap() {
    setup_io();
    let test_dir = unique_temp_dir("axialcad-grid-toggle");
    let mut s = Session::new();
    s.open_drawing(Drawing::new_metric(), "toggle_test", None);

    // Asegurar valores iniciales conocidos
    s.settings.snapmode = true;
    s.execute("grid", &json!({ "on": false })).unwrap();

    assert_eq!(s.settings.gridmode, false);
    assert_eq!(s.settings.snapmode, true, "Cambiar GRID no debe tocar SNAPMODE");
    assert_eq!(s.doc().unwrap().header.grid_mode(), Some(false));

    // Guardar en DXF
    let tmp_dxf = test_dir.join("test_grid_persist.dxf");
    s.execute("saveas", &json!({ "path": tmp_dxf.to_str().unwrap() })).unwrap();

    // Reabrir en nueva sesión
    let mut s2 = Session::new();
    s2.execute("open", &json!({ "path": tmp_dxf.to_str().unwrap() })).unwrap();

    assert_eq!(s2.doc().unwrap().header.grid_mode(), Some(false));
    assert_eq!(s2.drafting_settings().gridmode, false);

    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_grid_sync_across_new_open_switch_close_without_inheriting_or_changing_snap() {
    setup_io();
    let mut s = Session::new();
    s.settings.snapmode = true;

    // Doc 0 (inicial): rejilla activa
    assert_eq!(s.settings.gridmode, true);
    assert_eq!(s.drafting_settings().gridmode, true);

    // Abrir Doc 1 con rejilla desactivada
    let mut d_off = Drawing::new_metric();
    d_off.header.set_grid_mode(false);
    let idx_off = s.open_drawing(d_off, "DocOff", None);
    assert_eq!(s.active, idx_off);
    assert_eq!(s.settings.gridmode, false);
    assert_eq!(s.drafting_settings().gridmode, false);
    assert_eq!(cadcraft_engine::sysvars::get(&s, "GRIDMODE"), Some(json!(0)));
    assert_eq!(s.settings.snapmode, true, "SNAPMODE no debe alterarse");

    // Cambiar de vuelta a Doc 0 mediante document.switch -> debe sincronizar gridmode = true
    s.execute("document.switch", &json!({ "index": 0 })).unwrap();
    assert_eq!(s.settings.gridmode, true);
    assert_eq!(s.drafting_settings().gridmode, true);
    assert_eq!(cadcraft_engine::sysvars::get(&s, "GRIDMODE"), Some(json!(1)));

    // Volver a Doc 1 (grid OFF) y crear un nuevo dibujo en blanco -> no debe heredar grid OFF
    s.execute("document.switch", &json!({ "index": idx_off })).unwrap();
    assert_eq!(s.settings.gridmode, false);
    let idx_new = s.new_drawing(true);
    assert_eq!(s.active, idx_new);
    assert_eq!(s.settings.gridmode, true, "Un dibujo nuevo en blanco no debe heredar gridmode=false del documento anterior");
    assert_eq!(s.drafting_settings().gridmode, true);

    // Activar Doc 1 (grid OFF) y cerrarlo -> el documento que queda activo (idx_new) tiene grid ON
    s.execute("document.switch", &json!({ "index": idx_off })).unwrap();
    assert_eq!(s.settings.gridmode, false);
    s.execute("close", &json!({ "index": idx_off })).unwrap();
    assert_eq!(s.settings.gridmode, true, "Cerrar el documento sin rejilla debe sincronizar el estado con el documento que queda activo");
    assert_eq!(s.drafting_settings().gridmode, true);
    assert_eq!(s.settings.snapmode, true);
}
