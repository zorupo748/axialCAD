//! Recently opened files (#858): File ▸ Open Recent and the Start page list the drawings opened
//! from a path, newest first, and keep them across restarts.

use cadcraft_engine::Session;
use cadcraft_engine::cmd::file::{IoHooks, set_io};
use cadcraft_engine::doc::Drawing;
use cadcraft_ui_egui::menus::{self, Entry};
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Rect, Shape, vec2};
use egui_kittest::Harness;

/// Two drawing files on disk (the reader below accepts anything, so their contents don't matter).
fn drawings(test: &str) -> (String, String) {
    set_io(IoHooks { read: |_, _| Ok(Drawing::new_metric()), write: |_, _| Ok(Vec::new()), plot: None });
    cadcraft_engine::cmd::file::set_write_as(|_, _, _| Ok(Vec::new()));
    let dir = std::env::temp_dir().join(format!("cadcraft-recent-{test}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = |name: &str| {
        let p = dir.join(name);
        std::fs::write(&p, b"0\nEOF\n").unwrap();
        p.to_string_lossy().to_string()
    };
    (path("plan.dxf"), path("section.dwg"))
}

/// Wait for the drawings being read in the background to be added (or reported).
fn settle(app: &mut CadApp) {
    let t = std::time::Instant::now();
    while app.opening.busy() && t.elapsed() < std::time::Duration::from_secs(10) {
        app.finish_opening();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    app.finish_opening();
}

/// File ▸ Open Recent: (label, command id) of each item.
fn open_recent(app: &CadApp) -> Vec<(String, String)> {
    let file = menus::tree(app).into_iter().find(|(m, _)| m == "File").map(|(_, e)| e).unwrap_or_default();
    let Some(Entry::Sub { children, .. }) = file.into_iter().find(|e| matches!(e, Entry::Sub { label, .. } if label == "Open Recent")) else {
        return Vec::new();
    };
    children
        .into_iter()
        .filter_map(|e| match e {
            Entry::Item { label, id, .. } => Some((label, id)),
            Entry::Sub { .. } => None,
        })
        .collect()
}

#[test]
fn opened_drawings_are_listed_newest_first_and_survive_a_restart() {
    let (plan, section) = drawings("menu");
    let mut app = CadApp::new(Session::empty(), Services::default());
    assert!(app.ui.recent_files.is_empty());

    app.open_path(&plan);
    settle(&mut app);
    app.open_path(&section);
    settle(&mut app);
    app.open_path(&plan);
    settle(&mut app);
    assert_eq!(app.ui.recent_files, [plan.clone(), section.clone()], "newest first, each file once");
    // A file that could not be opened is not listed.
    app.open_path("/nonexistent/missing.dxf");
    settle(&mut app);
    assert_eq!(app.ui.recent_files, [plan.clone(), section.clone()]);

    let items = open_recent(&app);
    let labels: Vec<_> = items.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(labels, ["plan.dxf", "section.dwg", "Clear Recent Files"]);

    // Choosing an item opens that file and moves it to the top.
    let before = app.session.docs.len();
    menus::activate(&mut app, &items[1].1);
    settle(&mut app);
    assert_eq!(app.session.docs.len(), before + 1);
    assert_eq!(app.session.state().ok().and_then(|s| s.path.clone()), Some(section.clone()));
    assert_eq!(app.ui.recent_files, [section.clone(), plan.clone()]);

    // Kept with the preferences the host stores across restarts.
    let mut restarted = CadApp::new(Session::empty(), Services::default());
    restarted.load_prefs(&app.prefs_json());
    assert_eq!(restarted.ui.recent_files, [section.clone(), plan.clone()]);

    let clear = items.iter().find(|(l, _)| l == "Clear Recent Files").map(|(_, id)| id.clone()).unwrap();
    menus::activate(&mut app, &clear);
    assert!(app.ui.recent_files.is_empty());
    assert_eq!(open_recent(&app).iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(), ["Clear Recent Files"]);
}

/// Painted texts and where they are.
fn texts(shape: &Shape, out: &mut Vec<(String, Rect)>) {
    match shape {
        Shape::Text(t) => out.push((t.galley.text().to_string(), t.visual_bounding_rect())),
        Shape::Vec(v) => v.iter().for_each(|s| texts(s, out)),
        _ => {}
    }
}

#[test]
fn start_page_lists_recent_files_and_opens_one_on_click() {
    let (plan, section) = drawings("start");
    let mut app = CadApp::new(Session::empty(), Services::default());
    app.ui.recent_files = vec![section.clone(), plan.clone()];
    let mut h = Harness::builder().with_size(vec2(1200.0, 760.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);
    let mut found = Vec::new();
    for s in &h.output().shapes {
        texts(&s.shape, &mut found);
    }
    assert!(!found.iter().any(|(t, _)| t == "There is nothing here yet."), "{found:?}");
    let names: Vec<_> = found.iter().filter(|(t, _)| t == "section.dwg" || t == "plan.dxf").collect();
    assert_eq!(names.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(), ["section.dwg", "plan.dxf"], "newest first");

    let at = names[1].1.center();
    h.hover_at(at);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);
    let t = std::time::Instant::now();
    while h.state().opening.busy() && t.elapsed() < std::time::Duration::from_secs(10) {
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run_steps(1);
    }
    h.run_steps(1);
    let app = h.state();
    assert_eq!(app.session.state().ok().and_then(|s| s.path.clone()), Some(plan.clone()));
    assert_eq!(app.ui.recent_files, [plan, section]);
}

#[test]
fn start_page_shown_on_empty_session_and_new_switches_to_drawing() {
    let app = CadApp::new(Session::empty(), Services::default());
    assert!(app.session.docs.is_empty());
    let mut h = Harness::builder().with_size(vec2(1200.0, 760.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(3);
    let mut found = Vec::new();
    for s in &h.output().shapes {
        texts(&s.shape, &mut found);
    }
    assert!(found.iter().any(|(t, _)| t == "New"), "Start page offers New");
    assert!(found.iter().any(|(t, _)| t == "New (metric)"), "Start page offers New (metric)");

    // Clicking New creates a drawing and leaves the Start page
    let new_btn = found.iter().find(|(t, _)| t == "New").unwrap();
    let at = new_btn.1.center();
    h.hover_at(at);
    h.run_steps(1);
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() });
    h.event(egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() });
    h.run_steps(2);

    let app = h.state();
    assert_eq!(app.session.docs.len(), 1);
    assert!(!app.ui.start_tab, "start tab is not active once a drawing is created");
}

#[test]
fn failed_open_on_empty_session_keeps_start_page_and_reports_error() {
    let mut app = CadApp::new(Session::empty(), Services::default());
    let missing = std::env::temp_dir().join(format!("cadcraft-missing-{}.dwg", std::process::id()));
    app.open_path(&missing.to_string_lossy());
    settle(&mut app);
    assert!(app.session.docs.is_empty(), "failed open does not create an empty drawing");
    assert!(app.status.is_some(), "status bar contains the error");
}

#[test]
fn opening_lisp_on_empty_session_creates_drawing_and_loads_routine() {
    let dir = std::env::temp_dir().join(format!("cadcraft-lisp-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("test.lsp");
    std::fs::write(&path, b"(defun c:hello () (princ \"world\") (princ))").unwrap();
    let mut app = CadApp::new(Session::empty(), Services::default());
    app.open_path(&path.to_string_lossy());
    assert_eq!(app.session.docs.len(), 1, "loads into a newly created drawing");
    assert!(!app.ui.start_tab);
    app.cmdline("hello");
    assert_eq!(app.session.log.last().map(String::as_str), Some("world"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mixed_open_paths_script_first_loads_into_drawing_without_drawing1() {
    let (plan, _) = drawings("mix1");
    let lisp_dir = std::env::temp_dir().join(format!("cadcraft-lisp-mix1-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&lisp_dir);
    let script = lisp_dir.join("cmd.lsp");
    std::fs::write(&script, b"(defun c:mixedone () (princ \"mix1-ok\") (princ))").unwrap();

    let mut app = CadApp::new(Session::empty(), Services::default());
    let files = vec![script.to_string_lossy().to_string(), plan.clone()];
    app.open_paths(&files);

    // Immediately after open_paths: plan is opening in background, lisp is pending, no Drawing1
    assert!(app.session.docs.is_empty(), "no Drawing1 created while drawing reads");
    assert!(app.opening.busy());
    assert_eq!(app.pending_lisp.len(), 1);

    settle(&mut app);

    // After settle: exactly 1 drawing (plan), lisp is loaded and executed, not on start tab
    assert_eq!(app.session.docs.len(), 1);
    assert_eq!(app.session.state().ok().and_then(|s| s.path.clone()), Some(plan));
    assert!(!app.ui.start_tab);
    app.cmdline("mixedone");
    assert_eq!(app.session.log.last().map(String::as_str), Some("mix1-ok"));

    let _ = std::fs::remove_dir_all(&lisp_dir);
}

#[test]
fn mixed_open_paths_drawing_first_loads_into_drawing_without_drawing1() {
    let (plan, _) = drawings("mix2");
    let lisp_dir = std::env::temp_dir().join(format!("cadcraft-lisp-mix2-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&lisp_dir);
    let script = lisp_dir.join("cmd2.lsp");
    std::fs::write(&script, b"(defun c:mixedtwo () (princ \"mix2-ok\") (princ))").unwrap();

    let mut app = CadApp::new(Session::empty(), Services::default());
    let files = vec![plan.clone(), script.to_string_lossy().to_string()];
    app.open_paths(&files);

    assert!(app.session.docs.is_empty(), "no Drawing1 created while drawing reads");
    assert!(app.opening.busy());
    assert_eq!(app.pending_lisp.len(), 1);

    settle(&mut app);

    assert_eq!(app.session.docs.len(), 1);
    assert_eq!(app.session.state().ok().and_then(|s| s.path.clone()), Some(plan));
    assert!(!app.ui.start_tab);
    app.cmdline("mixedtwo");
    assert_eq!(app.session.log.last().map(String::as_str), Some("mix2-ok"));

    let _ = std::fs::remove_dir_all(&lisp_dir);
}

#[test]
fn drawing_fails_with_pending_lisp_keeps_start_and_reports_explicit_message() {
    let missing = std::env::temp_dir().join(format!("cadcraft-missing-{}.dwg", std::process::id()));
    let lisp_dir = std::env::temp_dir().join(format!("cadcraft-lisp-fail-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&lisp_dir);
    let script = lisp_dir.join("never_run.lsp");
    std::fs::write(&script, b"(defun c:fail () (princ \"never\") (princ))").unwrap();

    let mut app = CadApp::new(Session::empty(), Services::default());
    let files = vec![script.to_string_lossy().to_string(), missing.to_string_lossy().to_string()];
    app.open_paths(&files);

    assert!(app.session.docs.is_empty());
    assert!(app.opening.busy());
    assert_eq!(app.pending_lisp.len(), 1);

    settle(&mut app);

    // After failure: session still has 0 docs, no Drawing1
    assert!(app.session.docs.is_empty(), "no empty drawing created on failure");
    assert!(app.pending_lisp.is_empty(), "pending lisp was drained and reported");
    assert!(app.session.log.iter().any(|l| l.contains("could not be loaded because no drawing was opened")), "{:?}", app.session.log);
    assert!(app.status.as_ref().is_some_and(|(s, _)| s.contains("could not be loaded because no drawing was opened")));

    let _ = std::fs::remove_dir_all(&lisp_dir);
}

#[test]
fn opening_second_drawing_with_lisp_does_not_run_on_previous_drawing_before_opening_finishes() {
    let (first_dwg, second_dwg) = drawings("mix_exist_ok");
    let lisp_dir = std::env::temp_dir().join(format!("cadcraft-lisp-exist-ok-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&lisp_dir);
    let script = lisp_dir.join("addline.lsp");
    std::fs::write(&script, b"(command \"_.LINE\" '(0 0) '(10 10) \"\") (defun c:secondok () (princ \"ran-second\") (princ))").unwrap();

    let mut app = CadApp::new(Session::empty(), Services::default());
    app.open_path(&first_dwg);
    settle(&mut app);
    assert_eq!(app.session.docs.len(), 1);
    assert_eq!(app.session.docs[0].doc.entity_count(), 0);

    // Open second drawing and lisp together
    app.open_paths(&[second_dwg.clone(), script.to_string_lossy().to_string()]);
    assert!(app.opening.busy());
    assert_eq!(app.pending_lisp.len(), 1);

    // Even if finish_opening is called before reader finishes, routine must not execute on first drawing
    app.finish_opening();
    assert_eq!(app.session.docs[0].doc.entity_count(), 0, "first drawing untouched while second is reading");

    settle(&mut app);

    assert_eq!(app.session.docs.len(), 2);
    assert_eq!(app.session.state().ok().and_then(|s| s.path.clone()), Some(second_dwg));
    assert_eq!(app.session.docs[0].doc.entity_count(), 0, "first drawing remains untouched after second finishes");
    assert_eq!(app.session.docs[1].doc.entity_count(), 1, "line was drawn in the newly opened second drawing");

    app.cmdline("secondok");
    assert_eq!(app.session.log.last().map(String::as_str), Some("ran-second"));

    let _ = std::fs::remove_dir_all(&lisp_dir);
}

#[test]
fn failed_second_drawing_with_pending_lisp_does_not_execute_on_previous_drawing() {
    let (first_dwg, _) = drawings("mix_exist_fail");
    let lisp_dir = std::env::temp_dir().join(format!("cadcraft-lisp-exist-fail-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&lisp_dir);
    let script = lisp_dir.join("failtest.lsp");
    std::fs::write(&script, b"(command \"_.LINE\" '(0 0) '(10 10) \"\") (defun c:neverrun () (princ \"bad\") (princ))").unwrap();

    let mut app = CadApp::new(Session::empty(), Services::default());
    app.open_path(&first_dwg);
    settle(&mut app);
    assert_eq!(app.session.docs.len(), 1);
    assert_eq!(app.session.docs[0].doc.entity_count(), 0);

    let missing = std::env::temp_dir().join(format!("cadcraft-missing-{}.dwg", std::process::id()));
    app.open_paths(&[missing.to_string_lossy().to_string(), script.to_string_lossy().to_string()]);
    assert!(app.opening.busy());
    assert_eq!(app.pending_lisp.len(), 1);

    settle(&mut app);

    assert_eq!(app.session.docs.len(), 1, "only original drawing remains");
    assert_eq!(app.session.docs[0].doc.entity_count(), 0, "routine was not executed on the previous drawing");
    assert!(app.pending_lisp.is_empty(), "pending lisp was drained");
    assert!(app.session.log.iter().any(|l| l.contains("could not be loaded because no drawing was opened")), "{:?}", app.session.log);
    assert!(app.status.as_ref().is_some_and(|(s, _)| s.contains("could not be loaded because no drawing was opened")));

    let _ = std::fs::remove_dir_all(&lisp_dir);
}

#[test]
fn test_thumbnails_and_identical_names_no_collision() {
    let dir = std::env::temp_dir().join(format!("cadcraft-thumb-test-{}", std::process::id()));
    let dir_a = dir.join("proj_a");
    let dir_b = dir.join("proj_b");
    let _ = std::fs::create_dir_all(&dir_a);
    let _ = std::fs::create_dir_all(&dir_b);
    let path_a = dir_a.join("detail.dwg");
    let path_b = dir_b.join("detail.dwg");
    std::fs::write(&path_a, b"sample_a").unwrap();
    std::fs::write(&path_b, b"sample_b").unwrap();

    let str_a = path_a.to_string_lossy().to_string();
    let str_b = path_b.to_string_lossy().to_string();

    let thumb_a = cadcraft_ui_egui::thumbnails::thumbnail_path(&str_a);
    let thumb_b = cadcraft_ui_egui::thumbnails::thumbnail_path(&str_b);

    assert_ne!(thumb_a, thumb_b, "different paths with the same filename must have distinct thumbnail cache files");

    // Render a thumbnail for a drawing
    let drawing = Drawing::new_metric();
    cadcraft_ui_egui::thumbnails::update_thumbnail(&drawing, None, &str_a);
    assert!(thumb_a.exists(), "thumbnail PNG file was generated and saved to cache");
    assert!(thumb_a.metadata().unwrap().len() > 0, "thumbnail PNG has content");

    // Check existence and modified time
    assert!(cadcraft_ui_egui::thumbnails::file_exists(&str_a));
    assert!(!cadcraft_ui_egui::thumbnails::file_exists("/nonexistent/file.dwg"));
    let mtime = cadcraft_ui_egui::thumbnails::file_modified_time(&str_a);
    assert!(!mtime.is_empty(), "valid timestamp string produced");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_start_page_pinned_and_grid_view_preferences_persist() {
    let mut app = CadApp::new(Session::empty(), Services::default());
    let path1 = "/path/to/alpha.dwg".to_string();
    let path2 = "/path/to/beta.dwg".to_string();

    app.remember_recent(path1.clone());
    app.remember_recent(path2.clone());
    app.toggle_pinned(&path1);
    app.ui.start_grid_view = false; // toggle to list

    assert!(app.ui.pinned_files.contains(&path1));
    assert!(!app.ui.pinned_files.contains(&path2));
    assert!(!app.ui.start_grid_view);

    // Preferences JSON serialization
    let prefs_str = app.prefs_json();
    let prefs = serde_json::from_str::<serde_json::Value>(&prefs_str).unwrap();
    let pinned = prefs.get("pinnedFiles").and_then(serde_json::Value::as_array).unwrap();
    assert_eq!(pinned.len(), 1);
    assert_eq!(pinned[0].as_str(), Some("/path/to/alpha.dwg"));
    assert_eq!(prefs.get("startGridView").and_then(serde_json::Value::as_bool), Some(false));

    // Reload into a fresh app
    let mut app2 = CadApp::new(Session::empty(), Services::default());
    app2.load_prefs(&prefs_str);
    assert!(app2.ui.pinned_files.contains(&path1));
    assert_eq!(app2.ui.start_grid_view, false);

    // Forget recent
    app2.forget_recent(&path1);
    assert!(!app2.ui.recent_files.contains(&path1));
    assert!(!app2.ui.pinned_files.contains(&path1));
}

#[test]
fn test_thumbnail_invalidation_and_session_update() {
    let mut app = CadApp::new(Session::empty(), Services::default());
    let path = "/tmp/test_card_render.dwg".to_string();

    // Invalidation removes texture handle from app.thumbnail_textures
    app.invalidate_thumbnail(&path);
    assert!(!app.thumbnail_textures.contains_key(&path));
}

#[test]
fn test_save_coverage_menu_shortcut_cmdline() {
    set_io(IoHooks { read: |_, _| Ok(Drawing::new_metric()), write: |_, _| Ok(Vec::new()), plot: None });
    cadcraft_engine::cmd::file::set_write_as(|_, _, _| Ok(Vec::new()));
    let dir = std::env::temp_dir().join(format!("cadcraft-save-cov-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let p_saveas = dir.join("menu_saved.dxf").to_string_lossy().to_string();
    let p_cmdline = dir.join("cmd_saved.dxf").to_string_lossy().to_string();

    // 1. Save via menu pick_save (ui.saveas)
    let p_saveas_clone = p_saveas.clone();
    let services = Services { pick_save: Some(Box::new(move |_| Some(p_saveas_clone.clone()))), ..Services::default() };
    let mut app = CadApp::new(Session::default(), services);
    app.session.new_drawing(true);
    let res = app.run("ui.saveas", serde_json::Value::Null);
    assert!(res.is_ok(), "{:?}", res);
    assert_eq!(app.ui.recent_files[0], p_saveas);
    assert!(cadcraft_ui_egui::thumbnails::thumbnail_path(&p_saveas).exists());

    // 2. Save via qsave shortcut (when drawing already has path)
    let res2 = app.run("qsave", serde_json::json!({}));
    assert!(res2.is_ok());
    assert_eq!(app.ui.recent_files[0], p_saveas);

    // 3. Save via command line string (JSON form)
    let res3 = app.cmdline(&format!(r#"saveas {{"path": "{}"}}"#, p_cmdline));
    assert!(res3.is_none(), "cmdline returned no error: {:?}", res3);
    assert_eq!(app.ui.recent_files[0], p_cmdline);
    assert!(cadcraft_ui_egui::thumbnails::thumbnail_path(&p_cmdline).exists());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_failed_save_does_not_update_thumbnail_or_recents() {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.session.new_drawing(true);
    let invalid_path = "/nonexistent_root_dir_12345/unwritable_subfolder/draw.dxf";

    let res = app.run("saveas", serde_json::json!({ "path": invalid_path }));
    assert!(res.is_err(), "saving to invalid path must fail");

    // Recents must NOT contain the failed path
    assert!(!app.ui.recent_files.contains(&invalid_path.to_string()));
    assert!(!cadcraft_ui_egui::thumbnails::thumbnail_path(invalid_path).exists());
}

#[test]
fn test_corrupt_or_missing_cache_does_not_crash_or_block() {
    let ctx = egui::Context::default();
    let missing_path = "/nonexistent/test_never_cached.dwg";
    assert!(cadcraft_ui_egui::thumbnails::load_thumbnail_texture(&ctx, missing_path).is_none());

    // Corrupt PNG file
    let corrupt_path = "/tmp/test_corrupt_thumb.dwg";
    let thumb_file = cadcraft_ui_egui::thumbnails::thumbnail_path(corrupt_path);
    if let Some(parent) = thumb_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&thumb_file, b"THIS IS NOT A VALID PNG HEADER").unwrap();

    // Must return None safely without panic
    assert!(cadcraft_ui_egui::thumbnails::load_thumbnail_texture(&ctx, corrupt_path).is_none());
    let _ = std::fs::remove_file(thumb_file);
}

#[test]
fn test_cache_pruning_limits() {
    let mut app = CadApp::new(Session::empty(), Services::default());
    app.prune_thumbnail_textures();

    // Test disk cache pruning
    cadcraft_ui_egui::thumbnails::prune_disk_cache(100);
}

#[test]
fn test_pinned_favorites_preserved_when_overflowing_recents() {
    let mut app = CadApp::new(Session::empty(), Services::default());
    let fav = "/drawings/my_favorite_project.dwg".to_string();

    // Remember and pin favorite
    app.remember_recent(fav.clone());
    app.toggle_pinned(&fav);
    assert!(app.ui.pinned_files.contains(&fav));
    assert!(app.ui.recent_files.contains(&fav));

    // Open 15 other files to push fav out of recent_files (limit is RECENT_FILES = 9)
    for i in 1..=15 {
        app.remember_recent(format!("/drawings/other_drawing_{i}.dwg"));
    }

    // Recent files is capped at RECENT_FILES
    assert_eq!(app.ui.recent_files.len(), cadcraft_ui_egui::RECENT_FILES);
    assert!(!app.ui.recent_files.contains(&fav), "fav dropped out of recent_files");

    // But pinned_files retains it!
    assert!(app.ui.pinned_files.contains(&fav), "fav stays pinned");

    // And Start page combined list still contains fav as the first item!
    let mut combined: Vec<String> = Vec::new();
    for p in &app.ui.pinned_files {
        if !combined.contains(p) {
            combined.push(p.clone());
        }
    }
    for p in &app.ui.recent_files {
        if !combined.contains(p) {
            combined.push(p.clone());
        }
    }
    assert_eq!(combined[0], fav, "pinned favorite is kept at the top of Start page");
}
