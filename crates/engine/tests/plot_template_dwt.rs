//! Contract and verification test: reading template files (.dwt in DXF format),
//! validating page setups (device name, 0 mm margins, portrait/landscape, plot style table),
//! and plotting with explicit PDF device equivalence using cadcraft_io.
//!
//! Note on DXF/DWT format:
//! The template fixture used here is an ASCII DXF serialized with .dwt extension.
//! This verifies axialCAD template loading, layout resolution, and PDF output without
//! relying on external proprietary AutoCAD binary DWT files.

use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use cadcraft_doc::{Drawing, Layout, PageSetup};
use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use serde_json::json;

fn setup_io() {
    set_io(IoHooks {
        read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: Some(|d, space, opts| cadcraft_io::plotter::plot(d, space, opts).map_err(|e| e.to_string())),
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

fn make_synthetic_template_dwt(path: &std::path::Path) {
    let mut d = Drawing::new_metric();

    let mut l_v = Layout::new("Formato A4 vertical", 1);
    l_v.page = PageSetup {
        device: "Trazador PDF.pc3".to_string(),
        landscape: false,
        margins_mm: [0.0, 0.0, 0.0, 0.0],
        width_mm: 210.0,
        height_mm: 297.0,
        plot_style_table: "test_named_styles.stb".to_string(),
        ..PageSetup::default()
    };

    let mut l_h = Layout::new("Formato A4 horizontal", 2);
    l_h.page = PageSetup {
        device: "Trazador PDF.pc3".to_string(),
        landscape: true,
        margins_mm: [0.0, 0.0, 0.0, 0.0],
        width_mm: 210.0,
        height_mm: 297.0,
        plot_style_table: "test_named_styles.stb".to_string(),
        ..PageSetup::default()
    };

    d.layouts.push(l_v);
    d.layouts.push(l_h);

    let bytes = cadcraft_io::write(&d, "template.dxf").expect("Must serialize synthetic template");
    fs::write(path, bytes).expect("Must write synthetic template .dwt");
}

#[test]
fn synthetic_dwt_template_layout_plot() {
    setup_io();

    let test_dir = unique_temp_dir("axialcad-plot-template-dwt");
    let template_path = test_dir.join("synthetic_template.dwt");
    make_synthetic_template_dwt(&template_path);

    let template_str = template_path.to_str().expect("valid path");
    let orig_mtime = fs::metadata(&template_path).unwrap().modified().unwrap();

    let mut s = Session::new();
    s.execute("open", &json!({ "path": template_str })).unwrap();

    // 1. Formato A4 vertical
    s.execute("layout.set", &json!({ "name": "Formato A4 vertical" })).unwrap();
    let doc = s.doc().unwrap();
    let page_v = doc.layout("Formato A4 vertical").unwrap().page.clone();
    assert_eq!(page_v.device, "Trazador PDF.pc3");
    assert_eq!(page_v.landscape, false);
    assert_eq!(page_v.margins_mm, [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(page_v.width_mm, 210.0);
    assert_eq!(page_v.height_mm, 297.0);

    let path_v = test_dir.join("cadcraft_test_real_v.pdf");
    let path_v_str = path_v.to_str().unwrap();
    let r_v = s
        .execute(
            "plot",
            &json!({
                "layout": "Formato A4 vertical",
                "device": "Trazador PDF.pc3",
                "deviceEquivalent": "pdf",
                "path": path_v_str,
            }),
        )
        .unwrap();
    assert!(r_v["warning"].as_str().unwrap().contains("test_named_styles.stb"));
    let bytes_v = fs::read(&path_v).unwrap();
    let text_v = String::from_utf8_lossy(&bytes_v);
    assert!(text_v.contains("/MediaBox [0 0 595.276 841.89]"), "Must have 210 x 297 mm portrait MediaBox");
    let _ = fs::remove_file(&path_v);

    // 2. Formato A4 horizontal
    s.execute("layout.set", &json!({ "name": "Formato A4 horizontal" })).unwrap();
    let doc = s.doc().unwrap();
    let page_h = doc.layout("Formato A4 horizontal").unwrap().page.clone();
    assert_eq!(page_h.device, "Trazador PDF.pc3");
    assert_eq!(page_h.landscape, true);
    assert_eq!(page_h.margins_mm, [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(page_h.width_mm, 210.0);
    assert_eq!(page_h.height_mm, 297.0);

    let path_h = test_dir.join("cadcraft_test_real_h.pdf");
    let path_h_str = path_h.to_str().unwrap();
    let _r_h = s
        .execute(
            "plot",
            &json!({
                "layout": "Formato A4 horizontal",
                "device": "Trazador PDF.pc3",
                "deviceEquivalent": "pdf",
                "path": path_h_str,
            }),
        )
        .unwrap();
    let bytes_h = fs::read(&path_h).unwrap();
    let text_h = String::from_utf8_lossy(&bytes_h);
    assert!(text_h.contains("/MediaBox [0 0 841.89 595.276]"), "Must have 297 x 210 mm landscape MediaBox");
    let _ = fs::remove_file(&path_h);

    // Verify template file was never modified
    let new_mtime = fs::metadata(&template_path).unwrap().modified().unwrap();
    assert_eq!(orig_mtime, new_mtime, "Template file must never be altered");

    let _ = fs::remove_dir_all(&test_dir);
}
