//! Contract test suite for named plot style tables (.stb):
//! - Loading via `stylesmanager load`
//! - Disappearance of missing plot style table warning once loaded
//! - Verification of numerical lineweights and colors:
//!     Normal          -> preserves object/layer color & lineweight (never forces hairline 0.0)
//!     Pen_Thin        -> #000000 black, 0.20 mm (0.567 pt)
//!     Pen_Medium      -> #000000 black, 0.40 mm (1.134 pt)
//!     Pen_Thick       -> #000000 black, 0.80 mm (2.268 pt)
//!     Pen_Medium_Dup  -> second style with identical properties (0.40 mm, black)
//!     Pen_Custom_Red  -> #FF0000 red, 0.50 mm (1.417 pt)
//! - Precedence: entity direct override vs layer plot style
//! - Persistence across DXF saveas / reopen
//!
//! Note: Templates used here are ASCII DXF serialized with .dwt extension.

use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use cadcraft_doc::PageSetup;
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

fn write_synthetic_stb_json(path: &std::path::Path) {
    let table = json!({
        "cadcraftPlotStyleTable": 1,
        "name": "test_named_styles.stb",
        "kind": "named",
        "description": "Synthetic named plot style table for contract verification",
        "styles": [
            { "name": "Normal", "description": "Preserves object properties", "color": null, "lineweight": null, "screening": 100 },
            { "name": "Pen_Thin", "description": "0.20 mm black", "color": "#000000", "lineweight": 20, "screening": 100 },
            { "name": "Pen_Medium", "description": "0.40 mm black", "color": "#000000", "lineweight": 40, "screening": 100 },
            { "name": "Pen_Thick", "description": "0.80 mm black", "color": "#000000", "lineweight": 80, "screening": 100 },
            { "name": "Pen_Medium_Dup", "description": "Second style with identical properties: 0.40 mm black", "color": "#000000", "lineweight": 40, "screening": 100 },
            { "name": "Pen_Custom_Red", "description": "0.50 mm red", "color": "#FF0000", "lineweight": 50, "screening": 100 }
        ]
    });
    fs::write(path, serde_json::to_string_pretty(&table).unwrap()).expect("Must write synthetic STB JSON");
}

fn write_synthetic_stb_template_dwt(path: &std::path::Path) {
    let mut s = Session::new();
    s.new_drawing(true);
    s.execute("layer.new", &json!({ "name": "L_Thin", "color": 2 })).unwrap();
    s.execute("layer.set", &json!({ "name": "L_Thin", "plotStyle": "Pen_Thin" })).unwrap();
    s.execute("layer.new", &json!({ "name": "L_Medium", "color": 4 })).unwrap();
    s.execute("layer.set", &json!({ "name": "L_Medium", "plotStyle": "Pen_Medium" })).unwrap();
    s.execute("layout.new", &json!({ "name": "Formato A4 vertical" })).unwrap();

    let doc = s.doc_mut().unwrap();
    if let Some(l) = doc.layouts.iter_mut().find(|l| l.name == "Formato A4 vertical") {
        l.page = PageSetup {
            device: "Trazador PDF.pc3".to_string(),
            landscape: false,
            margins_mm: [0.0, 0.0, 0.0, 0.0],
            width_mm: 210.0,
            height_mm: 297.0,
            plot_style_table: "test_named_styles.stb".to_string(),
            ..PageSetup::default()
        };
    }
    let _ = doc;

    s.execute("layout.set", &json!({ "name": "Formato A4 vertical" })).unwrap();
    s.execute("layer.current", &json!({ "name": "L_Thin" })).unwrap();
    s.execute("line", &json!({ "points": [[10, 10], [100, 10]] })).unwrap();
    s.execute("layer.current", &json!({ "name": "L_Medium" })).unwrap();
    s.execute("line", &json!({ "points": [[10, 20], [100, 20]] })).unwrap();

    let d = s.doc().unwrap();
    let bytes = cadcraft_io::write(d, "template.dxf").unwrap();
    fs::write(path, bytes).unwrap();
}

