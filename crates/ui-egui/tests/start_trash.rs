//! Start page context menu and system trash safety tests.

use std::fs::File;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, PendingTrash, Services, chrome};
use egui::{PointerButton, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

static COUNTER: AtomicUsize = AtomicUsize::new(1);

fn make_temp_file(prefix: &str, ext: &str) -> String {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("cadcraft-trash-test-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file_path = dir.join(format!("{prefix}.{ext}"));
    let mut f = File::create(&file_path).unwrap();
    writeln!(f, "test content").unwrap();
    file_path.to_str().unwrap().to_string()
}

#[test]
fn test_remove_from_recents_keeps_file_on_disk() {
    let path = make_temp_file("test_remove_keep", "dwg");
    assert!(std::path::Path::new(&path).exists());

    let mut app = CadApp::new(Session::empty(), Services::default());
    app.remember_recent(path.clone());
    app.toggle_pinned(&path);
    assert!(app.ui.recent_files.contains(&path));
    assert!(app.ui.pinned_files.contains(&path));

    // Forget recent
    app.forget_recent(&path);

    // Verify removed from recents and pinned
    assert!(!app.ui.recent_files.contains(&path));
    assert!(!app.ui.pinned_files.contains(&path));

    // CRITICAL: The file itself MUST remain intact on disk
    assert!(std::path::Path::new(&path).exists(), "Remove from recents must never delete the file");
}

#[test]
fn test_trash_confirmation_cancel_leaves_everything_intact() {
    let path = make_temp_file("test_cancel_trash", "dwg");
    let called = Arc::new(Mutex::new(Vec::<String>::new()));
    let called_clone = Arc::clone(&called);

    let services = Services {
        trash_file: Some(Box::new(move |p| {
            called_clone.lock().unwrap().push(p.to_string());
            Ok(())
        })),
        ..Services::default()
    };

    let mut app = CadApp::new(Session::empty(), services);
    app.remember_recent(path.clone());
    app.ui.pending_trash =
        Some(PendingTrash { path: path.clone(), filename: "test_cancel_trash.dwg".to_string(), is_open_in_cadcraft: false, error: None });

    let mut h = Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            chrome::trash_confirmation_dialog(app, ui.ctx());
        },
        app,
    );
    h.run_steps(2);

    // Click Cancel
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Cancel").click();
    h.run_steps(2);

    let app = h.state();
    assert!(app.ui.pending_trash.is_none(), "pending_trash cleared after cancel");
    assert!(app.ui.recent_files.contains(&path), "recents preserved after cancel");
    assert!(called.lock().unwrap().is_empty(), "trash service was not called");
    assert!(std::path::Path::new(&path).exists(), "file remains intact on disk");
}

#[test]
fn test_trash_confirmation_success_invokes_trash_and_removes_from_recents() {
    let path = make_temp_file("test_confirm_trash", "dwg");
    let called = Arc::new(Mutex::new(Vec::<String>::new()));
    let called_clone = Arc::clone(&called);

    let services = Services {
        trash_file: Some(Box::new(move |p| {
            called_clone.lock().unwrap().push(p.to_string());
            Ok(())
        })),
        ..Services::default()
    };

    let mut app = CadApp::new(Session::empty(), services);
    app.remember_recent(path.clone());
    app.toggle_pinned(&path);
    app.ui.pending_trash =
        Some(PendingTrash { path: path.clone(), filename: "test_confirm_trash.dwg".to_string(), is_open_in_cadcraft: false, error: None });

    let mut h = Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            chrome::trash_confirmation_dialog(app, ui.ctx());
        },
        app,
    );
    h.run_steps(2);

    // Click "Move to Trash"
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Move to Trash").click();
    h.run_steps(2);

    let app = h.state();
    assert!(app.ui.pending_trash.is_none(), "pending_trash cleared after successful move");
    assert!(!app.ui.recent_files.contains(&path), "removed from recents after trash");
    assert!(!app.ui.pinned_files.contains(&path), "removed from pinned after trash");
    assert_eq!(*called.lock().unwrap(), vec![path]);
}

