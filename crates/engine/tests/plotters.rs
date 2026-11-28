//! Plotter configurations (issue #564): PLOTTERMANAGER keeps user devices in the host's folder,
//! PAGESETUP takes a device and its papers with their printable areas, and PLOT hands the
//! plotter the device's output, paper and margins.

use cadcraft_doc::{Drawing, Space};
use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, base64_decode, set_io};
use cadcraft_engine::cmd::plotters::{PlotterStore, set_plotter_store};
use serde_json::{Value, json};

/// The plot hook echoes the options it gets.
fn echo_plot(_: &Drawing, _: &Space, opts: &Value) -> Result<Vec<u8>, String> {
    Ok(opts.to_string().into_bytes())
}

fn session() -> Session {
    set_plotter_store(PlotterStore::memory());
    set_io(IoHooks { read: |_, _| Err("no reader in this test".into()), write: |_, _| Err("no writer in this test".into()), plot: Some(echo_plot) });
    let mut s = Session::new();
    s.cmdline("line 0,0 100,0").unwrap();
    s.cmdline("").unwrap();
    s
}

fn names(s: &mut Session) -> Vec<String> {
    let r = s.execute("plottermanager", &json!({})).unwrap();
    r["devices"].as_array().unwrap().iter().map(|d| d["name"].as_str().unwrap().to_string()).collect()
}

