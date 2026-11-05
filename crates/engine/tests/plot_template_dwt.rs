//! Regression and verification test: reading Templates/EGI-ISO.dwt, validating its
//! page setups ("Trazador PDF.pc3", 0 mm margins, portrait/landscape, EGI.stb),
//! and plotting with explicit PDF device equivalence using cadcraft_io.

use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use serde_json::json;

#[test]
fn real_dwt_template_plot_with_cadcraft_io() {
    set_io(IoHooks {
        read: |b, name| cadcraft_io::read(b, name).map_err(|e| e.to_string()),
        write: |d, name| cadcraft_io::write(d, name).map_err(|e| e.to_string()),
        plot: Some(|d, space, opts| cadcraft_io::plotter::plot(d, space, opts).map_err(|e| e.to_string())),
    });

    let template_path = "Templates/EGI-ISO.dwt";
    let orig_mtime = std::fs::metadata(template_path).unwrap().modified().unwrap();

    let mut s = Session::new();
    s.execute("open", &json!({ "path": template_path })).unwrap();

    // 1. Formato A4 vertical
    s.execute("layout.set", &json!({ "name": "Formato A4 vertical" })).unwrap();
    let page_v = s.doc().unwrap().layout("Formato A4 vertical").unwrap().page.clone();
    assert_eq!(page_v.device, "Trazador PDF.pc3");
    assert_eq!(page_v.landscape, false);
    assert_eq!(page_v.margins_mm, [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(page_v.width_mm, 210.0);
    assert_eq!(page_v.height_mm, 297.0);

    let path_v = std::env::temp_dir().join("cadcraft_test_real_v.pdf").to_string_lossy().to_string();
    let r_v = s
        .execute(
            "plot",
            &json!({
                "layout": "Formato A4 vertical",
                "device": "Trazador PDF.pc3",
                "deviceEquivalent": "pdf",
                "path": path_v,
            }),
        )
        .unwrap();
    assert!(r_v["warning"].as_str().unwrap().contains("EGI.stb"));
    let bytes_v = std::fs::read(&path_v).unwrap();
    let text_v = String::from_utf8_lossy(&bytes_v);
    assert!(text_v.contains("/MediaBox [0 0 595.276 841.89]"), "Must have 210 x 297 mm portrait MediaBox");
    let _ = std::fs::remove_file(&path_v);

    // 2. Formato A4 horizontal
    s.execute("layout.set", &json!({ "name": "Formato A4 horizontal" })).unwrap();
    let page_h = s.doc().unwrap().layout("Formato A4 horizontal").unwrap().page.clone();
    assert_eq!(page_h.device, "Trazador PDF.pc3");
    assert_eq!(page_h.landscape, true);
    assert_eq!(page_h.margins_mm, [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(page_h.width_mm, 210.0);
    assert_eq!(page_h.height_mm, 297.0);

    let path_h = std::env::temp_dir().join("cadcraft_test_real_h.pdf").to_string_lossy().to_string();
    let _r_h = s
        .execute(
            "plot",
            &json!({
                "layout": "Formato A4 horizontal",
                "device": "Trazador PDF.pc3",
                "deviceEquivalent": "pdf",
                "path": path_h,
            }),
        )
        .unwrap();
    let bytes_h = std::fs::read(&path_h).unwrap();
    let text_h = String::from_utf8_lossy(&bytes_h);
    assert!(text_h.contains("/MediaBox [0 0 841.89 595.276]"), "Must have 297 x 210 mm landscape MediaBox");
    let _ = std::fs::remove_file(&path_h);

    // Verify template file was never modified
    let new_mtime = std::fs::metadata(template_path).unwrap().modified().unwrap();
    assert_eq!(orig_mtime, new_mtime, "Templates/EGI-ISO.dwt must never be altered");
}