#[test]
fn test_trash_open_file_blocked_with_notice() {
    let path = make_temp_file("test_open_file", "dwg");
    let called = Arc::new(Mutex::new(Vec::<String>::new()));
    let called_clone = Arc::clone(&called);

    let services = Services {
        trash_file: Some(Box::new(move |p| {
            called_clone.lock().unwrap().push(p.to_string());
            Ok(())
        })),
        ..Services::default()
    };

    let mut app = CadApp::new(Session::empty(), services);
    app.remember_recent(path.clone());

    // Open drawing in session
    let d = cadcraft_engine::doc::Drawing::new_metric();
    app.session.open_drawing(d, "test_open_file.dwg", Some(path.clone()));

    // Verify is_file_open_in_cadcraft recognizes it
    assert!(chrome::is_file_open_in_cadcraft(&app, &path));

    // Also test with canonicalized / non-canonicalized variations
    let canon_path = std::fs::canonicalize(&path).unwrap().to_str().unwrap().to_string();
    assert!(chrome::is_file_open_in_cadcraft(&app, &canon_path));

    // Open pending trash
    app.ui.pending_trash =
        Some(PendingTrash { path: path.clone(), filename: "test_open_file.dwg".to_string(), is_open_in_cadcraft: true, error: None });

    let mut h = Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            chrome::trash_confirmation_dialog(app, ui.ctx());
        },
        app,
    );
    h.run_steps(2);

    // Verify warning is displayed and "Move to Trash" button is NOT available
    assert!(
        h.query_by_role_and_label(egui::accesskit::Role::Button, "Move to Trash").is_none(),
        "Move to Trash must be blocked when file is open in CADcraft"
    );

    // Click Close
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Close").click();
    h.run_steps(2);

    assert!(h.state().ui.pending_trash.is_none());
    assert!(called.lock().unwrap().is_empty(), "trash service was NEVER called on open file");
    assert!(std::path::Path::new(&path).exists());
}

#[test]
fn test_trash_failure_keeps_entry_and_displays_error() {
    let path = make_temp_file("test_fail_trash", "dwg");

    let services = Services { trash_file: Some(Box::new(|_| Err("Access denied / permission error".to_string()))), ..Services::default() };

    let mut app = CadApp::new(Session::empty(), services);
    app.remember_recent(path.clone());
    app.ui.pending_trash =
        Some(PendingTrash { path: path.clone(), filename: "test_fail_trash.dwg".to_string(), is_open_in_cadcraft: false, error: None });

    let mut h = Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            chrome::trash_confirmation_dialog(app, ui.ctx());
        },
        app,
    );
    h.run_steps(2);

    // Click Move to Trash
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Move to Trash").click();
    h.run_steps(2);

    let app = h.state();
    // Verify file is still in recents and error is shown
    assert!(app.ui.recent_files.contains(&path));
    assert!(app.ui.pending_trash.as_ref().and_then(|p| p.error.as_ref()).is_some_and(|e| e.contains("Access denied")));
}

#[test]
fn test_trash_unavailable_reported() {
    let path = make_temp_file("test_no_trash", "dwg");

    // services without trash_file (e.g. web)
    let services = Services { trash_file: None, ..Services::default() };

    let mut app = CadApp::new(Session::empty(), services);
    app.remember_recent(path.clone());
    app.ui.pending_trash =
        Some(PendingTrash { path: path.clone(), filename: "test_no_trash.dwg".to_string(), is_open_in_cadcraft: false, error: None });

    let mut h = Harness::builder().with_size(vec2(1000.0, 700.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            chrome::trash_confirmation_dialog(app, ui.ctx());
        },
        app,
    );
    h.run_steps(2);

    // Click Move to Trash
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Move to Trash").click();
    h.run_steps(2);

    let app = h.state();
    assert!(app.ui.recent_files.contains(&path));
    assert!(app.ui.pending_trash.as_ref().and_then(|p| p.error.as_ref()).is_some());
}

