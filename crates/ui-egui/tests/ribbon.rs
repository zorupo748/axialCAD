//! Integration tests for the Ribbon top panel (cinta de opciones superior).
//! Verifies ribbon visibility, tab switching, collapsing, command routing,
//! workspace persistence, responsive rendering with narrow windows,
//! and coexistence with Tool Sets and palettes.

use cadcraft_engine::Session;
use cadcraft_ui_egui::{CadApp, Services, menus, ribbon};
use egui::vec2;
use egui_kittest::Harness;
use serde_json::{Value, json};

fn harness(size: egui::Vec2) -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.ui.start_tab = false;
    app.ui.in_window_menu = false;
    let mut h = Harness::builder().with_size(size).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    h.run_steps(5);
    h
}

#[test]
fn ribbon_default_state_and_commands() {
    let app = CadApp::new(Session::default(), Services::default());
    assert!(app.ui.show_ribbon, "Ribbon is visible by default");
    assert_eq!(app.ui.ribbon_tab, "Home", "Default ribbon tab is Home");
    assert!(!app.ui.ribbon_collapsed, "Ribbon is expanded by default");

    assert!(menus::is_ui_command("ui.toggle.ribbon"));
    assert!(menus::is_ui_command("ribbon"));
    assert!(menus::is_ui_command("ribbonclose"));
}

#[test]
fn ribbon_toggle_and_cmdline_commands() {
    let mut app = CadApp::new(Session::default(), Services::default());

    // Toggle via ui.toggle.ribbon
    app.run("ui.toggle.ribbon", Value::Null).unwrap();
    assert!(!app.ui.show_ribbon);

    app.run("ui.toggle.ribbon", Value::Null).unwrap();
    assert!(app.ui.show_ribbon);

    // Explicit on/off via params
    app.run("ui.toggle.ribbon", json!({ "on": false })).unwrap();
    assert!(!app.ui.show_ribbon);

    app.run("ui.toggle.ribbon", json!({ "on": true })).unwrap();
    assert!(app.ui.show_ribbon);

    // AutoCAD command line commands: RIBBONCLOSE and RIBBON
    assert!(app.cmdline("RIBBONCLOSE").is_none());
    assert!(!app.ui.show_ribbon);

    assert!(app.cmdline("RIBBON").is_none());
    assert!(app.ui.show_ribbon);
    assert!(!app.ui.ribbon_collapsed);
}

#[test]
fn ribbon_tab_switching_and_collapse() {
    let mut app = CadApp::new(Session::default(), Services::default());

    for tab in ribbon::TABS {
        app.ui.ribbon_tab = tab.id.to_string();
        assert_eq!(app.ui.ribbon_tab, tab.id);
    }

    app.ui.ribbon_collapsed = true;
    assert!(app.ui.ribbon_collapsed);

    // Running RIBBON should uncollapse and show
    app.cmdline("ribbon");
    assert!(app.ui.show_ribbon);
    assert!(!app.ui.ribbon_collapsed);
}

#[test]
fn ribbon_renders_in_normal_and_narrow_windows() {
    // Normal 1280x800 window
    let mut h_normal = harness(vec2(1280.0, 800.0));
    h_normal.run_steps(5);
    assert!(h_normal.state().ui.show_ribbon);

    // Narrow 480x800 window (scroll area must absorb overflow without panic)
    let mut h_narrow = harness(vec2(480.0, 800.0));
    h_narrow.run_steps(5);
    assert!(h_narrow.state().ui.show_ribbon);

    // Switch all tabs in narrow window
    for tab in ribbon::TABS {
        h_narrow.state_mut().ui.ribbon_tab = tab.id.to_string();
        h_narrow.run_steps(3);
    }

    // Collapsed ribbon in narrow window
    h_narrow.state_mut().ui.ribbon_collapsed = true;
    h_narrow.run_steps(3);
}

#[test]
fn toolsets_and_ribbon_coexist_independently() {
    let mut app = CadApp::new(Session::default(), Services::default());

    // Ribbon is visible by default while Tool Sets is hidden to maximize drawing space
    assert!(app.ui.show_ribbon);
    assert!(!app.ui.show_toolsets);

    // Show Tool Sets: both are now visible simultaneously
    app.run("ui.toggle.toolsets", json!({ "on": true })).unwrap();
    assert!(app.ui.show_toolsets);
    assert!(app.ui.show_ribbon);

    // Hide Tool Sets: ribbon remains visible
    app.run("ui.toggle.toolsets", json!({ "on": false })).unwrap();
    assert!(!app.ui.show_toolsets);
    assert!(app.ui.show_ribbon);

    // Hide Ribbon: Tool Sets can be restored
    app.run("ui.toggle.ribbon", json!({ "on": false })).unwrap();
    assert!(!app.ui.show_ribbon);

    app.run("ui.toggle.toolsets", json!({ "on": true })).unwrap();
    assert!(app.ui.show_toolsets);
    assert!(!app.ui.show_ribbon);

    // Restore both
    app.run("ui.toggle.ribbon", json!({ "on": true })).unwrap();
    assert!(app.ui.show_ribbon);
    assert!(app.ui.show_toolsets);
}

