//! Comprehensive test suite for the verified EGI.stb conversion:
//! - Loading via `stylesmanager load`
//! - Disappearance of missing plot style table warning once loaded
//! - Verified numerical lineweights:
//!     Normal   -> preserves object color & lineweight (never forces hairline 0.0)
//!     UNE 0.18 -> #000000 black, 0.13 mm (0.369 pt)
//!     UNE 0.25 -> #000000 black, 0.30 mm (0.850 pt)
//!     UNE 0.35 -> #000000 black, 0.40 mm (1.134 pt)
//!     UNE 0.50 -> #000000 black, 0.53 mm (1.502 pt)
//!     UNE 0.70 -> #000000 black, 0.80 mm (2.268 pt)
//!     Aliases (UNE_0.18 .. UNE_0.70)
//! - Precedence: entity direct override vs layer plot style
//! - Persistence across DXF saveas / reopen

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

#[test]
fn test_egi_converted_plot_style_application_and_persistence() {
    setup_io();

    let template_path = "Templates/EGI-ISO.dwt";
    let converted_json = "Templates/EGI.converted.plotstyles.json";

    let mut s = Session::new();
    s.execute("open", &json!({ "path": template_path })).unwrap();

    // 1. Before loading: warning is present because EGI.stb is missing
    let r_before = s
        .execute(
            "plot",
            &json!({
                "layout": "Formato A4 vertical",
                "device": "Trazador PDF.pc3",
                "deviceEquivalent": "pdf",
                "path": "/tmp/test_before_stb.pdf"
            }),
        )
        .unwrap();
    assert!(r_before["warning"].as_str().unwrap().contains("EGI.stb"), "Warning must be present before loading table");
    let _ = std::fs::remove_file("/tmp/test_before_stb.pdf");

    // 2. Load the converted plot style table via stylesmanager
    let r_load = s.execute("stylesmanager", &json!({ "action": "load", "path": converted_json })).unwrap();
    assert_eq!(r_load["name"], "EGI.stb");

    // Verify it is in doc.plot_style_tables
    let doc = s.doc().unwrap();
    assert_eq!(doc.plot_style_tables.len(), 1);
    assert_eq!(doc.plot_style_tables[0].name, "EGI.stb");
    assert_eq!(doc.plot_style_tables[0].styles.len(), 11);

    // 3. After loading: plot layout with equivalence. Warning must disappear!
    let path_after = std::env::temp_dir().join("test_after_stb.pdf").to_string_lossy().to_string();
    let r_after = s
        .execute(
            "plot",
            &json!({
                "layout": "Formato A4 vertical",
                "device": "Trazador PDF.pc3",
                "deviceEquivalent": "pdf",
                "compress": false,
                "path": path_after
            }),
        )
        .unwrap();
    assert!(r_after.get("warning").is_none(), "Warning must disappear once EGI.stb is loaded and resolved!");

    // 4. Inspect the uncompressed PDF:
    let bytes = std::fs::read(&path_after).unwrap();
    let text = String::from_utf8_lossy(&bytes);

    // Black stroke '0 0 0 RG' must be present for UNE styles
    assert!(text.contains("0 0 0 RG"), "PDF must contain black stroke '0 0 0 RG'");

    // Lineweights verified from numeric indices:
    // UNE 0.35 is 0.40 mm -> approx 1.134 pt
    // UNE 0.18 is 0.13 mm -> approx 0.369 pt
    assert!(text.contains("1.134 w") || text.contains("1.13"), "PDF must contain 0.40 mm lineweight (UNE 0.35)");
    assert!(text.contains("0.369 w") || text.contains("0.37"), "PDF must contain 0.13 mm lineweight (UNE 0.18)");

    // Exact A4 portrait dimensions
    assert!(text.contains("/MediaBox [0 0 595.276 841.89]"), "Must keep exact A4 portrait dimensions");
    let _ = std::fs::remove_file(&path_after);

    // 5. Test persistence: save drawing to DXF and reopen
    let temp_dxf = std::env::temp_dir().join("test_persisted_stb.dxf").to_string_lossy().to_string();
    s.execute("saveas", &json!({ "path": temp_dxf })).unwrap();

    let mut s2 = Session::new();
    s2.execute("open", &json!({ "path": temp_dxf })).unwrap();
    let doc2 = s2.doc().unwrap();

    // Table EGI.stb must still exist in drawing after reopen
    assert_eq!(doc2.plot_style_tables.len(), 1, "Plot style table must be persisted in DXF");
    assert_eq!(doc2.plot_style_tables[0].name, "EGI.stb");
    assert_eq!(doc2.plot_style_tables[0].styles.len(), 11);

    // Plotting reopened drawing must also succeed without warning!
    let path_reopened = std::env::temp_dir().join("test_reopened_stb.pdf").to_string_lossy().to_string();
    let r_reopen = s2
        .execute(
            "plot",
            &json!({
                "layout": "Formato A4 vertical",
                "device": "Trazador PDF.pc3",
                "deviceEquivalent": "pdf",
                "path": path_reopened
            }),
        )
        .unwrap();
    assert!(r_reopen.get("warning").is_none(), "Reopened drawing must retain plot style table and produce no warning");
    let _ = std::fs::remove_file(&path_reopened);
    let _ = std::fs::remove_file(&temp_dxf);
}