/// The options the plot hook got.
fn plotted(r: Value) -> Value {
    let bytes = base64_decode(r["data"].as_str().unwrap()).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn manage_devices() {
    let mut s = session();
    let n = names(&mut s);
    assert_eq!(n[..3], ["CADCraft PDF", "CADCraft PNG", "CADCraft SVG"], "built-in devices first");
    let pdf = s.execute("plottermanager", &json!({"action": "get", "name": "cadcraft pdf"})).unwrap();
    assert!(pdf["builtin"].as_bool().unwrap());
    let papers = pdf["papers"].as_array().unwrap();
    assert!(papers.iter().any(|p| p["name"] == "ISO B1 (707.00 x 1000.00 MM)"));
    let a4 = papers.iter().find(|p| p["name"] == "ISO A4 (210.00 x 297.00 MM)").unwrap();
    assert_eq!(a4["margins"], json!([5.0, 5.0, 5.0, 5.0]));
    let letter = papers.iter().find(|p| p["name"] == "ANSI A (8.50 x 11.00 Inches)").unwrap();
    assert_eq!(letter["margins"], json!([6.35, 6.35, 6.35, 6.35]));

    // A user device: a PNG copy with its own borderless paper.
    s.execute("plottermanager", &json!({"action": "new", "name": "Office Raster", "output": "png", "resolution": 150})).unwrap();
    let paper = json!({"name": "A3 borderless", "width": 420, "height": 297, "margins": [0, 0, 0, 0]});
    let r = s.execute("plottermanager", &json!({"action": "set", "name": "office raster", "paper": paper, "defaultPaper": "A3 borderless"})).unwrap();
    assert_eq!(r["defaultPaper"], "A3 borderless");
    assert_eq!(r["resolution"], 150);
    assert!(names(&mut s).contains(&"Office Raster".to_string()));
    assert!(s.execute("plottermanager", &json!({"action": "new", "name": "OFFICE RASTER"})).is_err(), "duplicate");
    assert!(s.execute("plottermanager", &json!({"action": "set", "name": "CADCraft PDF", "resolution": 100})).is_err(), "built-ins are fixed");
    assert!(s.execute("plottermanager", &json!({"action": "delete", "name": "CADCraft SVG"})).is_err());
    let bad_margins = json!({"name": "x", "width": 100, "height": 100, "margins": [60, 0, 60, 0]});
    assert!(s.execute("plottermanager", &json!({"action": "set", "name": "Office Raster", "paper": bad_margins})).is_err());

    // Save as text, load under another name, delete.
    let text = s.execute("plottermanager", &json!({"action": "save", "name": "Office Raster"})).unwrap()["text"].as_str().unwrap().to_string();
    assert!(text.contains("\"cadcraftPlotter\": 1"), "{text}");
    s.execute("plottermanager", &json!({"action": "load", "text": text, "name": "Office Copy"})).unwrap();
    let copy = s.execute("plottermanager", &json!({"action": "get", "name": "Office Copy"})).unwrap();
    assert_eq!(copy["output"], "png");
    assert!(s.execute("plottermanager", &json!({"action": "load", "text": "{}"})).is_err());
    s.execute("plottermanager", &json!({"action": "delete", "name": "office copy"})).unwrap();
    assert!(!names(&mut s).contains(&"Office Copy".to_string()));
    assert!(s.execute("plottermanager", &json!({"action": "nope"})).is_err());
}

#[test]
fn page_setup_takes_a_device_and_its_papers() {
    let mut s = session();
    s.execute("plottermanager", &json!({"action": "new", "name": "Plan Printer", "output": "pdf"})).unwrap();
    let paper = json!({"name": "Strip", "width": 200, "height": 600, "margins": [10, 20, 10, 20]});
    s.execute("plottermanager", &json!({"action": "set", "name": "Plan Printer", "paper": paper})).unwrap();
    let r = s.execute("pagesetup", &json!({"layout": "Layout1", "device": "plan printer", "paper": "strip", "landscape": false})).unwrap();
    assert_eq!(r["page"]["device"], "Plan Printer");
    assert_eq!(r["page"]["paper"], "Strip");
    let page = &s.doc().unwrap().layout("Layout1").unwrap().page;
    assert_eq!((page.width_mm, page.height_mm, page.margins_mm), (200.0, 600.0, [10.0, 20.0, 10.0, 20.0]));
    // The printable area shown in the layout follows the device paper.
    let sheet = cadcraft_render::sheet(s.doc().unwrap(), "Layout1").unwrap();
    let mm = |v: f64| v * sheet.unit_mm;
    let got = [mm(sheet.printable.min.x), mm(sheet.printable.min.y), mm(sheet.printable.max.x), mm(sheet.printable.max.y)];
    assert!(got.iter().zip([10.0, 20.0, 190.0, 580.0]).all(|(a, b)| (a - b).abs() < 1e-9), "{got:?}");

    // Switching to a built-in device keeps a paper it has, else takes its default (A4).
    s.execute("pagesetup", &json!({"layout": "Layout1", "device": "CADCraft PDF"})).unwrap();
    let page = s.doc().unwrap().layout("Layout1").unwrap().page.clone();
    assert_eq!((page.paper.as_str(), page.margins_mm), ("ISO A4 (210.00 x 297.00 MM)", [5.0; 4]));
    s.execute("pagesetup", &json!({"layout": "Layout1", "paper": "A3"})).unwrap();
    let page = s.doc().unwrap().layout("Layout1").unwrap().page.clone();
    assert_eq!((page.width_mm, page.height_mm), (297.0, 420.0));

    assert!(s.execute("pagesetup", &json!({"layout": "Layout1", "device": "Nope"})).is_err());
    assert!(
        s.execute("pagesetup", &json!({"layout": "Layout1", "device": "Plan Printer", "paper": "Tabloid Extra"})).is_err(),
        "not a paper of the device"
    );
    s.execute("pagesetup", &json!({"layout": "Layout1", "device": "None", "paper": "Letter"})).unwrap();
    let page = s.doc().unwrap().layout("Layout1").unwrap().page.clone();
    assert_eq!((page.device.as_str(), page.paper.as_str()), ("None", "ANSI A (8.50 x 11.00 Inches)"));
    let report = s.execute("pagesetup", &Value::Null).unwrap();
    assert!(report["devices"].as_array().unwrap().iter().any(|d| d == "Plan Printer"));
}

#[test]
fn plot_uses_the_device() {
    let mut s = session();
    s.execute("plottermanager", &json!({"action": "new", "name": "Sketch PNG", "output": "png", "resolution": 96})).unwrap();
    let o = plotted(s.execute("plot", &json!({"layout": "Model", "device": "Sketch PNG", "paper": "A3"})).unwrap());
    assert_eq!(o["output"], "png");
    assert_eq!(o["resolution"], 96);
    assert_eq!((o["width"].as_f64(), o["height"].as_f64()), (Some(297.0), Some(420.0)));
    assert_eq!(o["margins"], json!([5.0, 5.0, 5.0, 5.0]));
    assert_eq!(o["landscape"], true, "the device's default orientation");
    assert!(o.get("paper").is_none() && o.get("device").is_none());
    // EXPORTPDF always writes PDF.
    let o = plotted(s.execute("exportpdf", &json!({"layout": "Model", "device": "Sketch PNG"})).unwrap());
    assert_eq!(o["output"], "pdf");
    // A layout plots with its page setup's device and paper.
    s.execute("pagesetup", &json!({"layout": "Layout1", "device": "Sketch PNG", "paper": "A4"})).unwrap();
    let o = plotted(s.execute("plot", &json!({"layout": "Layout1"})).unwrap());
    assert_eq!(o["output"], "png");
    assert!(o.get("width").is_none(), "the page setup already holds the paper");
    // No device: options pass through unchanged.
    let o = plotted(s.execute("plot", &json!({"layout": "Model", "paper": "A4"})).unwrap());
    assert!(o.get("output").is_none());
    assert!(s.execute("plot", &json!({"layout": "Model", "device": "Nope"})).is_err());
    assert!(s.execute("plot", &json!({"layout": "Model", "device": "Sketch PNG", "paper": "Nope"})).is_err());
}

#[test]
fn typed_plotter_manager() {
    let mut s = session();
    for line in ["-plottermanager", "New", "Typed Plotter", "Svg"] {
        s.cmdline(line).unwrap();
    }
    let d = s.execute("plottermanager", &json!({"action": "get", "name": "Typed Plotter"})).unwrap();
    assert_eq!(d["output"], "svg");
    for line in ["-plottermanager", "Paper", "Typed Plotter", "Card", "100,150"] {
        s.cmdline(line).unwrap();
    }
    let d = s.execute("plottermanager", &json!({"action": "get", "name": "Typed Plotter"})).unwrap();
    assert!(d["papers"].as_array().unwrap().iter().any(|p| p["name"] == "Card" && p["width"] == 100.0));
    for line in ["-plottermanager", "Delete", "Typed Plotter"] {
        s.cmdline(line).unwrap();
    }
    assert!(!names(&mut s).contains(&"Typed Plotter".to_string()));
}

#[test]
fn hostile_names_and_values_are_refused() {
    let mut s = session();
    let r = s.execute("plottermanager", &json!({"action": "new", "name": "../../etc/passwd"})).unwrap();
    assert_eq!(r["name"], "../../etc/passwd", "the name is kept; only the file name is cleaned");
    for p in [
        json!({"action": "new", "name": ""}),
        json!({"action": "new", "name": "x", "output": "dwf"}),
        json!({"action": "new", "name": "y", "resolution": 1e12}),
        json!({"action": "set", "name": "../../etc/passwd", "paper": {"name": "p", "width": f64::MAX, "height": 1}}),
        json!({"action": "set", "name": "../../etc/passwd", "paper": {"name": "p", "width": -1, "height": 1}}),
        json!({"action": "set", "name": "../../etc/passwd", "removePaper": "nope"}),
        json!({"action": "load", "text": "x".repeat(2 << 20)}),
        json!({"action": "get"}),
    ] {
        assert!(s.execute("plottermanager", &p).is_err(), "{p}");
    }
}

#[test]
fn plotwindow_keeps_the_picked_window_for_the_plot_dialog() {
    let mut s = Session::new();
    // Typed: two corners, with the window's rectangle following the cursor after the first.
    s.cmdline("plotwindow").unwrap();
    assert_eq!(s.current_prompt().unwrap().message, "Specify first corner");
    s.cmdline("10,20").unwrap();
    assert_eq!(s.preview(cadcraft_engine::geom::Vec2::new(40.0, 5.0)).len(), 4, "the rubber-band rectangle");
    s.cmdline("40,5").unwrap();
    assert!(s.running.is_none());
    let w = s.plot_window.expect("the picked window");
    assert_eq!((w.min.x, w.min.y, w.max.x, w.max.y), (10.0, 5.0, 40.0, 20.0));
    // JSON, and a window without area is refused.
    let r = s.execute("plotwindow", &json!({"p1": [0, 0], "p2": [3, 4]})).unwrap();
    assert_eq!(r["window"], json!([[0.0, 0.0], [3.0, 4.0]]));
    assert!(s.execute("plotwindow", &json!({"p1": [0, 0], "p2": [3, 0]})).is_err());
}

#[test]
fn explicit_device_equivalence_and_property_preservation() {
    let mut s = session();
    s.execute("layout.new", &json!({"name": "LayoutA4"})).unwrap();
    // Custom page setup: A4 portrait, 0 mm margins
    s.execute(
        "pagesetup",
        &json!({
            "layout": "LayoutA4",
            "device": "Trazador PDF.pc3",
            "deviceEquivalent": "pdf",
            "paper": "ISO A4 (210.00 x 297.00 MM)",
            "width": 210.0,
            "height": 297.0,
            "landscape": false,
            "margins": [0.0, 0.0, 0.0, 0.0],
            "plotArea": "layout",
            "scale": 1.0,
            "plotStyleTable": "custom_styles.stb",
        }),
    )
    .unwrap();

    let page = s.doc().unwrap().layout("LayoutA4").unwrap().page.clone();
    assert_eq!(page.device, "Trazador PDF.pc3");
    assert_eq!(page.landscape, false);
    assert_eq!(page.margins_mm, [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(page.width_mm, 210.0);
    assert_eq!(page.height_mm, 297.0);
    assert_eq!(page.plot_style_table, "custom_styles.stb");

    // Without explicit equivalence, an unknown device is rejected
    assert!(
        s.execute("plot", &json!({"layout": "LayoutA4", "device": "Desconocido.pc3"}))
            .is_err(),
        "unknown device without equivalence must fail"
    );

    // With explicit equivalence, plot succeeds and warns about unavailable custom_styles.stb
    let r = s
        .execute(
            "plot",
            &json!({
                "layout": "LayoutA4",
                "device": "Trazador PDF.pc3",
                "deviceEquivalent": "pdf"
            }),
        )
        .unwrap();
    assert!(r["data"].is_string());
    assert!(r["warning"].as_str().unwrap().contains("custom_styles.stb"));

    // Changing device preserves existing orientation and custom 0 mm margins
    s.execute("pagesetup", &json!({"layout": "LayoutA4", "device": "CADCraft PDF"})).unwrap();
    let updated = s.doc().unwrap().layout("LayoutA4").unwrap().page.clone();
    assert_eq!(updated.device, "CADCraft PDF");
    assert_eq!(updated.landscape, false, "portrait must not be flipped to landscape");
    assert_eq!(updated.margins_mm, [0.0, 0.0, 0.0, 0.0], "custom 0 margins must be preserved");
}

#[test]
fn axialcad_devices_and_stroke_font_aliases() {
    use cadcraft_engine::cmd::plotters::{canonical_device_name, display_device_name, find_device};

    // 1. Devices: Display name vs canonical identifier
    assert_eq!(display_device_name("CADCraft PDF"), "axialCAD PDF");
    assert_eq!(display_device_name("CADCraft PNG"), "axialCAD PNG");
    assert_eq!(display_device_name("CADCraft SVG"), "axialCAD SVG");
    assert_eq!(display_device_name("Custom Plotter"), "Custom Plotter");

    assert_eq!(canonical_device_name("axialCAD PDF"), "CADCraft PDF");
    assert_eq!(canonical_device_name("axialCAD PNG"), "CADCraft PNG");
    assert_eq!(canonical_device_name("axialCAD SVG"), "CADCraft SVG");
    assert_eq!(canonical_device_name("CADCraft PDF"), "CADCraft PDF");
    assert_eq!(canonical_device_name("Custom Plotter"), "Custom Plotter");

    // 2. Both new aliases and historical names resolve to the canonical built-in device
    let dev_alias = find_device("axialCAD PDF").expect("alias resolves");
    let dev_canon = find_device("CADCraft PDF").expect("canonical resolves");
    assert_eq!(dev_alias.name, "CADCraft PDF");
    assert_eq!(dev_canon.name, "CADCraft PDF");
    assert_eq!(dev_alias.output, dev_canon.output);

    // 3. Selection of axialCAD label normalizes to historical identifier in page setup
    let mut s = session();
    s.execute("pagesetup", &json!({"layout": "Layout1", "device": "axialCAD PDF"})).unwrap();
    let p = s.doc().unwrap().layout("Layout1").unwrap().page.clone();
    assert_eq!(p.device, "CADCraft PDF", "historical identifier preserved in drawing model");

    // 4. Font: Display name vs canonical identifier
    assert_eq!(cadcraft_fonts::display_font_name("CADCraft Stroke"), "axialCAD Stroke");
    assert_eq!(cadcraft_fonts::canonical_font_name("axialCAD Stroke"), "CADCraft Stroke");
    assert_eq!(cadcraft_fonts::canonical_font_name("CADCraft Stroke"), "CADCraft Stroke");
    assert_eq!(cadcraft_fonts::display_font_name("Arial"), "Arial");

    // 5. TextFont::resolve resolves both to stroke font
    assert!(!cadcraft_fonts::TextFont::resolve("axialCAD Stroke").is_outline());
    assert!(!cadcraft_fonts::TextFont::resolve("CADCraft Stroke").is_outline());

    // 6. Style command with axialCAD Stroke alias sets canonical font name
    s.execute("style", &json!({"name": "BrandStyle", "font": "axialCAD Stroke"})).unwrap();
    let d = s.doc().unwrap();
    let st = d.text_styles.iter().find(|t| t.name == "BrandStyle").expect("style created");
    assert_eq!(st.font, "CADCraft Stroke", "canonical font name stored in TextStyle");
}

#[test]
fn settings_root_precedence_and_platform_defaults() {
    use std::ffi::OsString;
    use std::path::PathBuf;

    // 1. AXIALCAD_CONFIG_DIR has precedence over CADCRAFT_CONFIG_DIR
    let env_both = |k: &str| match k {
        "AXIALCAD_CONFIG_DIR" => Some(OsString::from("/custom/axial")),
        "CADCRAFT_CONFIG_DIR" => Some(OsString::from("/custom/cadcraft")),
        _ => None,
    };
    assert_eq!(
        PlotterStore::settings_root_from("linux", env_both),
        Some(PathBuf::from("/custom/axial"))
    );

    // 2. Empty AXIALCAD_CONFIG_DIR falls back to CADCRAFT_CONFIG_DIR
    let env_empty_axial = |k: &str| match k {
        "AXIALCAD_CONFIG_DIR" => Some(OsString::from("")),
        "CADCRAFT_CONFIG_DIR" => Some(OsString::from("/custom/cadcraft")),
        _ => None,
    };
    assert_eq!(
        PlotterStore::settings_root_from("linux", env_empty_axial),
        Some(PathBuf::from("/custom/cadcraft"))
    );

    // 3. Clean platform defaults resolve to axialCAD paths
    let env_linux = |k: &str| match k {
        "HOME" => Some(OsString::from("/home/user")),
        _ => None,
    };
    assert_eq!(
        PlotterStore::settings_root_from("linux", env_linux),
        Some(PathBuf::from("/home/user/.config/axialcad"))
    );

    let env_macos = |k: &str| match k {
        "HOME" => Some(OsString::from("/Users/user")),
        _ => None,
    };
    assert_eq!(
        PlotterStore::settings_root_from("macos", env_macos),
        Some(PathBuf::from("/Users/user/Library/Application Support/axialCAD"))
    );

    let env_windows = |k: &str| match k {
        "APPDATA" => Some(OsString::from(r"C:\Users\user\AppData\Roaming")),
        _ => None,
    };
    assert_eq!(
        PlotterStore::settings_root_from("windows", env_windows),
        Some(PathBuf::from(r"C:\Users\user\AppData\Roaming").join("axialCAD"))
    );
}

static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct ScopedEnv {
    map: std::collections::HashMap<String, std::ffi::OsString>,
}

impl ScopedEnv {
    fn new() -> Self {
        Self { map: std::collections::HashMap::new() }
    }

    fn set(&mut self, key: &str, val: &std::path::Path) {
        self.map.insert(key.to_string(), val.as_os_str().to_os_string());
        cadcraft_engine::cmd::plotters::set_test_env(Some(self.map.clone()));
    }

    fn remove(&mut self, key: &str) {
        self.map.remove(key);
        cadcraft_engine::cmd::plotters::set_test_env(Some(self.map.clone()));
    }
}

impl Drop for ScopedEnv {
    fn drop(&mut self) {
        cadcraft_engine::cmd::plotters::set_test_env(None);
    }
}

#[test]
fn plotter_store_real_filesystem_save_read_list_delete_in_temp_dir() {
    let _lock = TEST_ENV_LOCK.lock().unwrap();
    let mut env_guard = ScopedEnv::new();

    let tmp = std::env::temp_dir().join(format!("axialcad-plotter-fs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    // 1. AXIALCAD_CONFIG_DIR directs real PlotterStore I/O to tmp/plotters
    env_guard.set("AXIALCAD_CONFIG_DIR", &tmp);
    env_guard.remove("CADCRAFT_CONFIG_DIR");

    let store = PlotterStore::settings_folder();
    let filename = "real_device.plotter.json";
    let content = r#"{"name":"Real Device","output":"pdf","papers":[]}"#;

    // Write: must create tmp/plotters and real file
    assert!((store.write)(filename, content).is_ok());
    let real_file = tmp.join("plotters").join(filename);
    assert!(real_file.is_file(), "plotter file must exist on disk");

    // List: must list the real file from disk
    let files = (store.list)().expect("list succeeds");
    assert!(files.contains(&filename.to_string()), "file must be listed");

    // Read: must read exact content from disk
    let read_back = (store.read)(filename).expect("read succeeds");
    assert_eq!(read_back, content);

    // Delete: must remove real file from disk
    assert!((store.delete)(filename).is_ok());
    assert!(!real_file.exists(), "file must be removed from disk");

    // 2. CADCRAFT_CONFIG_DIR fallback directs real I/O when AXIALCAD_CONFIG_DIR is absent
    env_guard.remove("AXIALCAD_CONFIG_DIR");
    let legacy_tmp = std::env::temp_dir().join(format!("cadcraft-plotter-fs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&legacy_tmp);
    std::fs::create_dir_all(&legacy_tmp).unwrap();
    env_guard.set("CADCRAFT_CONFIG_DIR", &legacy_tmp);

    let legacy_store = PlotterStore::settings_folder();
    assert!((legacy_store.write)(filename, content).is_ok());
    assert!(legacy_tmp.join("plotters").join(filename).is_file());
    assert!((legacy_store.delete)(filename).is_ok());

    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_dir_all(&legacy_tmp);
}

#[test]
fn plotter_store_has_existing_config_distinguishes_startup_artifacts() {
    let tmp = std::env::temp_dir().join(format!("axialcad-plotter-artifacts-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();

    let legacy = tmp.join("cadcraft");
    let primary = tmp.join("axialcad");
    std::fs::create_dir_all(&legacy).unwrap();
    std::fs::create_dir_all(&primary).unwrap();

    // 1. Empty folders in primary do not count as user configuration
    std::fs::create_dir_all(primary.join("support")).unwrap();
    std::fs::create_dir_all(primary.join("plotters")).unwrap();
    std::fs::create_dir_all(primary.join("autosave")).unwrap();
    std::fs::create_dir_all(primary.join("logs")).unwrap();
    std::fs::write(primary.join("logs").join("cadcraft.log"), b"startup log\n").unwrap();
    assert!(!PlotterStore::has_existing_config(&primary, false), "startup artifacts in primary must not count as config");

    // 2. Legacy with workspace.json is recognized
    std::fs::write(legacy.join("workspace.json"), b"{}").unwrap();
    assert!(PlotterStore::has_existing_config(&legacy, true), "legacy with workspace.json is recognized");

    // 3. Primary with real user plotter file counts as existing config
    std::fs::write(primary.join("plotters").join("custom.plotter.json"), b"{}").unwrap();
    assert!(PlotterStore::has_existing_config(&primary, false), "primary with user plotter is recognized");

    let _ = std::fs::remove_dir_all(&tmp);
}