#[test]
fn operational_2d_boolean_and_text_tools_are_in_ribbon() {
    let home = ribbon::TABS.iter().find(|t| t.id == "Home").unwrap();
    let modify = home.panels.iter().find(|p| p.title == "Modify").unwrap();
    let found_union = modify.more.iter().any(|(cmd, _, _)| *cmd == "union");
    let found_subtract = modify.more.iter().any(|(cmd, _, _)| *cmd == "subtract");
    let found_intersect = modify.more.iter().any(|(cmd, _, _)| *cmd == "intersect");

    assert!(found_union, "union must be accessible from ribbon");
    assert!(found_subtract, "subtract must be accessible from ribbon");
    assert!(found_intersect, "intersect must be accessible from ribbon");

    let annot = home.panels.iter().find(|p| p.title == "Annotation").unwrap();
    let mut found_text = false;
    for item in annot.items {
        if let ribbon::RibbonItem::SplitButton { menu, .. } = item {
            for (cmd, _, _) in *menu {
                if *cmd == "text" {
                    found_text = true;
                }
            }
        }
    }
    assert!(found_text, "text must be accessible from Home Annotation panel");
}

#[test]
fn ribbon_command_activation_starts_engine_command() {
    let mut app = CadApp::new(Session::default(), Services::default());
    assert!(app.session.running.is_none());

    // Activating "line" from ribbon starts interactive line command
    menus::activate(&mut app, "line");
    assert_eq!(app.session.running.as_ref().map(|c| c.id.as_str()), Some("line"));

    // Activating "circle" replaces running command
    menus::activate(&mut app, "circle");
    assert_eq!(app.session.running.as_ref().map(|c| c.id.as_str()), Some("circle"));
}

#[test]
fn ribbon_prefs_roundtrip() {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.ui.show_ribbon = false;
    app.ui.ribbon_tab = "Annotate".into();
    app.ui.ribbon_collapsed = true;

    let json_str = app.prefs_json();

    let mut restored = CadApp::new(Session::default(), Services::default());
    restored.load_prefs(&json_str);

    assert!(!restored.ui.show_ribbon);
    assert_eq!(restored.ui.ribbon_tab, "Annotate");
    assert!(restored.ui.ribbon_collapsed);
}

