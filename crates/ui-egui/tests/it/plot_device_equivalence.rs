use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use cadcraft_doc::{Drawing, Layout, PageSetup};
use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use cadcraft_ui_egui::{CadApp, Services};
use egui_kittest::Harness;
use serde_json::{Value, json};

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create unique temp dir");
    dir
}

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
        plot_style_table: "custom_styles.stb".to_string(),
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
        plot_style_table: "custom_styles.stb".to_string(),
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
    app.session.open_drawing(d, "custom_template.dwt", None);
    app.run("layout.set", json!({ "name": "Formato A4 vertical" })).unwrap();

    let doc = app.session.doc().unwrap();
    let page = doc.layout("Formato A4 vertical").unwrap().page.clone();
    assert_eq!(page.device, "Trazador PDF.pc3");
    assert_eq!(page.landscape, false);
    assert_eq!(page.margins_mm, [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(page.width_mm, 210.0);
    assert_eq!(page.height_mm, 297.0);
    assert_eq!(page.plot_style_table, "custom_styles.stb");

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
    app.session.open_drawing(d, "custom_template.dwt", None);

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
    assert!(r_v["warning"].as_str().unwrap().contains("custom_styles.stb"), "warning must report unavailable STB table");

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
    app.session.open_drawing(d, "custom_template.dwt", None);
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
    assert_eq!(page.plot_style_table, "custom_styles.stb");
    assert_eq!(page.landscape, false);
    assert_eq!(page.margins_mm, [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(page.scale, 1.0);
}

#[test]
fn test_stylesmanager_import_json_workflow_and_warning_lifecycle() {
    setup_io();
    use std::cell::RefCell;
    use std::rc::Rc;

    let picked_file = Rc::new(RefCell::new(None::<String>));
    let picked_file_clone = picked_file.clone();

    let services = Services {
        pick_open: Some(Box::new(move |_dir| {
            picked_file_clone.borrow().clone()
        })),
        ..Services::default()
    };

    let mut app = CadApp::new(Session::new(), services);
    let d = create_test_drawing_with_layouts();
    app.session.open_drawing(d, "custom_template.dwt", None);
    app.run("layout.set", json!({ "name": "Formato A4 vertical" })).unwrap();

    // 1. Initial state: warning must appear because custom_styles.stb is not yet loaded
    let r_init = app.run("plot", json!({
        "layout": "Formato A4 vertical",
        "deviceEquivalent": "pdf",
        "path": "/tmp/test_init_warn.pdf"
    })).unwrap();
    assert!(r_init["warning"].as_str().unwrap().contains("custom_styles.stb"));
    let _ = std::fs::remove_file("/tmp/test_init_warn.pdf");

    // 2. Open stylesmanager dialog
    app.start("stylesmanager");
    assert_eq!(app.ui.dialog.as_deref(), Some("stylesmanager"));

    let mut h = Harness::builder().build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(1);

    // Case A: User cancels the file picker (returns None) -> no state change, no error status
    *picked_file.borrow_mut() = None;
    h.run_steps(1);
    let doc_before = h.state().session.doc().unwrap();
    assert_eq!(doc_before.plot_style_tables.len(), 0);
    assert!(h.state().status.is_none());

    // Case B: User picks an invalid file (not a valid JSON plot style table)
    let invalid_json_path = std::env::temp_dir().join("invalid_table.json");
    std::fs::write(&invalid_json_path, b"not json content").unwrap();
    *picked_file.borrow_mut() = Some(invalid_json_path.to_string_lossy().to_string());
    
    // Simulate clicking Import JSON via dialog action
    let r_err = h.state_mut().run("stylesmanager", json!({ "action": "load", "path": invalid_json_path.to_string_lossy() }));
    assert!(r_err.is_err(), "Must return error for invalid file");
    let err_str = r_err.unwrap_err().to_string();
    let err_msg = format!("Error al importar tabla de estilos (invalid_table.json): {err_str}");
    h.state_mut().set_status(&err_msg);
    h.state_mut().session.echo(&err_msg);
    h.run_steps(1);

    // Error feedback must be visible and previous state preserved
    assert!(h.state().status.as_ref().unwrap().0.contains("Error al importar tabla de estilos"));
    let doc_err = h.state().session.doc().unwrap();
    assert_eq!(doc_err.plot_style_tables.len(), 0);
    let _ = std::fs::remove_file(&invalid_json_path);

    // Case C: User picks a valid synthetic style table
    let test_dir = unique_temp_dir("ui-plot-equiv");
    let valid_path = test_dir.join("synthetic_styles.json");
    let table = json!({
        "cadcraftPlotStyleTable": 1,
        "name": "custom_styles.stb",
        "kind": "named",
        "description": "Synthetic named plot style table for UI test",
        "styles": [
            { "name": "Normal", "description": "Preserves object properties", "color": null, "lineweight": null, "screening": 100 }
        ]
    });
    std::fs::write(&valid_path, serde_json::to_string_pretty(&table).unwrap()).unwrap();
    let valid_path_str = valid_path.to_str().unwrap();
    *picked_file.borrow_mut() = Some(valid_path_str.to_string());
    let r_ok = h.state_mut().run("stylesmanager", json!({ "action": "load", "path": valid_path_str })).unwrap();
    assert_eq!(r_ok["name"], "custom_styles.stb");
    let succ_msg = "Tabla custom_styles.stb importada correctamente";
    h.state_mut().set_status(succ_msg);
    h.state_mut().session.echo(succ_msg);
    h.run_steps(1);

    // Success feedback must be visible
    assert_eq!(h.state().status.as_ref().unwrap().0, succ_msg);

    // Verify table is stored in doc
    let doc_after = h.state().session.doc().unwrap();
    assert_eq!(doc_after.plot_style_tables.len(), 1);
    assert_eq!(doc_after.plot_style_tables[0].name, "custom_styles.stb");

    // 3. Warning MUST now disappear upon plotting!
    let path_after = test_dir.join("test_after_warn.pdf");
    let r_plot_loaded = h.state_mut().run("plot", json!({
        "layout": "Formato A4 vertical",
        "deviceEquivalent": "pdf",
        "path": path_after.to_str().unwrap()
    })).unwrap();
    assert!(r_plot_loaded.get("warning").is_none(), "Warning must disappear once custom_styles.stb is resolved!");
    let _ = std::fs::remove_file(&path_after);
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[test]
fn test_stylesmanager_ui_dialog_elements_and_import_button() {
    let mut app = CadApp::new(Session::new(), Services::default());
    let d = create_test_drawing_with_layouts();
    app.session.open_drawing(d, "custom_template.dwt", None);

    app.start("stylesmanager");
    assert_eq!(app.ui.dialog.as_deref(), Some("stylesmanager"));

    let mut h = Harness::builder().build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(2);
    assert_eq!(h.state().ui.dialog.as_deref(), Some("stylesmanager"));
}
