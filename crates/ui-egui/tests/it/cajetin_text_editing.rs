//! Integration tests for cajetín text editing: double-click, in-place edit, accept,
//! cancel, undo/redo, property preservation, and block attribute isolation.

use cadcraft_doc::EntityKind;
use cadcraft_engine::geom::Vec2;
use cadcraft_engine::{Input, Session};
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, vec2};
use egui_kittest::Harness;
use serde_json::json;

fn double_click(h: &mut Harness<'_, CadApp>, at: Vec2) {
    let pos: Pos2 = h.state().canvas.xf.unwrap().to_screen(at);
    h.event(Event::PointerMoved(pos));
    h.run_steps(2);
    for _ in 0..2 {
        for pressed in [true, false] {
            h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
            h.run_steps(1);
        }
    }
    h.run_steps(2);
}

#[test]
fn test_textedit_double_click_paper_space_and_in_place_accept() {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.run("layout.set", json!({ "name": "Layout1" })).unwrap();
    // Add cajetín-like text in paper space
    app.run("text", json!({
        "at": [65.33, 26.64],
        "text": "1:1",
        "height": 2.5,
        "layer": "0"
    })).unwrap();

    let doc = app.session.doc().unwrap();
    let text_h = doc.space(&app.session.space()).unwrap().iter().find_map(|e| match &e.kind {
        EntityKind::Text(t) if t.value == "1:1" => Some(e.handle),
        _ => None,
    }).unwrap();

    let orig_text = match &app.session.doc().unwrap().entity(text_h).unwrap().kind {
        EntityKind::Text(t) => t.clone(),
        _ => panic!("expected text"),
    };

    app.session.execute("view.set", &json!({ "center": [65.33, 26.64], "height": 30 })).unwrap();
    app.canvas.zoom_pending = false;

    let mut h = Harness::builder().with_size(vec2(1200.0, 800.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);

    // Double-click on the text on screen
    double_click(&mut h, Vec2::new(65.33, 26.64));

    // Verify textedit has started and is editing text_h
    assert_eq!(h.state().session.running.as_ref().map(|r| r.id.as_str()), Some("textedit"));
    assert_eq!(h.state().session.editing_text_entity(), Some(text_h));

    // Type new text "1:5" and press Enter
    h.event(Event::Key { key: Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.state_mut().session.input(Input::Text("1:5".into())).unwrap();
    h.run_steps(2);

    // Verify the text changed
    let updated_e = h.state().session.doc().unwrap().entity(text_h).unwrap();
    match &updated_e.kind {
        EntityKind::Text(t) => {
            assert_eq!(t.value, "1:5");
            assert_eq!(t.insert, orig_text.insert, "Position must be preserved");
            assert_eq!(t.height, orig_text.height, "Height must be preserved");
            assert_eq!(t.rotation, orig_text.rotation, "Rotation must be preserved");
            assert_eq!(t.style, orig_text.style, "Style must be preserved");
        }
        _ => panic!("expected text"),
    }
}

#[test]
fn test_textedit_cancel_leaves_document_unmodified() {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.run("layout.set", json!({ "name": "Layout1" })).unwrap();
    app.run("text", json!({ "at": [100.0, 50.0], "text": "TITULO", "height": 5.0 })).unwrap();

    let doc = app.session.doc().unwrap();
    let text_h = doc.space(&app.session.space()).unwrap().iter().find_map(|e| match &e.kind {
        EntityKind::Text(t) if t.value == "TITULO" => Some(e.handle),
        _ => None,
    }).unwrap();

    app.session.execute("view.set", &json!({ "center": [100.0, 50.0], "height": 30 })).unwrap();
    app.canvas.zoom_pending = false;

    let mut h = Harness::builder().with_size(vec2(1200.0, 800.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);

    double_click(&mut h, Vec2::new(100.0, 50.0));
    assert_eq!(h.state().session.running.as_ref().map(|r| r.id.as_str()), Some("textedit"));

    // Press Escape to cancel
    h.event(Event::Key { key: Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.state_mut().session.cancel();
    h.run_steps(2);

    assert!(h.state().session.running.is_none());
    let e = h.state().session.doc().unwrap().entity(text_h).unwrap();
    match &e.kind {
        EntityKind::Text(t) => assert_eq!(t.value, "TITULO", "Value must remain unchanged after cancel"),
        _ => panic!("expected text"),
    }
}

#[test]
fn test_textedit_undo_and_redo() {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.run("layout.set", json!({ "name": "Layout1" })).unwrap();
    app.run("text", json!({ "at": [20.0, 20.0], "text": "ORIGINAL", "height": 2.5 })).unwrap();

    let doc = app.session.doc().unwrap();
    let text_h = doc.space(&app.session.space()).unwrap().iter().find_map(|e| match &e.kind {
        EntityKind::Text(t) if t.value == "ORIGINAL" => Some(e.handle),
        _ => None,
    }).unwrap();

    // Edit to MODIFICADO
    app.run("textedit", json!({ "handle": text_h.hex(), "text": "MODIFICADO" })).unwrap();
    let e = app.session.doc().unwrap().entity(text_h).unwrap();
    assert_eq!(match &e.kind { EntityKind::Text(t) => &t.value, _ => "" }, "MODIFICADO");

    // Undo restores ORIGINAL
    app.run("u", json!({})).unwrap();
    let e = app.session.doc().unwrap().entity(text_h).unwrap();
    assert_eq!(match &e.kind { EntityKind::Text(t) => &t.value, _ => "" }, "ORIGINAL");

    // Redo restores MODIFICADO
    app.run("redo", json!({})).unwrap();
    let e = app.session.doc().unwrap().entity(text_h).unwrap();
    assert_eq!(match &e.kind { EntityKind::Text(t) => &t.value, _ => "" }, "MODIFICADO");
}

#[test]
fn test_textedit_preselection_picks_selected_text() {
    let mut app = CadApp::new(Session::new(), Services::default());
    app.run("layout.set", json!({ "name": "Layout1" })).unwrap();
    app.run("text", json!({ "at": [30.0, 30.0], "text": "ESCALA", "height": 2.5 })).unwrap();

    let doc = app.session.doc().unwrap();
    let text_h = doc.space(&app.session.space()).unwrap().iter().find_map(|e| match &e.kind {
        EntityKind::Text(t) if t.value == "ESCALA" => Some(e.handle),
        _ => None,
    }).unwrap();

    // Select text beforehand
    app.session.set_selection(vec![text_h]);

    // Start textedit (e.g. from menu or shortcut)
    app.start("textedit");
    assert_eq!(app.session.editing_text_entity(), Some(text_h), "Preselected text must be adopted immediately");

    // Enter new value
    app.session.input(Input::Text("ESCALA GRÁFICA".into())).unwrap();
    let e = app.session.doc().unwrap().entity(text_h).unwrap();
    assert_eq!(match &e.kind { EntityKind::Text(t) => &t.value, _ => "" }, "ESCALA GRÁFICA");
}

#[test]
fn test_double_click_block_with_attributes_starts_attedit() {
    let mut app = CadApp::new(Session::new(), Services::default());
    // Create a block definition with attribute
    let res = app.run("-attdef", json!({ "tag": "TITULO", "prompt": "Titulo", "default": "PROYECTO", "at": [0, 0] })).unwrap();
    let att_h = res.get("handle").unwrap().as_str().unwrap();
    app.run("-block", json!({ "name": "CAJETIN_BLK", "base": [0, 0], "handles": [att_h] })).unwrap();

    // Insert two instances of the block
    app.run("-insert", json!({ "name": "CAJETIN_BLK", "at": [10, 10], "attribs": { "TITULO": "PLANO 1" } })).unwrap();
    app.run("-insert", json!({ "name": "CAJETIN_BLK", "at": [50, 10], "attribs": { "TITULO": "PLANO 2" } })).unwrap();

    let doc = app.session.doc().unwrap();
    let inserts: Vec<_> = doc.model.iter().filter_map(|e| match &e.kind {
        EntityKind::Insert(i) if i.block == "CAJETIN_BLK" => Some((e.handle, i.clone())),
        _ => None,
    }).collect();
    assert_eq!(inserts.len(), 2);
    let (h1, _) = inserts[0];
    let (h2, _) = inserts[1];

    // Double-click on first block reference
    app.session.execute("view.set", &json!({ "center": [10, 10], "height": 30 })).unwrap();
    app.canvas.zoom_pending = false;

    let mut h = Harness::builder().with_size(vec2(1200.0, 800.0)).with_step_dt(1.0 / 60.0).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);

    double_click(&mut h, Vec2::new(10.0, 10.0));
    assert_eq!(h.state().session.running.as_ref().map(|r| r.id.as_str()), Some("attedit"), "Double click on block with attributes must start attedit");

    // Edit first instance attribute via attedit
    h.state_mut().run("attedit", json!({ "handle": h1.hex(), "values": { "TITULO": "MODIFICADO 1" } })).unwrap();

    // Verify isolation: h1 changed, h2 remained unchanged
    let doc = h.state().session.doc().unwrap();
    let ins1 = match &doc.entity(h1).unwrap().kind { EntityKind::Insert(i) => i.clone(), _ => panic!() };
    let ins2 = match &doc.entity(h2).unwrap().kind { EntityKind::Insert(i) => i.clone(), _ => panic!() };

    assert_eq!(ins1.attribs[0].text.value, "MODIFICADO 1");
    assert_eq!(ins2.attribs[0].text.value, "PLANO 2", "Other block reference must not be modified");
}