#[test]
fn test_same_filename_in_different_directories_isolated() {
    let path1 = make_temp_file("drawing", "dwg");
    let path2 = make_temp_file("drawing", "dwg");
    assert_ne!(path1, path2);

    let called = Arc::new(Mutex::new(Vec::<String>::new()));
    let called_clone = Arc::clone(&called);

    let services = Services {
        trash_file: Some(Box::new(move |p| {
            called_clone.lock().unwrap().push(p.to_string());
            Ok(())
        })),
        ..Services::default()
    };

    let mut app = CadApp::new(Session::empty(), services);
    app.remember_recent(path1.clone());
    app.remember_recent(path2.clone());

    app.forget_recent(&path1);

    assert!(!app.ui.recent_files.contains(&path1));
    assert!(app.ui.recent_files.contains(&path2), "other file with same filename must remain");
    assert!(std::path::Path::new(&path2).exists());
}

fn texts(shape: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
    match shape {
        egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.visual_bounding_rect())),
        egui::Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
        _ => {}
    }
}

#[test]
fn test_start_page_context_menu_does_not_open_drawing() {
    let path = make_temp_file("test_ctx_menu", "dwg");
    let mut app = CadApp::new(Session::empty(), Services::default());
    app.remember_recent(path.clone());
    app.ui.start_grid_view = true;

    let mut h = Harness::builder().with_size(vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);

    // Initial state: Start tab is visible, session docs are empty
    assert!(h.state().session.docs.is_empty());

    // Find card location on screen
    let mut found = Vec::new();
    for s in &h.output().shapes {
        texts(&s.shape, &mut found);
    }
    let pos = found.iter().find(|(t, _)| t == "test_ctx_menu.dwg").expect("card filename text").1.center();

    // Right-click on the card
    h.hover_at(pos);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos, button: PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos, button: PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(3);

    // Context menu is open: verify drawing was NOT opened accidentally!
    assert!(h.state().session.docs.is_empty(), "Secondary click must not open the drawing");

    // The context menu contains "Open", "Remove from Recents", and "Move to Trash..."
    assert!(h.query_by_label("Open").is_some());
    assert!(h.query_by_label("Remove from Recents").is_some());
    assert!(h.query_by_label("Move to Trash...").is_some());

    // Click "Remove from Recents"
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Remove from Recents").click();
    h.run_steps(3);

    // Verify file is removed from recents, but file remains on disk
    assert!(!h.state().ui.recent_files.contains(&path));
    assert!(std::path::Path::new(&path).exists());
    assert!(h.state().session.docs.is_empty(), "Drawing was not opened");
}

#[test]
fn test_start_page_list_view_context_menu_does_not_open_drawing() {
    let path = make_temp_file("test_list_ctx_menu", "dwg");
    let mut app = CadApp::new(Session::empty(), Services::default());
    app.remember_recent(path.clone());
    app.ui.start_grid_view = false; // List view

    let mut h = Harness::builder().with_size(vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);

    // Find row location on screen
    let mut found = Vec::new();
    for s in &h.output().shapes {
        texts(&s.shape, &mut found);
    }
    let pos = found.iter().find(|(t, _)| t == "test_list_ctx_menu.dwg").expect("row filename text").1.center();

    // Right-click on the list row
    h.hover_at(pos);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos, button: PointerButton::Secondary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos, button: PointerButton::Secondary, pressed: false, modifiers: Default::default() });
    h.run_steps(3);

    assert!(h.state().session.docs.is_empty(), "Secondary click in list view must not open drawing");
    assert!(h.query_by_label("Open").is_some());
    assert!(h.query_by_label("Remove from Recents").is_some());
    assert!(h.query_by_label("Move to Trash...").is_some());
}