#[test]
fn test_all_styles_and_precedence() {
    setup_io();

    let converted_json = "Templates/EGI.converted.plotstyles.json";
    let mut s = Session::new();
    s.execute("stylesmanager", &json!({ "action": "load", "path": converted_json })).unwrap();

    // Verify all 6 individual styles:
    // 1. Normal: preserves color and lineweight (e.g. Red, 0.50 mm = 1.417 pt)
    s.execute("layer.new", &json!({ "name": "L_Normal", "color": 1, "lineweight": 0.50 })).unwrap();
    s.execute("layer.set", &json!({ "name": "L_Normal", "plotStyle": "Normal" })).unwrap();

    // 2. UNE 0.18: 0.13 mm (0.369 pt), black
    s.execute("layer.new", &json!({ "name": "L_018", "color": 2 })).unwrap();
    s.execute("layer.set", &json!({ "name": "L_018", "plotStyle": "UNE 0.18" })).unwrap();

    // 3. UNE 0.25: 0.30 mm (0.850 pt), black
    s.execute("layer.new", &json!({ "name": "L_025", "color": 3 })).unwrap();
    s.execute("layer.set", &json!({ "name": "L_025", "plotStyle": "UNE 0.25" })).unwrap();

    // 4. UNE 0.35: 0.40 mm (1.134 pt), black
    s.execute("layer.new", &json!({ "name": "L_035", "color": 4 })).unwrap();
    s.execute("layer.set", &json!({ "name": "L_035", "plotStyle": "UNE 0.35" })).unwrap();

    // 5. UNE 0.50: 0.53 mm (1.502 pt), black
    s.execute("layer.new", &json!({ "name": "L_050", "color": 5 })).unwrap();
    s.execute("layer.set", &json!({ "name": "L_050", "plotStyle": "UNE 0.50" })).unwrap();

    // 6. UNE 0.70: 0.80 mm (2.268 pt), black
    s.execute("layer.new", &json!({ "name": "L_070", "color": 6 })).unwrap();
    s.execute("layer.set", &json!({ "name": "L_070", "plotStyle": "UNE 0.70" })).unwrap();

    // 7. Internal name alias UNE_0.35
    s.execute("layer.new", &json!({ "name": "L_Alias", "color": 7 })).unwrap();
    s.execute("layer.set", &json!({ "name": "L_Alias", "plotStyle": "UNE_0.35" })).unwrap();

    // Draw one line per layer
    let layers = ["L_Normal", "L_018", "L_025", "L_035", "L_050", "L_070", "L_Alias"];
    for (i, layer) in layers.iter().enumerate() {
        s.execute("layer.current", &json!({ "name": layer })).unwrap();
        let y = i as f64 * 10.0;
        s.execute("line", &json!({ "points": [[0, y], [50, y]] })).unwrap();
    }

    // Precedence test: on layer L_070 (UNE 0.70 = black, 0.80 mm), draw a line with direct override "Normal"
    s.execute("layer.current", &json!({ "name": "L_070" })).unwrap();
    let res = s.execute("line", &json!({ "points": [[0, 80], [50, 80]] })).unwrap();
    let h_override = res["handles"][0].as_str().unwrap();
    s.execute("plotstyle", &json!({ "handles": [h_override], "name": "Normal" })).unwrap();

    let path_pdf = std::env::temp_dir().join("test_all_styles.pdf").to_string_lossy().to_string();
    s.execute("plot", &json!({
        "deviceEquivalent": "pdf",
        "plotStyleTable": "EGI.stb",
        "compress": false,
        "path": path_pdf
    })).unwrap();

    let bytes = std::fs::read(&path_pdf).unwrap();
    let text = String::from_utf8_lossy(&bytes);

    // Assert Normal keeps original red color (1 0 0 RG) and original lineweight 0.50 mm (1.417 pt)
    assert!(text.contains("1 0 0 RG"), "Normal style must preserve red object color");
    assert!(text.contains("1.417 w") || text.contains("1.42"), "Normal style must preserve 0.50 mm object lineweight (never hairline 0)");

    // Assert black output for UNE styles
    assert!(text.contains("0 0 0 RG"), "UNE styles must output black stroke '0 0 0 RG'");

    // Assert all 5 verified lineweights:
    assert!(text.contains("0.369 w") || text.contains("0.37"), "Must have 0.13 mm (UNE 0.18)");
    assert!(text.contains("0.850 w") || text.contains("0.85"), "Must have 0.30 mm (UNE 0.25)");
    assert!(text.contains("1.134 w") || text.contains("1.13"), "Must have 0.40 mm (UNE 0.35)");
    assert!(text.contains("1.502 w") || text.contains("1.50"), "Must have 0.53 mm (UNE 0.50)");
    assert!(text.contains("2.268 w") || text.contains("2.27"), "Must have 0.80 mm (UNE 0.70)");

    // Direct override test: the entity on L_070 with Normal override must preserve L_070's magenta color (color 6: 1 0 1 RG)
    assert!(text.contains("1 0 1 RG"), "Entity with direct Normal override must preserve layer color instead of black");

    let _ = std::fs::remove_file(&path_pdf);
}
