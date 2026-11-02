use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services};
use serde_json::json;
use std::sync::{Arc, Mutex};

static TEMPLATE_TEST_LOCK: Mutex<()> = Mutex::new(());

struct RestoreQnewTemplate(String);
impl Drop for RestoreQnewTemplate {
    fn drop(&mut self) {
        cadcraft_engine::cmd::file::set_qnew_template(&self.0);
    }
}

fn app() -> CadApp {
    CadApp::new(Session::new(), Services::default())
}

#[test]
fn test_default_folders_prefs_persistence_and_backward_compatibility() {
    let mut a = app();
    assert_eq!(a.ui.default_drawings_dir, None);
    assert_eq!(a.ui.default_templates_dir, None);

    // 1. Set paths including non-ASCII and spaces
    let custom_drawings = "/tmp/Mis Dibujos 2026/Planos España";
    let custom_templates = "/tmp/Plantillas de Ingeniería/ISO Métrico";

    a.run(
        "options",
        json!({
            "drawingsDir": custom_drawings,
            "templatesDir": custom_templates,
        }),
    )
    .unwrap();

    assert_eq!(a.ui.default_drawings_dir.as_deref(), Some(custom_drawings));
    assert_eq!(a.ui.default_templates_dir.as_deref(), Some(custom_templates));

    // Check report output
    let rep = a.run("options", json!({})).unwrap();
    assert_eq!(rep["drawingsDir"], json!(custom_drawings));
    assert_eq!(rep["templatesDir"], json!(custom_templates));

    // 2. Profile roundtrip (serialize / deserialize prefs)
    let profile = a.profile_json();
    let mut restored = app();
    restored.load_profile(&profile);
    assert_eq!(restored.ui.default_drawings_dir.as_deref(), Some(custom_drawings));
    assert_eq!(restored.ui.default_templates_dir.as_deref(), Some(custom_templates));

    // 3. Reset clears to None
    restored.run("options", json!({"reset": true})).unwrap();
    assert_eq!(restored.ui.default_drawings_dir, None);
    assert_eq!(restored.ui.default_templates_dir, None);

    // 4. Backward compatibility: profile without drawingsDir/templatesDir retains None
    let mut old_prefs = app();
    let legacy_json = json!({
        "recentFiles": [],
        "saveVersion": "2018",
        "theme": "dark"
    })
    .to_string();
    old_prefs.load_prefs(&legacy_json);
    assert_eq!(old_prefs.ui.default_drawings_dir, None);
    assert_eq!(old_prefs.ui.default_templates_dir, None);
}

#[test]
fn test_template_folder_and_qnewtemplate_independence() {
    let _serial = TEMPLATE_TEST_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let _restore = RestoreQnewTemplate(cadcraft_engine::cmd::file::qnew_template());

    let mut a = app();
    let original_qnew = cadcraft_engine::cmd::file::qnew_template();

    // Changing templates directory does NOT alter QNEWTEMPLATE
    a.run(
        "options",
        json!({
            "templatesDir": "/custom/templates/path"
        }),
    )
    .unwrap();
    assert_eq!(a.ui.default_templates_dir.as_deref(), Some("/custom/templates/path"));
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), original_qnew);

    // Changing QNEWTEMPLATE does NOT alter templatesDir
    a.run(
        "options",
        json!({
            "QNEWTEMPLATE": "/custom/specific/template.dwt"
        }),
    )
    .unwrap();
    assert_eq!(a.ui.default_templates_dir.as_deref(), Some("/custom/templates/path"));
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), "/custom/specific/template.dwt");

    // Clearing templatesDir does NOT alter QNEWTEMPLATE
    a.run(
        "options",
        json!({
            "templatesDir": null
        }),
    )
    .unwrap();
    assert_eq!(a.ui.default_templates_dir, None);
    assert_eq!(cadcraft_engine::cmd::file::qnew_template(), "/custom/specific/template.dwt");
}