#[test]
fn audit_all_ribbon_commands() {
    let mut app = CadApp::new(Session::default(), Services::default());
    // Put a dummy line in drawing and select it so commands requiring selection can be tested
    app.session.execute("line", &json!({ "points": [[0, 0], [10, 10]] })).unwrap();
    let handles: Vec<_> = app.session.doc().unwrap().model.iter().map(|e| e.handle).collect();
    app.session.state_mut().unwrap().selection = handles;

    let mut interactive = Vec::new();
    let mut ui_commands = Vec::new();
    let mut non_interactive_ok = Vec::new();
    let mut errored = Vec::new();

    let check_cmd = |app: &mut CadApp,
                     cmd: &str,
                     tab: &str,
                     panel: &str,
                     int_list: &mut Vec<String>,
                     ui_list: &mut Vec<String>,
                     ok_list: &mut Vec<String>,
                     err_list: &mut Vec<String>| {
        let before_log_len = app.session.log.len();
        let running_before = app.session.running.is_some();
        if running_before {
            app.session.cancel();
        }
        menus::activate(app, cmd);
        if menus::is_ui_command(cmd) {
            ui_list.push(format!("{tab} > {panel}: {cmd} (UI)"));
        } else if let Some(r) = app.session.running.as_ref() {
            int_list.push(format!("{tab} > {panel}: {cmd} -> running {}", r.id));
        } else {
            // Check if any error was logged
            let new_logs: Vec<_> = app.session.log.iter().skip(before_log_len).cloned().collect();
            let has_err = new_logs.iter().any(|m| m.starts_with("Error:") || m.contains("required") || m.contains("not available"));
            if has_err {
                err_list.push(format!("{tab} > {panel}: {cmd} -> error: {:?}", new_logs));
            } else {
                ok_list.push(format!("{tab} > {panel}: {cmd} -> ok: {:?}", new_logs));
            }
        }
    };

    for tab in ribbon::TABS {
        for panel in tab.panels {
            for item in panel.items {
                match item {
                    ribbon::RibbonItem::Large { id, .. } => {
                        check_cmd(&mut app, id, tab.id, panel.title, &mut interactive, &mut ui_commands, &mut non_interactive_ok, &mut errored);
                    }
                    ribbon::RibbonItem::SplitButton { id, menu, .. } => {
                        check_cmd(&mut app, id, tab.id, panel.title, &mut interactive, &mut ui_commands, &mut non_interactive_ok, &mut errored);
                        for (m_cmd, _, _) in *menu {
                            check_cmd(
                                &mut app,
                                m_cmd,
                                tab.id,
                                panel.title,
                                &mut interactive,
                                &mut ui_commands,
                                &mut non_interactive_ok,
                                &mut errored,
                            );
                        }
                    }
                    ribbon::RibbonItem::LargeDropdown { id, menu, .. } => {
                        check_cmd(&mut app, id, tab.id, panel.title, &mut interactive, &mut ui_commands, &mut non_interactive_ok, &mut errored);
                        for (m_cmd, _, _) in *menu {
                            check_cmd(
                                &mut app,
                                m_cmd,
                                tab.id,
                                panel.title,
                                &mut interactive,
                                &mut ui_commands,
                                &mut non_interactive_ok,
                                &mut errored,
                            );
                        }
                    }
                    ribbon::RibbonItem::MediumGrid { cols } => {
                        for col in *cols {
                            for mi in *col {
                                match mi {
                                    ribbon::MediumItem::Button { id, .. } => {
                                        check_cmd(
                                            &mut app,
                                            id,
                                            tab.id,
                                            panel.title,
                                            &mut interactive,
                                            &mut ui_commands,
                                            &mut non_interactive_ok,
                                            &mut errored,
                                        );
                                    }
                                    ribbon::MediumItem::Dropdown { id, menu, .. } => {
                                        check_cmd(
                                            &mut app,
                                            id,
                                            tab.id,
                                            panel.title,
                                            &mut interactive,
                                            &mut ui_commands,
                                            &mut non_interactive_ok,
                                            &mut errored,
                                        );
                                        for (m_cmd, _, _) in *menu {
                                            check_cmd(
                                                &mut app,
                                                m_cmd,
                                                tab.id,
                                                panel.title,
                                                &mut interactive,
                                                &mut ui_commands,
                                                &mut non_interactive_ok,
                                                &mut errored,
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                    ribbon::RibbonItem::CompactColumn(compact_items) => {
                        for ci in *compact_items {
                            match ci {
                                ribbon::CompactItem::Button { id, .. } | ribbon::CompactItem::IconOnly { id, .. } => {
                                    check_cmd(
                                        &mut app,
                                        id,
                                        tab.id,
                                        panel.title,
                                        &mut interactive,
                                        &mut ui_commands,
                                        &mut non_interactive_ok,
                                        &mut errored,
                                    );
                                }
                                ribbon::CompactItem::Dropdown { id, menu, .. } => {
                                    check_cmd(
                                        &mut app,
                                        id,
                                        tab.id,
                                        panel.title,
                                        &mut interactive,
                                        &mut ui_commands,
                                        &mut non_interactive_ok,
                                        &mut errored,
                                    );
                                    for (m_cmd, _, _) in *menu {
                                        check_cmd(
                                            &mut app,
                                            m_cmd,
                                            tab.id,
                                            panel.title,
                                            &mut interactive,
                                            &mut ui_commands,
                                            &mut non_interactive_ok,
                                            &mut errored,
                                        );
                                    }
                                }
                            }
                        }
                    }
                    ribbon::RibbonItem::IconColumn(icon_items) => {
                        for ii in *icon_items {
                            check_cmd(
                                &mut app,
                                ii.id,
                                tab.id,
                                panel.title,
                                &mut interactive,
                                &mut ui_commands,
                                &mut non_interactive_ok,
                                &mut errored,
                            );
                        }
                    }
                    ribbon::RibbonItem::LayersControl | ribbon::RibbonItem::PropertiesControl => {}
                }
            }
            for (cmd, _, _) in panel.more {
                check_cmd(&mut app, cmd, tab.id, panel.title, &mut interactive, &mut ui_commands, &mut non_interactive_ok, &mut errored);
            }
        }
    }

    println!("=== INTERACTIVE COMMANDS ({}) ===", interactive.len());
    for s in &interactive {
        println!("  {s}");
    }
    println!("=== UI COMMANDS ({}) ===", ui_commands.len());
    for s in &ui_commands {
        println!("  {s}");
    }
    println!("=== NON-INTERACTIVE OK ({}) ===", non_interactive_ok.len());
    for s in &non_interactive_ok {
        println!("  {s}");
    }
    println!("=== ERRORED / NOT INTERACTIVE ({}) ===", errored.len());
    for s in &errored {
        println!("  {s}");
    }
}

#[test]
fn layer_panel_associations_and_semantics() {
    let mut app = CadApp::new(Session::default(), Services::default());
    // Create layer "WALLS" and an object on it
    app.session.execute("layer.new", &json!({ "name": "WALLS" })).unwrap();
    app.session.execute("layer.current", &json!({ "name": "WALLS" })).unwrap();
    app.session.execute("line", &json!({ "points": [[0, 0], [10, 0]] })).unwrap();
    let line_handle = app.session.doc().unwrap().model.iter().next().unwrap().handle;
    app.session.execute("layer.current", &json!({ "name": "0" })).unwrap();

    // Verify laymch is NOT in the ribbon buttons (as it lacks interactive destination-object machine)
    let home_tab = ribbon::TABS.iter().find(|t| t.id == "Home").unwrap();
    let layers_panel = home_tab.panels.iter().find(|p| p.title == "Layers").unwrap();
    let has_layers_control = layers_panel.items.iter().any(|item| matches!(item, ribbon::RibbonItem::LayersControl));
    assert!(has_layers_control, "Layers panel must have dynamic LayersControl");

    // Select the object on WALLS and run laymcur with selection
    app.session.state_mut().unwrap().selection = vec![line_handle];
    assert_ne!(app.session.doc().unwrap().header.str("CLAYER", "0"), "WALLS");
    menus::activate(&mut app, "laymcur");
    assert_eq!(app.session.doc().unwrap().header.str("CLAYER", "0"), "WALLS");

    // Without selection, laymcur safely reports "select an object" (pre-selection required in CADCraft)
    app.session.state_mut().unwrap().selection.clear();
    let before_log = app.session.log.len();
    menus::activate(&mut app, "laymcur");
    let logs = &app.session.log[before_log..];
    assert!(logs.iter().any(|m| m.contains("select an object")), "laymcur requires pre-selected objects in CADCraft");

    // Test laylck starts interactive prompt machine
    app.session.cancel();
    menus::activate(&mut app, "laylck");
    assert_eq!(app.session.running.as_ref().map(|c| c.id.as_str()), Some("laylck"));
}

#[test]
fn leader_panel_associations() {
    let mut app = CadApp::new(Session::default(), Services::default());
    let annotate_tab = ribbon::TABS.iter().find(|t| t.id == "Annotate").unwrap();
    let leaders_panel = annotate_tab.panels.iter().find(|p| p.title == "Leaders").unwrap();
    let mut found_add = false;
    for item in leaders_panel.items {
        if let ribbon::RibbonItem::CompactColumn(cols) = item {
            for col in *cols {
                if let ribbon::CompactItem::Button { id, label, .. } = col
                    && *label == "Add Leader"
                {
                    assert_eq!(*id, "mleaderedit.add");
                    found_add = true;
                }
            }
        }
    }
    assert!(found_add);
    menus::activate(&mut app, "mleaderedit.add");
    assert_eq!(app.session.running.as_ref().map(|c| c.id.as_str()), Some("mleaderedit.add"));
}

#[test]
fn pending_incomplete_commands_are_not_in_ribbon() {
    let pending = ["laymch", "find", "lweight", "dimstyle", "3dorbit"];
    for tab in ribbon::TABS {
        for panel in tab.panels {
            for item in panel.items {
                match item {
                    ribbon::RibbonItem::Large { id, .. }
                    | ribbon::RibbonItem::SplitButton { id, .. }
                    | ribbon::RibbonItem::LargeDropdown { id, .. } => {
                        assert!(!pending.contains(id), "{id} must not be present in ribbon");
                    }
                    ribbon::RibbonItem::MediumGrid { cols } => {
                        for col in *cols {
                            for mi in *col {
                                match mi {
                                    ribbon::MediumItem::Button { id, .. } | ribbon::MediumItem::Dropdown { id, .. } => {
                                        assert!(!pending.contains(id), "{id} must not be present in ribbon");
                                    }
                                }
                            }
                        }
                    }
                    ribbon::RibbonItem::CompactColumn(cols) => {
                        for col in *cols {
                            match col {
                                ribbon::CompactItem::Button { id, .. }
                                | ribbon::CompactItem::Dropdown { id, .. }
                                | ribbon::CompactItem::IconOnly { id, .. } => {
                                    assert!(!pending.contains(id), "{id} must not be present in ribbon");
                                }
                            }
                        }
                    }
                    ribbon::RibbonItem::IconColumn(icons) => {
                        for ii in *icons {
                            assert!(!pending.contains(&ii.id), "{} must not be present in ribbon", ii.id);
                        }
                    }
                    ribbon::RibbonItem::LayersControl | ribbon::RibbonItem::PropertiesControl => {}
                }
            }
            for (cmd, _, _) in panel.more {
                assert!(!pending.contains(cmd), "{cmd} must not be present in ribbon more");
            }
        }
    }
}

#[test]
fn style_and_tablestyle_open_dialogs() {
    let mut app = CadApp::new(Session::default(), Services::default());
    menus::activate(&mut app, "style");
    assert_eq!(app.ui.dialog.as_deref(), Some("textstyle"));

    app.ui.dialog = None;
    menus::activate(&mut app, "tablestyle");
    assert_eq!(app.ui.dialog.as_deref(), Some("tablestyle"));
}

#[test]
fn layer_selector_reads_and_modifies_real_document_state() {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.ui.start_tab = false;
    let _ = app.run("new", json!({}));

    // Add a layer "ELECTRICAL"
    app.run("layer.new", json!({ "name": "ELECTRICAL", "color": 1 })).unwrap();
    let doc = app.session.doc().unwrap();
    assert!(doc.layer("ELECTRICAL").is_some());

    // Switch current layer
    app.run("layer.current", json!({ "name": "ELECTRICAL" })).unwrap();
    assert_eq!(app.session.doc().unwrap().header.str("CLAYER", "0"), "ELECTRICAL");

    // Create a line, select it, modify its layer
    let line_res = app.run("line", json!({ "points": [[0, 0], [10, 10]] })).unwrap();
    let handle = line_res["handles"][0].as_str().unwrap();
    app.session.set_selection(vec![cadcraft_doc::Handle::parse_hex(handle).unwrap()]);

    app.run("properties.set", json!({ "handles": [handle], "layer": "0" })).unwrap();
    let doc = app.session.doc().unwrap();
    let entity = doc.entity(cadcraft_doc::Handle::parse_hex(handle).unwrap()).unwrap();
    assert_eq!(entity.common.layer, "0");

    // Toggle layer property (freeze non-current layer)
    app.run("layer.current", json!({ "name": "0" })).unwrap();
    app.run("layer.set", json!({ "name": "ELECTRICAL", "frozen": true })).unwrap();
    let doc = app.session.doc().unwrap();
    assert!(doc.layer("ELECTRICAL").unwrap().frozen);
}

#[test]
fn properties_selector_reads_and_modifies_real_document_state() {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.ui.start_tab = false;
    let _ = app.run("new", json!({}));

    // Check defaults
    let doc = app.session.doc().unwrap();
    assert_eq!(doc.header.i64("CECOLOR", 256), 256); // ByLayer
    assert_eq!(doc.header.i64("CELWEIGHT", -1), -1); // ByLayer
    assert_eq!(doc.header.str("CELTYPE", "ByLayer"), "ByLayer");

    // Create an entity and modify color, lineweight, linetype on selection
    let line_res = app.run("line", json!({ "points": [[0, 0], [5, 5]] })).unwrap();
    let handle = line_res["handles"][0].as_str().unwrap();
    app.session.set_selection(vec![cadcraft_doc::Handle::parse_hex(handle).unwrap()]);

    app.run("properties.set", json!({ "handles": [handle], "color": 1 })).unwrap();
    let doc = app.session.doc().unwrap();
    let ent = doc.entity(cadcraft_doc::Handle::parse_hex(handle).unwrap()).unwrap();
    assert_eq!(ent.common.color.to_aci(), 1); // Red

    app.run("properties.set", json!({ "handles": [handle], "lineweight": 0.5 })).unwrap();
    let doc = app.session.doc().unwrap();
    let ent = doc.entity(cadcraft_doc::Handle::parse_hex(handle).unwrap()).unwrap();
    assert_eq!(ent.common.lineweight.to_dxf(), 50);

    // Modify default color when no selection
    app.session.set_selection(vec![]);
    app.run("color", json!({ "color": "1" })).unwrap();
    assert_eq!(app.session.doc().unwrap().header.i64("CECOLOR", 256), 1);
}

#[test]
fn ribbon_panels_layout_and_grouping_match_autocad_specification() {
    let home = ribbon::TABS.iter().find(|t| t.id == "Home").expect("Home tab exists");

    // 1. Draw panel: 4 large buttons, 1 compact column, panel more with secondary tools
    let draw = home.panels.iter().find(|p| p.title == "Draw").expect("Draw panel");
    assert_eq!(draw.items.len(), 5); // Line, Polyline, Circle (split), Arc (split), CompactColumn
    match &draw.items[0] {
        ribbon::RibbonItem::Large { id, label, .. } => {
            assert_eq!(*id, "line");
            assert_eq!(*label, "Line");
        }
        _ => panic!("Expected large button Line"),
    }
    match &draw.items[1] {
        ribbon::RibbonItem::Large { id, label, .. } => {
            assert_eq!(*id, "pline");
            assert_eq!(*label, "Polyline");
        }
        _ => panic!("Expected large button Polyline"),
    }
    match &draw.items[2] {
        ribbon::RibbonItem::SplitButton { id, label, menu, .. } => {
            assert_eq!(*id, "circle");
            assert_eq!(*label, "Circle");
            let cmds: Vec<&str> = menu.iter().map(|(cmd, _, _)| *cmd).collect();
            assert_eq!(cmds, vec!["circle", "circle.cd", "circle.2p", "circle.3p", "circle.ttr", "circle.ttt"]);
            assert!(!cmds.contains(&"donut"), "Donut must not be presented as a circle variant");
        }
        _ => panic!("Expected split button Circle"),
    }
    match &draw.items[3] {
        ribbon::RibbonItem::SplitButton { id, label, .. } => {
            assert_eq!(*id, "arc");
            assert_eq!(*label, "Arc");
        }
        _ => panic!("Expected split button Arc"),
    }
    match &draw.items[4] {
        ribbon::RibbonItem::CompactColumn(cols) => {
            assert_eq!(cols.len(), 3); // Rect/Polygon, Ellipse, Hatch/Gradient/Boundary
        }
        _ => panic!("Expected compact column for rect/ellipse/hatch"),
    }
    // Check secondary draw tools in panel more
    let draw_more_cmds: Vec<&str> = draw.more.iter().map(|(c, _, _)| *c).collect();
    assert!(draw_more_cmds.contains(&"spline"));
    assert!(draw_more_cmds.contains(&"xline"));
    assert!(draw_more_cmds.contains(&"ray"));
    assert!(draw_more_cmds.contains(&"revcloud"));
    assert!(draw_more_cmds.contains(&"region"));
    assert!(draw_more_cmds.contains(&"wipeout"));
    assert!(draw_more_cmds.contains(&"point.multiple"));
    assert!(draw_more_cmds.contains(&"donut"));

    // 2. Modify panel: 3x3 medium grid, 1 icon column of 3, panel more
    let modify = home.panels.iter().find(|p| p.title == "Modify").expect("Modify panel");
    assert_eq!(modify.items.len(), 2); // MediumGrid, IconColumn
    match &modify.items[0] {
        ribbon::RibbonItem::MediumGrid { cols } => {
            assert_eq!(cols.len(), 3);
            for col in *cols {
                assert_eq!(col.len(), 3);
            }
        }
        _ => panic!("Expected 3x3 MediumGrid for Modify"),
    }
    match &modify.items[1] {
        ribbon::RibbonItem::IconColumn(icons) => {
            assert_eq!(icons.len(), 3);
            let ids: Vec<&str> = icons.iter().map(|i| i.id).collect();
            assert_eq!(ids, vec!["erase", "explode", "offset"]);
        }
        _ => panic!("Expected IconColumn with erase, explode, offset"),
    }
    let mod_more_cmds: Vec<&str> = modify.more.iter().map(|(c, _, _)| *c).collect();
    assert!(mod_more_cmds.contains(&"break"));
    assert!(mod_more_cmds.contains(&"join"));
    assert!(mod_more_cmds.contains(&"lengthen"));
    assert!(mod_more_cmds.contains(&"matchprop"));

    // 3. Annotation panel: Split Text, Large Dim, CompactColumn (types, leader, table)
    let annotation = home.panels.iter().find(|p| p.title == "Annotation").expect("Annotation panel");
    assert_eq!(annotation.items.len(), 3);

    // 4. Layers panel: Large LayerProps, LayersControl
    let layers = home.panels.iter().find(|p| p.title == "Layers").expect("Layers panel");
    assert_eq!(layers.items.len(), 2);
    match &layers.items[0] {
        ribbon::RibbonItem::Large { id, .. } => assert_eq!(*id, "ui.dialog.layers"),
        _ => panic!("Expected Large button ui.dialog.layers"),
    }
    assert!(matches!(&layers.items[1], ribbon::RibbonItem::LayersControl));

    // 5. Properties panel: Large MatchProp, PropertiesControl
    let props = home.panels.iter().find(|p| p.title == "Properties").expect("Properties panel");
    assert_eq!(props.items.len(), 2);
    match &props.items[0] {
        ribbon::RibbonItem::Large { id, .. } => assert_eq!(*id, "matchprop"),
        _ => panic!("Expected Large button matchprop"),
    }
    assert!(matches!(&props.items[1], ribbon::RibbonItem::PropertiesControl));

    // 6. Block panel: Large Insert, CompactColumn (create, edit, attdef)
    let block = home.panels.iter().find(|p| p.title == "Block").expect("Block panel");
    assert_eq!(block.items.len(), 2);

    // 7. Utilities panel: SplitButton Measure, CompactColumn (area, qselect, list)
    let util = home.panels.iter().find(|p| p.title == "Utilities").expect("Utilities panel");
    assert_eq!(util.items.len(), 2);
}

#[test]
fn fillet_chamfer_split_button_options_and_command_routing() {
    let home = ribbon::TABS.iter().find(|t| t.id == "Home").expect("Home tab");
    let modify = home.panels.iter().find(|p| p.title == "Modify").expect("Modify panel");

    let mut found_fillet_split = false;
    if let ribbon::RibbonItem::MediumGrid { cols } = &modify.items[0] {
        for col in *cols {
            for item in *col {
                if let ribbon::MediumItem::Dropdown { id, label, menu, .. } = item
                    && *id == "fillet"
                {
                    found_fillet_split = true;
                    assert_eq!(*label, "Fillet");
                    cadcraft_ui_egui::i18n::with_language("es", || {
                        assert_eq!(cadcraft_ui_egui::i18n::t(label), "Empalme");
                    });

                    let menu_cmds: Vec<&str> = menu.iter().map(|(cmd, _, _)| *cmd).collect();
                    assert!(menu_cmds.contains(&"fillet"), "Dropdown must contain fillet option");
                    assert!(menu_cmds.contains(&"chamfer"), "Dropdown must contain chamfer option");

                    let fillet_entry = menu.iter().find(|(cmd, _, _)| *cmd == "fillet").unwrap();
                    assert_eq!(fillet_entry.1, "Fillet");
                    cadcraft_ui_egui::i18n::with_language("es", || {
                        assert_eq!(cadcraft_ui_egui::i18n::t(fillet_entry.1), "Empalme");
                    });

                    let chamfer_entry = menu.iter().find(|(cmd, _, _)| *cmd == "chamfer").unwrap();
                    assert_eq!(chamfer_entry.1, "Chamfer");
                    cadcraft_ui_egui::i18n::with_language("es", || {
                        assert_eq!(cadcraft_ui_egui::i18n::t(chamfer_entry.1), "Chaflán");
                    });
                }
            }
        }
    }
    assert!(found_fillet_split, "Modify panel must contain fillet/chamfer split button");

    // Both commands must be registered and operative in the engine
    let _app = CadApp::new(Session::default(), Services::default());
    assert!(cadcraft_engine::find_command("fillet").is_some());
    assert!(cadcraft_engine::find_command("chamfer").is_some());

    // Executing each command begins its respective interactive tool
    let mut app_fillet = CadApp::new(Session::default(), Services::default());
    app_fillet.start("fillet");
    assert_eq!(app_fillet.session.running.as_ref().map(|r| r.id.as_str()), Some("fillet"));

    let mut app_chamfer = CadApp::new(Session::default(), Services::default());
    app_chamfer.start("chamfer");
    assert_eq!(app_chamfer.session.running.as_ref().map(|r| r.id.as_str()), Some("chamfer"));
}

#[test]
fn spline_ribbon_access_interactive_creation_and_smooth_curve_geometry() {
    let home = ribbon::TABS.iter().find(|t| t.id == "Home").expect("Home tab");
    let draw = home.panels.iter().find(|p| p.title == "Draw").expect("Draw panel");

    // 1. Verify visible and easy access to SPLINE in Draw panel "more" flyout
    let spline_fit = draw.more.iter().find(|(cmd, _, _)| *cmd == "spline").expect("Spline Fit in Draw more");
    assert_eq!(spline_fit.1, "Spline Fit");
    cadcraft_ui_egui::i18n::with_language("es", || {
        assert_eq!(cadcraft_ui_egui::i18n::t(spline_fit.1), "Ajuste de spline");
    });

    let spline_cv = draw.more.iter().find(|(cmd, _, _)| *cmd == "spline.cv").expect("Spline CV in Draw more");
    assert_eq!(spline_cv.1, "Spline CV");
    cadcraft_ui_egui::i18n::with_language("es", || {
        assert_eq!(cadcraft_ui_egui::i18n::t(spline_cv.1), "Vértices de control de spline");
    });

    // 2. Interactive creation of true curved B-spline through successive points
    let mut app = CadApp::new(Session::default(), Services::default());
    app.start("spline");
    assert_eq!(app.session.running.as_ref().map(|r| r.id.as_str()), Some("spline"));

    // Enter point 1
    app.session.input(cadcraft_engine::Input::Point(cadcraft_geom::Vec2::new(0.0, 0.0))).unwrap();
    // Enter point 2
    app.session.input(cadcraft_engine::Input::Point(cadcraft_geom::Vec2::new(10.0, 20.0))).unwrap();

    // Verify curve preview during interactive drawing
    let preview = app.session.preview(cadcraft_geom::Vec2::new(20.0, 0.0));
    assert!(!preview.is_empty(), "Rubber-band preview must be active");
    assert!(matches!(&preview[0].kind, cadcraft_doc::EntityKind::Spline(_)), "Preview must be a true Spline curve");

    // Enter point 3
    app.session.input(cadcraft_engine::Input::Point(cadcraft_geom::Vec2::new(20.0, 0.0))).unwrap();
    // Enter point 4 and finish
    app.session.input(cadcraft_engine::Input::Point(cadcraft_geom::Vec2::new(30.0, -10.0))).unwrap();
    app.session.input(cadcraft_engine::Input::Enter).unwrap();
    assert!(app.session.running.is_none(), "Interactive spline command finished");

    // 3. Verify resulting entity is a genuine curved B-spline, not straight segments
    let doc = app.session.doc().unwrap();
    let entities: Vec<_> = doc.model.iter().collect();
    assert_eq!(entities.len(), 1, "One spline entity created in document");

    let cadcraft_doc::EntityKind::Spline(ref sp) = entities[0].kind else {
        panic!("Entity must be EntityKind::Spline");
    };

    assert_eq!(sp.degree, 3, "Degree is 3 (cubic spline with 4 fit points)");
    assert_eq!(sp.fit.len(), 4, "Stores 4 fit points");
    assert!(!sp.control.is_empty(), "Has computed control points");
    assert!(!sp.knots.is_empty(), "Has computed knot vector");

    // Test that the evaluated midpoint is genuinely curved (apex near y=20)
    let domain = sp.domain();
    let mid_t = (domain.0 + domain.1) / 2.0;
    let mid_pt = sp.eval(mid_t);
    assert!(mid_pt.y > 10.0, "Evaluated midpoint y={:.2} must be curved, not on baseline", mid_pt.y);

    // 4. Test selection by pick point near the curve
    let candidates = app.session.pick_candidates(cadcraft_geom::Vec2::new(10.0, 20.0));
    assert!(candidates.contains(&entities[0].handle), "Spline curve can be selected near its apex");
}

#[test]
fn user_defined_hatch_ribbon_access_and_dialog_integration() {
    let mut app = CadApp::new(Session::default(), Services::default());

    // 1. Check Hatch dropdown options in Ribbon Home tab -> Draw panel
    let home_tab = ribbon::TABS.iter().find(|t| t.id == "Home").expect("Home tab exists");
    let draw_panel = home_tab.panels.iter().find(|p| p.title == "Draw").expect("Draw panel exists");

    let mut found_hatch_dropdown = false;
    for item in draw_panel.items {
        if let ribbon::RibbonItem::CompactColumn(col) = item {
            for compact in *col {
                if let ribbon::CompactItem::Dropdown { id, menu, .. } = compact
                    && *id == "hatch"
                {
                    found_hatch_dropdown = true;
                    let menu_ids: Vec<&str> = menu.iter().map(|(id, _, _)| *id).collect();
                    assert!(menu_ids.contains(&"hatch"), "Hatch dropdown contains 'hatch'");
                    assert!(menu_ids.contains(&"hatch.user"), "Hatch dropdown contains 'hatch.user'");
                    assert!(menu_ids.contains(&"gradient"), "Hatch dropdown contains 'gradient'");
                    assert!(menu_ids.contains(&"boundary"), "Hatch dropdown contains 'boundary'");

                    let user_opt = menu.iter().find(|(id, _, _)| *id == "hatch.user").unwrap();
                    assert_eq!(user_opt.1, "User-Defined Hatch");
                }
            }
        }
    }
    assert!(found_hatch_dropdown, "Found Hatch dropdown in Draw panel");

    // 2. Check Spanish translations
    cadcraft_ui_egui::i18n::with_language("es", || {
        assert_eq!(cadcraft_ui_egui::i18n::t("User-Defined Hatch"), "Sombreado personalizado");
        assert_eq!(cadcraft_ui_egui::i18n::t("User-defined"), "Personalizado");
        assert_eq!(cadcraft_ui_egui::i18n::t("Predefined"), "Predefinido");
        assert_eq!(cadcraft_ui_egui::i18n::t("Spacing"), "Separación");
        assert_eq!(cadcraft_ui_egui::i18n::t("Double"), "Cruzado");
    });

    // 3. Opening "hatch.user" opens Hatch dialog in user-defined mode
    let res = app.run("hatch.user", serde_json::Value::Null);
    assert!(res.is_ok(), "hatch.user with null params opens dialog: {:?}", res);
    assert_eq!(app.ui.dialog, Some("hatch".into()));

    let dlg = app.hatch_dlg.as_mut().expect("Hatch dialog state is initialized");
    assert!(dlg.fill.user_defined, "Fill must be in user-defined mode");
    assert_eq!(dlg.fill.pattern, "_U");

    // 4. Test ok_call in create mode
    dlg.points.push([5.0, 5.0]);
    dlg.fill.spacing = 2.5;
    dlg.fill.angle = 45.0;
    dlg.fill.double = true;

    let call = cadcraft_ui_egui::hatchdlg::ok_call(dlg).expect("ok_call produces a command");
    assert_eq!(call.0, "hatch.user");
    let params = call.1;
    assert_eq!(params["pattern"], "_U");
    assert_eq!(params["userDefined"], true);
    assert_eq!(params["spacing"], 2.5);
    assert_eq!(params["angle"], 45.0);
    assert_eq!(params["double"], true);

    // 5. Execute on boundary and verify created Hatch entity
    app.session.execute("rectang", &serde_json::json!({ "p1": [0, 0], "p2": [10, 10] })).unwrap();
    let run_res = app.run(call.0, params);
    assert!(run_res.is_ok(), "Execution of user-defined hatch: {:?}", run_res);

    let doc = app.session.doc().unwrap();
    let hatches: Vec<_> = doc.model.iter().filter_map(|e| if let cadcraft_doc::EntityKind::Hatch(h) = &e.kind { Some(h) } else { None }).collect();
    assert_eq!(hatches.len(), 1);
    assert_eq!(hatches[0].pattern, "_U");
    assert_eq!(hatches[0].pattern_lines.len(), 2, "Cross hatch has 2 line families");
    assert!((hatches[0].pattern_lines[0].delta.1 - 2.5).abs() < 1e-4);
    assert!((hatches[0].pattern_lines[1].delta.1 - 2.5).abs() < 1e-4);
    assert!((hatches[0].angle - 45.0f64.to_radians()).abs() < 1e-4);
}

#[test]
fn circle_variants_ribbon_integration_and_interactive_flows() {
    let mut app = CadApp::new(Session::default(), Services::default());

    // 1. Verify all 6 circle variants are registered commands and can be activated
    let circle_cmds = ["circle", "circle.cd", "circle.2p", "circle.3p", "circle.ttr", "circle.ttt"];
    for cmd in circle_cmds {
        assert!(cadcraft_engine::find_command(cmd).is_some(), "Command {cmd} must be registered");
    }

    // 2. Test Center, Diameter (circle.cd) via interactive activation
    cadcraft_ui_egui::menus::activate(&mut app, "circle.cd");
    assert!(app.session.running.is_some(), "circle.cd must be active");
    assert!(app.cmdline("0,0").is_none());
    assert!(app.cmdline("10").is_none()); // Diameter 10 -> radius 5
    assert!(app.session.running.is_none(), "circle.cd must complete");
    let doc = app.session.doc().unwrap();
    let circles: Vec<_> = doc.model.iter().filter_map(|e| if let cadcraft_doc::EntityKind::Circle(c) = &e.kind { Some(c) } else { None }).collect();
    assert_eq!(circles.len(), 1);
    assert!((circles[0].radius - 5.0).abs() < 1e-9);
    assert!((circles[0].center.x - 0.0).abs() < 1e-9);

    // 3. Test 2-Point (circle.2p) via interactive activation
    cadcraft_ui_egui::menus::activate(&mut app, "circle.2p");
    assert!(app.session.running.is_some());
    assert!(app.cmdline("10,0").is_none());
    assert!(app.cmdline("20,0").is_none()); // Midpoint 15,0, radius 5
    assert!(app.session.running.is_none());
    let doc = app.session.doc().unwrap();
    let circles: Vec<_> = doc.model.iter().filter_map(|e| if let cadcraft_doc::EntityKind::Circle(c) = &e.kind { Some(c) } else { None }).collect();
    assert_eq!(circles.len(), 2);
    assert!((circles[1].radius - 5.0).abs() < 1e-9);
    assert!((circles[1].center.x - 15.0).abs() < 1e-9);

    // 4. Test 3-Point (circle.3p) via interactive activation
    cadcraft_ui_egui::menus::activate(&mut app, "circle.3p");
    assert!(app.session.running.is_some());
    assert!(app.cmdline("1,0 0,1 -1,0").is_none());
    assert!(app.session.running.is_none());
    let doc = app.session.doc().unwrap();
    let circles: Vec<_> = doc.model.iter().filter_map(|e| if let cadcraft_doc::EntityKind::Circle(c) = &e.kind { Some(c) } else { None }).collect();
    assert_eq!(circles.len(), 3);
    assert!((circles[2].radius - 1.0).abs() < 1e-9);
    assert!((circles[2].center.x - 0.0).abs() < 1e-9);

    // 5. Test 3-Point collinear failure (rejects collinear points)
    assert!(app.session.execute("circle.3p", &serde_json::json!({"p1": [0, 0], "p2": [5, 0], "p3": [10, 0]})).is_err());
    let doc = app.session.doc().unwrap();
    assert_eq!(
        doc.model.iter().filter(|e| matches!(&e.kind, cadcraft_doc::EntityKind::Circle(_))).count(),
        3,
        "No residual circle on collinear error"
    );

    // 6. Test cancellation during interactive circle creation (no residual geometry)
    cadcraft_ui_egui::menus::activate(&mut app, "circle.2p");
    assert!(app.session.running.is_some());
    assert!(app.cmdline("50,50").is_none());
    // Cancel command
    app.session.cancel();
    assert!(app.session.running.is_none());
    let doc = app.session.doc().unwrap();
    assert_eq!(doc.model.iter().filter(|e| matches!(&e.kind, cadcraft_doc::EntityKind::Circle(_))).count(), 3, "No residual circle on cancel");

    // 7. Test TTR (circle.ttr) with line-line geometry
    app.session.execute("line", &serde_json::json!({ "points": [[100, 0], [100, 100]] })).unwrap();
    app.session.execute("line", &serde_json::json!({ "points": [[0, 100], [100, 100]] })).unwrap();
    cadcraft_ui_egui::menus::activate(&mut app, "circle.ttr");
    assert!(app.session.running.is_some());
    assert!(app.cmdline("100,50 50,100 10").is_none()); // picks on lines, radius 10
    assert!(app.session.running.is_none());
    let doc = app.session.doc().unwrap();
    let circles: Vec<_> = doc.model.iter().filter_map(|e| if let cadcraft_doc::EntityKind::Circle(c) = &e.kind { Some(c) } else { None }).collect();
    assert_eq!(circles.len(), 4, "TTR circle added");
    assert!((circles[3].radius - 10.0).abs() < 1e-6);

    // 8. Test Donut is accessible via command and not broken
    cadcraft_ui_egui::menus::activate(&mut app, "donut");
    assert!(app.session.running.is_some());
    app.session.cancel();
}