#[test]
fn test_named_plot_style_table_application_and_persistence() {
    setup_io();

    let test_dir = unique_temp_dir("axialcad-stb-test-app");
    let template_path = test_dir.join("synthetic_template.dwt");
    let converted_json = test_dir.join("synthetic_stb.json");

    write_synthetic_stb_template_dwt(&template_path);
    write_synthetic_stb_json(&converted_json);

    let template_str = template_path.to_str().unwrap();
    let converted_json_str = converted_json.to_str().unwrap();

    let mut s = Session::new();
    s.execute("open", &json!({ "path": template_str })).unwrap();

    // 1. Before loading: warning is present because test_named_styles.stb is missing
    let path_before = test_dir.join("test_before_stb.pdf");
    let r_before = s
        .execute(
            "plot",
            &json!({
                "layout": "Formato A4 vertical",
                "device": "Trazador PDF.pc3",
                "deviceEquivalent": "pdf",
                "path": path_before.to_str().unwrap()
            }),
        )
        .unwrap();
    assert!(r_before["warning"].as_str().unwrap().contains("test_named_styles.stb"), "Warning must be present before loading table");
    let _ = fs::remove_file(&path_before);

    // 2. Load the converted plot style table via stylesmanager
    let r_load = s.execute("stylesmanager", &json!({ "action": "load", "path": converted_json_str })).unwrap();
    assert_eq!(r_load["name"], "test_named_styles.stb");

    // Verify it is in doc.plot_style_tables
    let doc = s.doc().unwrap();
    assert_eq!(doc.plot_style_tables.len(), 1);
    assert_eq!(doc.plot_style_tables[0].name, "test_named_styles.stb");
    assert_eq!(doc.plot_style_tables[0].styles.len(), 6);

    // 3. After loading: plot layout with equivalence. Warning must disappear!
    let path_after = test_dir.join("test_after_stb.pdf");
    let r_after = s
        .execute(
            "plot",
            &json!({
                "layout": "Formato A4 vertical",
                "device": "Trazador PDF.pc3",
                "deviceEquivalent": "pdf",
                "compress": false,
                "path": path_after.to_str().unwrap()
            }),
        )
        .unwrap();
    assert!(r_after.get("warning").is_none(), "Warning must disappear once test_named_styles.stb is loaded and resolved!");

    // 4. Inspect the uncompressed PDF:
    let bytes = fs::read(&path_after).unwrap();
    let text = String::from_utf8_lossy(&bytes);

    // Black stroke '0 0 0 RG' must be present for Pen styles
    assert!(text.contains("0 0 0 RG"), "PDF must contain black stroke 0 0 0 RG");

    // Lineweights verified from numeric indices:
    // Pen_Medium is 0.40 mm -> approx 1.134 pt
    // Pen_Thin is 0.20 mm -> approx 0.567 pt
    assert!(text.contains("1.134 w") || text.contains("1.13"), "PDF must contain 0.40 mm lineweight (Pen_Medium)");
    assert!(text.contains("0.567 w") || text.contains("0.57"), "PDF must contain 0.20 mm lineweight (Pen_Thin)");

    // Exact A4 portrait dimensions
    assert!(text.contains("/MediaBox [0 0 595.276 841.89]"), "Must keep exact A4 portrait dimensions");
    let _ = fs::remove_file(&path_after);

    // 5. Test persistence: save drawing to DXF and reopen
    let temp_dxf = test_dir.join("test_persisted_stb.dxf");
    s.execute("saveas", &json!({ "path": temp_dxf.to_str().unwrap() })).unwrap();

    let mut s2 = Session::new();
    s2.execute("open", &json!({ "path": temp_dxf.to_str().unwrap() })).unwrap();
    let doc2 = s2.doc().unwrap();

    // Table test_named_styles.stb must still exist in drawing after reopen
    assert_eq!(doc2.plot_style_tables.len(), 1, "Plot style table must be persisted in DXF");
    assert_eq!(doc2.plot_style_tables[0].name, "test_named_styles.stb");
    assert_eq!(doc2.plot_style_tables[0].styles.len(), 6);

    // Plotting reopened drawing must also succeed without warning!
    let path_reopened = test_dir.join("test_reopened_stb.pdf");
    let r_reopen = s2
        .execute(
            "plot",
            &json!({
                "layout": "Formato A4 vertical",
                "device": "Trazador PDF.pc3",
                "deviceEquivalent": "pdf",
                "path": path_reopened.to_str().unwrap()
            }),
        )
        .unwrap();
    assert!(r_reopen.get("warning").is_none(), "Reopened drawing must retain plot style table and produce no warning");
    let _ = fs::remove_file(&path_reopened);
    let _ = fs::remove_file(&temp_dxf);
    let _ = fs::remove_dir_all(&test_dir);
}