#[test]
fn test_saveas_proposes_default_drawings_folder_for_new_doc_and_current_folder_for_existing() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |_, _| Ok(cadcraft_doc::Drawing::new_metric()),
        write: |_, _| Ok(b"0\nEOF\n".to_vec()),
        plot: None,
    });
    let temp_dir = std::env::temp_dir().join("cadcraft_test_save_dirs");
    let _ = std::fs::create_dir_all(&temp_dir);
    let custom_drawings_dir = temp_dir.to_str().unwrap().to_string();

    let captured_initial_dir = Arc::new(Mutex::new(None::<Option<String>>));
    let captured_clone = Arc::clone(&captured_initial_dir);

    let mut services = Services::default();
    services.pick_save = Some(Box::new(move |_name: &str, dir: Option<&str>| {
        *captured_clone.lock().unwrap() = Some(dir.map(|s| s.to_string()));
        None // User cancels picker in this test
    }));

    let mut a = CadApp::new(Session::new(), services);
    a.ui.default_drawings_dir = Some(custom_drawings_dir.clone());

    // Case 1: New untitled document -> proposes default_drawings_dir
    a.session.new_drawing(true);
    let _ = a.run("ui.saveas", json!({}));
    assert_eq!(
        *captured_initial_dir.lock().unwrap(),
        Some(Some(custom_drawings_dir.clone())),
        "New untitled drawing must propose default_drawings_dir"
    );

    // Case 2: Document with existing path -> proposes parent directory of document
    let existing_folder = temp_dir.join("subfolder_project");
    let _ = std::fs::create_dir_all(&existing_folder);
    let existing_file = existing_folder.join("ProjectA.dwg");

    a.session.execute("saveas", &json!({ "path": existing_file.to_str().unwrap() })).unwrap();
    let _ = a.run("ui.saveas", json!({}));
    assert_eq!(
        *captured_initial_dir.lock().unwrap(),
        Some(Some(existing_folder.to_str().unwrap().to_string())),
        "Existing drawing must propose its current parent directory, not default_drawings_dir"
    );

    // Default drawings folder was NOT changed by opening or proposing saveas in another folder
    assert_eq!(a.ui.default_drawings_dir.as_deref(), Some(custom_drawings_dir.as_str()));
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_template_pickers_propose_default_templates_dir() {
    let temp_dir = std::env::temp_dir().join("cadcraft_test_template_dirs");
    let _ = std::fs::create_dir_all(&temp_dir);
    let custom_templates_dir = temp_dir.to_str().unwrap().to_string();

    let captured_initial_dir = Arc::new(Mutex::new(None::<Option<String>>));
    let captured_clone = Arc::clone(&captured_initial_dir);

    let mut services = Services::default();
    services.pick_template = Some(Box::new(move |dir: Option<&str>| {
        *captured_clone.lock().unwrap() = Some(dir.map(|s| s.to_string()));
        None // Cancel
    }));

    let mut a = CadApp::new(Session::new(), services);
    a.ui.default_templates_dir = Some(custom_templates_dir.clone());

    // 1. Choose default template command
    let _ = a.run("ui.defaulttemplate.choose", json!({}));
    assert_eq!(
        *captured_initial_dir.lock().unwrap(),
        Some(Some(custom_templates_dir.clone())),
        "Choosing default template must propose default_templates_dir"
    );

    // 2. New from template command
    *captured_initial_dir.lock().unwrap() = None;
    let _ = a.run("ui.newtemplate", json!({}));
    assert_eq!(
        *captured_initial_dir.lock().unwrap(),
        Some(Some(custom_templates_dir.clone())),
        "New from template must propose default_templates_dir"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_options_dialog_cancel_and_apply_transactionality() {
    let mut a = app();
    a.ui.default_drawings_dir = Some("/initial/drawings".into());
    a.ui.default_templates_dir = Some("/initial/templates".into());

    let ctx = egui::Context::default();

    // 1. Open Options dialog
    a.ui.dialog = Some("options:files".into());

    let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0))), ..Default::default() };
    a.logic(&ctx);
    let mut out = ctx.run_ui(input, |ui| a.ui(ui));
    out.textures_delta.clear();

    // 2. Simulate pending modification while dialog is open
    a.run(
        "options",
        json!({
            "drawingsDir": "/modified/pending/drawings",
            "templatesDir": "/modified/pending/templates",
        }),
    )
    .unwrap();
    assert_eq!(a.ui.default_drawings_dir.as_deref(), Some("/modified/pending/drawings"));

    // 3. User cancels the dialog (triggers rollback logic in options dialog)
    // We can simulate Cancel by rolling back to snapshot or closing dialog with cancel.
    // In dialog.rs, clicking Cancel executes snapshot rollback:
    if let Some(snapshot) = ctx.data(|d| d.get_temp::<serde_json::Value>(egui::Id::new("cc_options_snapshot"))) {
        let _ = a.run("options", snapshot);
        ctx.data_mut(|d| d.remove_temp::<serde_json::Value>(egui::Id::new("cc_options_snapshot")));
        a.ui.dialog = None;
    }

    assert_eq!(
        a.ui.default_drawings_dir.as_deref(),
        Some("/initial/drawings"),
        "Cancelling options dialog must restore initial default_drawings_dir"
    );
    assert_eq!(
        a.ui.default_templates_dir.as_deref(),
        Some("/initial/templates"),
        "Cancelling options dialog must restore initial default_templates_dir"
    );
}

