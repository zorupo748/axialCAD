//! Properties palette numeric fields (#849): typed values are read as the command line reads
//! them (feet and inches included), distances are shown in the drawing's units, and text that
//! isn't a number is reported instead of being dropped.

use cadcraft_engine::Session;
use cadcraft_engine::doc::EntityKind;
use cadcraft_ui_egui::{CadApp, Services};
use egui::{Key, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{Queryable, by};
use serde_json::json;

/// A selected TEXT 2.5 high, with lengths in `lunits`.
fn palettes(lunits: i64) -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.ui.show_palettes = true;
    app.run("setvar", json!({ "name": "LUNITS", "value": lunits })).unwrap();
    let h = app.run("text", json!({ "at": [0, 0], "text": "Note", "height": 2.5 })).unwrap();
    app.run("select", json!({ "handles": [h["handle"]] })).unwrap();
    let mut h = Harness::builder().with_size(vec2(400.0, 1400.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            cadcraft_ui_egui::palettes::right_palettes(app, ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

fn height(app: &CadApp) -> f64 {
    let d = app.session.doc().unwrap();
    d.model.iter().find_map(|e| if let EntityKind::Text(t) = &e.kind { Some(t.height) } else { None }).unwrap()
}

/// The text box showing `shown` (its text run child carries the same value).
fn field<'h>(h: &'h Harness<'static, CadApp>, shown: &'h str) -> egui_kittest::Node<'h> {
    h.get(by().role(egui::accesskit::Role::TextInput).value(shown))
}

fn type_into(h: &mut Harness<'static, CadApp>, shown: &str, text: &str) {
    field(h, shown).focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    field(h, shown).type_text(text);
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
}

#[test]
fn text_height_takes_feet_and_inches() {
    let mut h = palettes(2);
    type_into(&mut h, "2.5000", "3'");
    assert_eq!(height(h.state()), 36.0);
    type_into(&mut h, "36.0000", "1'6 1/2\"");
    assert_eq!(height(h.state()), 18.5);
}

#[test]
fn text_height_is_shown_in_the_drawing_units() {
    let mut h = palettes(4);
    type_into(&mut h, "0'-2 1/2\"", "3'6\"");
    assert_eq!(height(h.state()), 42.0);
    field(&h, "3'-6\"");
}

#[test]
fn unreadable_text_is_reported_not_dropped() {
    let mut h = palettes(2);
    let before = h.state().session.log.len();
    type_into(&mut h, "2.5000", "abc");
    assert_eq!(height(h.state()), 2.5);
    let log = &h.state().session.log[before..];
    assert!(log.iter().any(|l| l.contains("abc")), "{log:?}");
    field(&h, "2.5000");
}

fn properties_harness(app: CadApp) -> Harness<'static, CadApp> {
    let mut h = Harness::builder().with_size(vec2(420.0, 1400.0)).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            cadcraft_ui_egui::palettes::right_palettes(app, ui);
        },
        app,
    );
    h.run_steps(4);
    h
}

#[test]
fn leader_properties_no_selection_adjusts_new_leaders_and_rejects_invalid() {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.ui.show_palettes = true;
    // Draw an existing leader with default 2.0 first.
    let existing = app.run("directrizzr", json!({ "points": [[0, 0], [20, 0]] })).unwrap()["handle"]
        .as_str()
        .unwrap()
        .to_string();

    let mut h = properties_harness(app);
    // Reads effective metric style value (2.0000), not hardcoded.
    field(&h, "2.0000");
    // Shows Millimeters (metric), not a wrong unit.
    assert!(h.query_by_label_contains("Millimeters").is_some());

    // Wide positive decimal range works directly.
    type_into(&mut h, "2.0000", "12.75");
    field(&h, "12.7500");
    assert!((h.state().session.doc().unwrap().mleader_styles[0].arrow_size - 12.75).abs() < 1e-9);
    assert!(h.state().session.state().unwrap().is_dirty());

    // Invalid inputs (0, negative, non-numeric) are rejected and revert display.
    for bad in ["0", "-5", "abc"] {
        let before = h.state().session.log.len();
        type_into(&mut h, "12.7500", bad);
        assert!((h.state().session.doc().unwrap().mleader_styles[0].arrow_size - 12.75).abs() < 1e-9);
        assert!(h.state().session.log.len() > before, "expected validation message for {bad}");
        field(&h, "12.7500");
    }

    // Existing drawn leader was not modified; new directrizzr uses 12.75.
    let d = h.state().session.doc().unwrap();
    let existing_h = cadcraft_engine::doc::Handle::parse_hex(&existing).unwrap();
    let EntityKind::MLeader(old_ml) = &d.entity(existing_h).unwrap().kind else { panic!("expected MLeader") };
    assert!((old_ml.arrow_size - 2.0).abs() < 1e-9);

    let created = h.state_mut().run("directrizzr", json!({ "points": [[0, 30], [20, 30]] })).unwrap()["handle"]
        .as_str()
        .unwrap()
        .to_string();
    let created_h = cadcraft_engine::doc::Handle::parse_hex(&created).unwrap();
    let EntityKind::MLeader(new_ml) = &h.state().session.doc().unwrap().entity(created_h).unwrap().kind else {
        panic!("expected MLeader")
    };
    assert!((new_ml.arrow_size - 12.75).abs() < 1e-9);
}

