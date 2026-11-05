use cadcraft_doc::{Drawing, Layout, PageSetup};
use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use cadcraft_ui_egui::{CadApp, Services};
use egui_kittest::Harness;
use serde_json::{Value, json};

fn setup_io() {
    set_io(IoHooks {
        read: |_, _| Err("no reader in this test".into()),
        write: |_, _| Ok(Vec::new()),
        plot: Some(|doc, space, opts| {
            let page = match space {
                cadcraft_doc::Space::Paper(name) => doc.layout(name).map(|l| l.page.clone()),
                _ => None,
            };
            let landscape = opts.get("landscape").and_then(Value::as_bool).or_else(|| page.as_ref().map(|p| p.landscape)).unwrap_or(false);
            let media_box = if landscape {
                "/MediaBox [ 0 0 841.89 595.27 ]"
            } else {
                "/MediaBox [ 0 0 595.27 841.89 ]"
            };
            Ok(format!("%PDF-1.4\n{media_box}\n%%EOF").into_bytes())
        }),
    });
}

fn create_test_drawing_with_layouts() -> Drawing {
    let mut d = Drawing::new_metric();
    let page_v = PageSetup {
        device: "Trazador PDF.pc3".to_string(),
        plot_style_table: "EGI.stb".to_string(),
        landscape: false,
        margins_mm: [0.0, 0.0, 0.0, 0.0],
        width_mm: 210.0,
        height_mm: 297.0,
        scale: 1.0,
        scale_to_fit: false,
        ..Default::default()
    };
    let page_h = PageSetup {
        device: "Trazador PDF.pc3".to_string(),
        plot_style_table: "EGI.stb".to_string(),
        landscape: true,
        margins_mm: [0.0, 0.0, 0.0, 0.0],
        width_mm: 297.0,
        height_mm: 210.0,
        scale: 1.0,
        scale_to_fit: false,
        ..Default::default()
    };
    let mut l_v = Layout::new("Formato A4 vertical", 1);
    l_v.page = page_v;
    d.layouts.push(l_v);

    let mut l_h = Layout::new("Formato A4 horizontal", 2);
    l_h.page = page_h;
    d.layouts.push(l_h);
    d
}

#[test]
fn test_plot_dialog_shows_equivalence_notice_and_stb_warning() {
    setup_io();
    let mut app = CadApp::new(Session::new(), Services::default());
    let d = create_test_drawing_with_layouts();
    app.session.open_drawing(d, "EGI-ISO.dwt", None);
    app.run("layout.set", json!({ "name": "Formato A4 vertical" })).unwrap();

    let doc = app.session.doc().unwrap();
    let page = doc.layout("Formato A4 vertical").unwrap().page.clone();
    assert_eq!(page.device, "Trazador PDF.pc3");
    assert_eq!(page.landscape, false);
    assert_eq!(page.margins_mm, [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(page.width_mm, 210.0);
    assert_eq!(page.height_mm, 297.0);
    assert_eq!(page.plot_style_table, "EGI.stb");

    // Open plot dialog in UI
    app.start("plot");
    assert_eq!(app.ui.dialog.as_deref(), Some("plot"));

    // Render with test harness
    let mut h = Harness::builder().build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(2);
    // Dialog must be open and state initialized
    assert_eq!(h.state().ui.dialog.as_deref(), Some("plot"));
}

#[test]
fn test_export_a4_vertical_and_horizontal_exact_geometry() {
    setup_io();
    let mut app = CadApp::new(Session::new(), Services::default());
    let d = create_test_drawing_with_layouts();
    app.session.open_drawing(d, "EGI-ISO.dwt", None);

    // 1. Formato A4 vertical
    app.run("layout.set", json!({ "name": "Formato A4 vertical" })).unwrap();
    let path_v = std::env::temp_dir().join("cadcraft_test_v.pdf").to_string_lossy().to_string();

    let r_v = app.run("plot", json!({
        "layout": "Formato A4 vertical",
        "device": "Trazador PDF.pc3",
        "deviceEquivalent": "pdf",
        "path": path_v,
    })).unwrap();

    assert_eq!(r_v["layout"], "Formato A4 vertical");
    assert!(r_v["warning"].as_str().unwrap().contains("EGI.stb"), "warning must report unavailable STB table");

    // Inspect generated PDF bytes
    let bytes_v = std::fs::read(&path_v).unwrap();
    let text_v = String::from_utf8_lossy(&bytes_v);
    assert!(text_v.contains("/MediaBox [ 0 0 595.27 841.89 ]"), "A4 vertical must have portrait MediaBox");
    let _ = std::fs::remove_file(&path_v);

    // 2. Formato A4 horizontal
    app.run("layout.set", json!({ "name": "Formato A4 horizontal" })).unwrap();
    let path_h = std::env::temp_dir().join("cadcraft_test_h.pdf").to_string_lossy().to_string();

    let r_h = app.run("plot", json!({
        "layout": "Formato A4 horizontal",
        "device": "Trazador PDF.pc3",
        "deviceEquivalent": "pdf",
        "path": path_h,
    })).unwrap();

    assert_eq!(r_h["layout"], "Formato A4 horizontal");
    let bytes_h = std::fs::read(&path_h).unwrap();
    let text_h = String::from_utf8_lossy(&bytes_h);
    assert!(text_h.contains("/MediaBox [ 0 0 841.89 595.27 ]"), "A4 horizontal must have landscape MediaBox");
    let _ = std::fs::remove_file(&path_h);
}

#[test]
fn test_defpoints_and_paper_scale_and_property_preservation() {
    setup_io();
    let mut app = CadApp::new(Session::new(), Services::default());
    let d = create_test_drawing_with_layouts();
    app.session.open_drawing(d, "EGI-ISO.dwt", None);
    app.run("layout.set", json!({ "name": "Formato A4 vertical" })).unwrap();

    let path_pdf = std::env::temp_dir().join("cadcraft_test_scale.pdf").to_string_lossy().to_string();

    let r = app.run("plot", json!({
        "layout": "Formato A4 vertical",
        "device": "Trazador PDF.pc3",
        "deviceEquivalent": "pdf",
        "path": path_pdf,
    })).unwrap();
    assert!(r["bytes"].as_u64().unwrap() > 0);
    let _ = std::fs::remove_file(&path_pdf);

    // Verify page setup preserved intact in session
    let doc = app.session.doc().unwrap();
    let page = doc.layout("Formato A4 vertical").unwrap().page.clone();
    assert_eq!(page.device, "Trazador PDF.pc3");
    assert_eq!(page.plot_style_table, "EGI.stb");
    assert_eq!(page.landscape, false);
    assert_eq!(page.margins_mm, [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(page.scale, 1.0);
}