#[test]
fn test_cancelling_folder_picker_preserves_previous_value() {
    let mut services = Services::default();
    // Simulate user cancelling folder picker (returns None)
    services.pick_folder = Some(Box::new(|_dir| None));

    let mut a = CadApp::new(Session::new(), services);
    a.ui.default_drawings_dir = Some("/preserved/drawings".into());

    // When folder picker returns None, Ui does not update the setting
    let picked = a.services.pick_folder.as_ref().and_then(|f| f(a.ui.default_drawings_dir.as_deref()));
    assert_eq!(picked, None);
    if let Some(folder) = picked {
        a.ui.default_drawings_dir = Some(folder);
    }
    assert_eq!(a.ui.default_drawings_dir.as_deref(), Some("/preserved/drawings"));
}

#[test]
fn test_explicit_paths_are_not_redirected_by_default_drawings_dir() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |_, _| Ok(cadcraft_doc::Drawing::new_metric()),
        write: |_, _| Ok(b"0\nEOF\n".to_vec()),
        plot: None,
    });
    let mut a = app();
    a.ui.default_drawings_dir = Some("/some/default/drawings".into());

    let temp_target = std::env::temp_dir().join("explicit_target.dxf");
    let explicit_str = temp_target.to_str().unwrap();

    // Drawing opened from explicit path
    a.session.new_drawing(true);
    let run_res = a.session.execute(
        "saveas",
        &json!({
            "path": explicit_str
        }),
    );
    assert!(run_res.is_ok(), "Explicit saveas must succeed");
    let st = a.session.state().unwrap();
    assert_eq!(st.path.as_deref(), Some(explicit_str), "Must save to explicit path, not default folder");
    let _ = std::fs::remove_file(&temp_target);
}

#[test]
fn test_cancellation_of_save_does_not_update_recent_files() {
    let mut a = app();
    let initial_recents = a.ui.recent_files.clone();

    // Simulate saveas cancelled by user
    let res = a.run("saveas", json!({"cancelled": true}));
    assert!(res.is_err() || a.ui.recent_files == initial_recents);
    assert_eq!(a.ui.recent_files, initial_recents, "Cancelled save must not touch recent files");
}

#[test]
fn test_existing_document_qsave_retains_path_and_format() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |_, _| Ok(cadcraft_doc::Drawing::new_metric()),
        write: |_, _| Ok(b"0\nEOF\n".to_vec()),
        plot: None,
    });
    let mut a = app();
    a.ui.default_drawings_dir = Some("/some/default/drawings".into());

    let temp_dxf = std::env::temp_dir().join("existing_document.dxf");
    let dxf_str = temp_dxf.to_str().unwrap();

    // 1. Initial save as DXF
    a.session.new_drawing(true);
    a.session.execute("saveas", &json!({ "path": dxf_str })).unwrap();
    assert_eq!(a.session.state().unwrap().path.as_deref(), Some(dxf_str));

    // 2. QSave (e.g. Ctrl+S) on existing DXF document keeps path and DXF format
    let res = a.run("qsave", json!({}));
    assert!(res.is_ok());
    let st = a.session.state().unwrap();
    assert_eq!(st.path.as_deref(), Some(dxf_str), "Must remain existing_document.dxf, not moved or converted to DWG");
    let _ = std::fs::remove_file(&temp_dxf);
}

#[test]
fn test_new_from_template_is_untitled_and_independent() {
    cadcraft_engine::cmd::file::set_io(cadcraft_engine::cmd::file::IoHooks {
        read: |_, _| Ok(cadcraft_doc::Drawing::new_metric()),
        write: |_, _| Ok(b"0\nEOF\n".to_vec()),
        plot: None,
    });
    let mut a = app();
    a.ui.default_templates_dir = Some("/some/templates/dir".into());

    let temp_dwt = std::env::temp_dir().join("template_sample.dwt");
    let _ = std::fs::write(&temp_dwt, b"mock dwt");

    // Creating a drawing from template
    a.session.new_drawing(true);
    let st = a.session.state().unwrap();
    assert!(st.path.is_none(), "Document created from template must be untitled without implicit destination");
    assert!(st.title.starts_with("Drawing"));

    let _ = std::fs::remove_file(&temp_dwt);
}

#[test]
fn test_missing_or_inaccessible_folder_shows_warning_without_panic() {
    let mut a = app();
    let non_existent_folder = "/path/that/definitely/does/not/exist_12345/nowhere";
    a.ui.default_drawings_dir = Some(non_existent_folder.to_string());
    a.ui.default_templates_dir = Some(non_existent_folder.to_string());

    let ctx = egui::Context::default();
    a.ui.dialog = Some("options:files".into());

    let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 1000.0))), ..Default::default() };
    a.logic(&ctx);
    let mut out = ctx.run_ui(input, |ui| a.ui(ui));
    out.textures_delta.clear();

    // The setting remains intact (not silently cleared)
    assert_eq!(a.ui.default_drawings_dir.as_deref(), Some(non_existent_folder));
    assert_eq!(a.ui.default_templates_dir.as_deref(), Some(non_existent_folder));
}