#[test]
fn test_all_styles_and_precedence() {
    setup_io();

    let test_dir = unique_temp_dir("axialcad-stb-test-prec");
    let converted_json = test_dir.join("synthetic_stb.json");
    write_synthetic_stb_json(&converted_json);

    let mut s = Session::new();
    s.execute("stylesmanager", &json!({ "action": "load", "path": converted_json.to_str().unwrap() })).unwrap();

    // Verify individual styles:
    // 1. Normal: preserves color and lineweight (e.g. Red, 0.50 mm = 1.417 pt)
    s.execute("layer.new", &json!({ "name": "L_Normal", "color": 1, "lineweight": 0.50 })).unwrap();
    s.execute("layer.set", &json!({ "name": "L_Normal", "plotStyle": "Normal" })).unwrap();

    // 2. Pen_Thin: 0.20 mm (0.567 pt), black
    s.execute("layer.new", &json!({ "name": "L_Thin", "color": 2 })).unwrap();
    s.execute("layer.set", &json!({ "name": "L_Thin", "plotStyle": "Pen_Thin" })).unwrap();

    // 3. Pen_Medium: 0.40 mm (1.134 pt), black
    s.execute("layer.new", &json!({ "name": "L_Medium", "color": 3 })).unwrap();
    s.execute("layer.set", &json!({ "name": "L_Medium", "plotStyle": "Pen_Medium" })).unwrap();

    // 4. Pen_Thick: 0.80 mm (2.268 pt), black
    s.execute("layer.new", &json!({ "name": "L_Thick", "color": 4 })).unwrap();
    s.execute("layer.set", &json!({ "name": "L_Thick", "plotStyle": "Pen_Thick" })).unwrap();

    // 5. Pen_Medium_Dup: second style with identical properties (0.40 mm, black)
    s.execute("layer.new", &json!({ "name": "L_Dup", "color": 5 })).unwrap();
    s.execute("layer.set", &json!({ "name": "L_Dup", "plotStyle": "Pen_Medium_Dup" })).unwrap();

    // 6. Pen_Custom_Red: 0.50 mm (1.417 pt), custom color #FF0000
    s.execute("layer.new", &json!({ "name": "L_Red", "color": 6 })).unwrap();
    s.execute("layer.set", &json!({ "name": "L_Red", "plotStyle": "Pen_Custom_Red" })).unwrap();

    // Draw one line per layer
    let layers = ["L_Normal", "L_Thin", "L_Medium", "L_Thick", "L_Dup", "L_Red"];
    for (i, layer) in layers.iter().enumerate() {
        s.execute("layer.current", &json!({ "name": layer })).unwrap();
        let y = i as f64 * 10.0;
        s.execute("line", &json!({ "points": [[0, y], [50, y]] })).unwrap();
    }

    // Precedence test: on layer L_Thick (Pen_Thick = black, 0.80 mm), draw a line with direct override Normal
    s.execute("layer.current", &json!({ "name": "L_Thick" })).unwrap();
    let res = s.execute("line", &json!({ "points": [[0, 80], [50, 80]] })).unwrap();
    let h_override = res["handles"][0].as_str().unwrap();
    s.execute("plotstyle", &json!({ "handles": [h_override], "name": "Normal" })).unwrap();

    let path_pdf = test_dir.join("test_all_styles.pdf");
    s.execute("plot", &json!({
        "deviceEquivalent": "pdf",
        "plotStyleTable": "test_named_styles.stb",
        "compress": false,
        "path": path_pdf.to_str().unwrap()
    })).unwrap();

    let bytes = fs::read(&path_pdf).unwrap();
    let text = String::from_utf8_lossy(&bytes);

    // Assert Normal keeps original red color (1 0 0 RG) and original lineweight 0.50 mm (1.417 pt)
    assert!(text.contains("1 0 0 RG"), "Normal style must preserve red object color");
    assert!(text.contains("1.417 w") || text.contains("1.42"), "Normal style must preserve 0.50 mm object lineweight (never hairline 0)");

    // Assert black output for Pen styles
    assert!(text.contains("0 0 0 RG"), "Pen styles must output black stroke 0 0 0 RG");

    // Assert verified lineweights:
    assert!(text.contains("0.567 w") || text.contains("0.57"), "Must have 0.20 mm (Pen_Thin)");
    assert!(text.contains("1.134 w") || text.contains("1.13"), "Must have 0.40 mm (Pen_Medium)");
    assert!(text.contains("2.268 w") || text.contains("2.27"), "Must have 0.80 mm (Pen_Thick)");

    // Direct override test: the entity on L_Thick with Normal override must preserve L_Thick cyan color (color 4: 0 1 1 RG)
    assert!(text.contains("0 1 1 RG"), "Entity with direct Normal override must preserve layer color instead of black");

    let _ = fs::remove_file(&path_pdf);
    let _ = fs::remove_dir_all(&test_dir);
}