#[test]
fn leader_properties_selected_edits_only_selected_with_undo_redo_and_units() {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.ui.show_palettes = true;
    let h1 = app.run("directrizzr", json!({ "points": [[0, 0], [20, 0]] })).unwrap()["handle"].clone();
    let h2 = app.run("mleader", json!({ "points": [[0, 20], [20, 20]], "text": "A" })).unwrap()["handle"].clone();
    app.run("select", json!({ "handles": [h1.clone()] })).unwrap();

    let mut h = properties_harness(app);
    field(&h, "2.0000");
    type_into(&mut h, "2.0000", "42.5");
    field(&h, "42.5000");

    let handle1 = cadcraft_engine::doc::Handle::parse_hex(h1.as_str().unwrap()).unwrap();
    let handle2 = cadcraft_engine::doc::Handle::parse_hex(h2.as_str().unwrap()).unwrap();
    {
        let d = h.state().session.doc().unwrap();
        let EntityKind::MLeader(m1) = &d.entity(handle1).unwrap().kind else { panic!("expected MLeader") };
        let EntityKind::MLeader(m2) = &d.entity(handle2).unwrap().kind else { panic!("expected MLeader") };
        assert!((m1.arrow_size - 42.5).abs() < 1e-9);
        assert!((m2.arrow_size - 2.0).abs() < 1e-9, "sibling leader must remain unchanged");
        assert!((d.mleader_styles[0].arrow_size - 2.0).abs() < 1e-9, "shared style must remain unchanged");
    }

    // Undo and redo update the entity and displayed UI field immediately (undo/redo clears selection).
    h.state_mut().run("u", serde_json::Value::Null).unwrap();
    {
        let EntityKind::MLeader(m1) = &h.state().session.doc().unwrap().entity(handle1).unwrap().kind else {
            panic!("expected MLeader")
        };
        assert!((m1.arrow_size - 2.0).abs() < 1e-9);
    }
    h.state_mut().run("select", json!({ "handles": [h1.clone()] })).unwrap();
    h.run_steps(3);
    field(&h, "2.0000");

    h.state_mut().run("redo", serde_json::Value::Null).unwrap();
    {
        let EntityKind::MLeader(m1) = &h.state().session.doc().unwrap().entity(handle1).unwrap().kind else {
            panic!("expected MLeader")
        };
        assert!((m1.arrow_size - 42.5).abs() < 1e-9);
    }
    h.state_mut().run("select", json!({ "handles": [h1.clone()] })).unwrap();
    h.run_steps(3);
    field(&h, "42.5000");

    // Imperial drawing shows Inches (0.1800) and never labels as mm; unitless never labels as mm.
    let mut imp_app = CadApp::new(Session::default(), Services::default());
    imp_app.session.new_drawing(false);
    imp_app.ui.show_palettes = true;
    let imp_h = properties_harness(imp_app);
    field(&imp_h, "0.1800");
    assert!(imp_h.query_by_label_contains("Inches").is_some());
    assert!(imp_h.query_by_label_contains("u (in)").is_some());
    assert!(imp_h.query_by_label_contains("mm").is_none());

    let mut unitless_app = CadApp::new(Session::default(), Services::default());
    unitless_app.ui.show_palettes = true;
    unitless_app.run("setvar", json!({ "name": "INSUNITS", "value": 0 })).unwrap();
    let unitless_h = properties_harness(unitless_app);
    assert!(unitless_h.query_by_label_contains("Unitless").is_some());
    assert!(unitless_h.query_by_label_contains("mm").is_none());

    // Leader Properties entry opens the Properties Inspector panel.
    let mut menu_app = CadApp::new(Session::default(), Services::default());
    assert!(!menu_app.ui.docking.layout.contains(&cadcraft_ui_egui::docking::Panel::Properties));
    menu_app.start("ui.panel.leaderproperties");
    assert!(menu_app.ui.docking.layout.contains(&cadcraft_ui_egui::docking::Panel::Properties));
}

