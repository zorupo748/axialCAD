//! Comprehensive tests verifying elimination of the right sidebar (Layers & Properties),
//! real canvas rectangle measurements across window sizes, pointer click reception
//! in the freed area, on-demand floating palettes, legacy profile migration, and visual exports.

use cadcraft_engine::Session;
use cadcraft_ui_egui::{
    CadApp, Services, UiState,
    docking::{Panel, Workspace},
};
use craft_ui::docking::{Layout, Node, StackEntry};
use craft_ui::layout::{SplitAxis, SplitSize};
use egui::{Event, Modifiers, PointerButton, pos2, vec2};
use egui_kittest::Harness;
use serde_json::json;

fn harness(state: Option<UiState>, width: f32, scale: f32) -> Harness<'static, CadApp> {
    let mut app = CadApp::new(Session::default(), Services::default());
    app.run("rectang", json!({"p1":[30,30],"p2":[170,110]})).unwrap();
    app.run("circle", json!({"center":[100,70],"radius":26})).unwrap();
    app.run("line", json!({"points":[[30,30],[170,110]]})).unwrap();
    app.run("view.set", json!({"center":[100,70],"height":220})).unwrap();
    if let Some(state) = state {
        app.ui = state;
    } else {
        app.ui.show_toolsets = true;
    }
    app.ui.in_window_menu = false;
    app.integrated_titlebar = true;
    let mut harness = Harness::builder().with_size(vec2(width, 760.0)).with_pixels_per_point(scale).build_ui_state(
        |ui, app: &mut CadApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        app,
    );
    harness.run_steps(5);
    harness
}

#[test]
fn default_workspace_and_fresh_app_have_no_right_sidebar() {
    let workspace = Workspace::default();
    assert!(!workspace.layout.contains(&Panel::Layers));
    assert!(!workspace.layout.contains(&Panel::Properties));
    assert!(workspace.layout.contains(&Panel::Canvas));
    assert!(workspace.layout.contains(&Panel::ToolSets));
    assert!(workspace.hidden.iter().any(|(p, _)| *p == Panel::Layers));
    assert!(workspace.hidden.iter().any(|(p, _)| *p == Panel::Properties));
    assert!(workspace.validate().is_ok());

    let mut nodes: Vec<_> = workspace.layout.root.iter().collect();
    while let Some(node) = nodes.pop() {
        if let Node::Split { size, .. } = node {
            assert!(!matches!(size, SplitSize::FixedSecond(s) if *s >= 200.0), "Default layout must not reserve a fixed second right column");
        }
    }

    let app = CadApp::new(Session::default(), Services::default());
    assert!(!app.ui.docking.layout.contains(&Panel::Layers));
    assert!(!app.ui.docking.layout.contains(&Panel::Properties));
    assert!(!app.ui.show_palettes);
}

#[test]
fn real_canvas_rectangle_extends_to_right_edge_at_normal_and_narrow_sizes() {
    // 1. Normal window size (1280.0 x 760.0)
    let mut h_normal = harness(None, 1280.0, 1.0);
    let canvas_rect_normal = h_normal.state().canvas.rect.expect("Canvas must have rendered rect");
    assert!(
        canvas_rect_normal.max.x >= 1270.0,
        "Canvas max.x ({}) must extend to window right margin (1272.0 of 1280.0), with no reserved right sidebar",
        canvas_rect_normal.max.x
    );
    // Canvas left boundary starts after ToolSets (220px)
    assert!(
        canvas_rect_normal.min.x >= 219.0 && canvas_rect_normal.min.x <= 235.0,
        "Canvas min.x ({}) must begin right after ToolSets",
        canvas_rect_normal.min.x
    );

    // Collapsing ToolSets expands Canvas to the full window width (~22px collapsed bar)
    h_normal.state_mut().ui.show_toolsets = false;
    h_normal.run_steps(4);
    let canvas_full = h_normal.state().canvas.rect.expect("Canvas rect");
    assert!(canvas_full.min.x <= 32.0, "Canvas min.x must be at collapsed strip");
    assert!(canvas_full.max.x >= 1270.0, "Canvas max.x must remain at right margin");

    // 2. Narrow window size (900.0 x 760.0)
    let h_narrow = harness(None, 900.0, 1.0);
    let canvas_rect_narrow = h_narrow.state().canvas.rect.expect("Canvas must have rendered rect in narrow window");
    assert!(canvas_rect_narrow.max.x >= 890.0, "Canvas max.x ({}) must extend to narrow window edge (892.0 of 900.0)", canvas_rect_narrow.max.x);
}

#[test]
fn mouse_clicks_in_former_sidebar_zone_reach_canvas() {
    let mut h = harness(None, 1280.0, 1.0);
    h.state_mut().start("line");
    h.run_steps(2);

    // Click in the former sidebar zone (x: 1150.0, y: 400.0)
    let click_pos = pos2(1150.0, 400.0);
    h.event(Event::PointerMoved(click_pos));
    h.run_steps(1);
    h.event(Event::PointerButton { pos: click_pos, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    h.event(Event::PointerButton { pos: click_pos, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);

    // The Line command should now have its first base point confirmed from clicking in that region
    let running = h.state().session.running.as_ref().expect("LINE command should still be running");
    assert_eq!(running.id, "line");
    // Verify line prompt advanced to next point (meaning the first point was accepted)
    assert!(
        h.state().session.prompt_text().to_lowercase().contains("próximo")
            || h.state().session.prompt_text().to_lowercase().contains("next")
            || h.state().session.prompt_text().to_lowercase().contains("specify"),
        "Canvas must receive the click in the former sidebar area and advance prompt, but prompt was: {}",
        h.state().session.prompt_text()
    );
}

#[test]
fn legacy_user_profile_with_docked_sidebar_migrates_cleanly() {
    // Construct exact representation of user's ~/.config/cadcraft/workspace.json layout:
    // Split(Horizontal, FixedSecond(220.0), Canvas, Stack([properties (open: false)]))
    let mut legacy_workspace = Workspace {
        layout: Layout {
            root: Some(Node::Split {
                axis: SplitAxis::Horizontal,
                size: SplitSize::FixedFirst(220.0),
                first: Box::new(Node::Tabs { active: 0, panels: vec![Panel::ToolSets] }),
                second: Box::new(Node::Split {
                    axis: SplitAxis::Horizontal,
                    size: SplitSize::FixedSecond(220.0),
                    first: Box::new(Node::Tabs { active: 0, panels: vec![Panel::Canvas] }),
                    second: Box::new(Node::Stack { entries: vec![StackEntry { panel: Panel::Properties, open: false, height: None }] }),
                }),
            }),
            floating: Vec::new(),
        },
        hidden: Vec::new(),
    };

    // Before migration, the layout contains Properties and a FixedSecond split reserving 220px
    assert!(legacy_workspace.layout.contains(&Panel::Properties));

    // Run migration
    legacy_workspace.migrate_remove_sidebar();

    // After migration, Properties is removed from docked layout and moved to hidden as floating
    assert!(!legacy_workspace.layout.contains(&Panel::Properties));
    assert!(legacy_workspace.layout.contains(&Panel::Canvas));
    assert!(legacy_workspace.layout.contains(&Panel::ToolSets));
    assert!(legacy_workspace.hidden.iter().any(|(p, loc)| *p == Panel::Properties && loc.floating.is_some()));
    assert!(legacy_workspace.validate().is_ok());

    // Verify when loaded into app, canvas occupies full width
    let mut state = UiState::default();
    state.docking = legacy_workspace;
    state.show_palettes = false;
    let h = harness(Some(state), 1280.0, 1.0);
    let rect = h.state().canvas.rect.expect("Canvas rect");
    assert!(rect.max.x >= 1270.0, "Migrated legacy workspace must reclaim full width: canvas max.x is {}, expected >= 1270.0", rect.max.x);
}

#[test]
fn properties_and_layers_on_demand_open_as_floating_windows_and_close_leaves_no_gap() {
    let mut h = harness(None, 1280.0, 1.0);
    let canvas_initial = h.state().canvas.rect.expect("Canvas initial rect");
    assert!(canvas_initial.max.x >= 1270.0);

    // Open Properties on demand
    h.state_mut().run("ui.panel.properties", json!({})).unwrap();
    h.run_steps(3);
    assert!(h.state().ui.docking.layout.contains(&Panel::Properties));
    assert!(
        h.state().ui.docking.layout.floating.iter().any(|g| g.panels.contains(&Panel::Properties)),
        "Properties opened on demand must be in floating group"
    );
    // Canvas width is NOT squished by a column
    let canvas_with_properties = h.state().canvas.rect.expect("Canvas rect with floating properties");
    assert!(
        canvas_with_properties.max.x >= 1270.0,
        "Opening floating Properties must not reduce canvas width: max.x is {}",
        canvas_with_properties.max.x
    );

    // Close Properties
    h.state_mut().run("ui.dock", json!({"operation": "close", "panel": "properties"})).unwrap();
    h.run_steps(3);
    assert!(!h.state().ui.docking.layout.contains(&Panel::Properties));
    let canvas_after_close = h.state().canvas.rect.expect("Canvas rect after close");
    assert_eq!(canvas_after_close.max.x, canvas_initial.max.x, "Canvas max.x must remain at right edge after closing Properties");

    // Open Layers on demand
    h.state_mut().run("ui.panel.layers", json!({})).unwrap();
    h.run_steps(3);
    assert!(h.state().ui.docking.layout.contains(&Panel::Layers));
    assert!(
        h.state().ui.docking.layout.floating.iter().any(|g| g.panels.contains(&Panel::Layers)),
        "Layers opened on demand must be in floating group"
    );
    let canvas_with_layers = h.state().canvas.rect.expect("Canvas rect with floating layers");
    assert!(canvas_with_layers.max.x >= 1270.0);

    // Close Layers
    h.state_mut().run("ui.dock", json!({"operation": "close", "panel": "layers"})).unwrap();
    h.run_steps(3);
    assert!(!h.state().ui.docking.layout.contains(&Panel::Layers));
    let canvas_final = h.state().canvas.rect.expect("Canvas rect final");
    assert_eq!(canvas_final.max.x, canvas_initial.max.x);
}

#[test]
fn persistence_and_restart_preserve_clean_layout() {
    let app = CadApp::new(Session::default(), Services::default());
    assert!(!app.ui.docking.layout.contains(&Panel::Properties));
    assert!(!app.ui.docking.layout.contains(&Panel::Layers));

    // Save and reload state
    let serialized = serde_json::to_string(&app.ui).unwrap();
    let reloaded: UiState = serde_json::from_str(&serialized).unwrap();
    assert!(reloaded.docking.validate().is_ok());
    assert!(!reloaded.docking.layout.contains(&Panel::Properties));
    assert!(!reloaded.docking.layout.contains(&Panel::Layers));
    assert!(reloaded.docking.layout.contains(&Panel::Canvas));

    let h = harness(Some(reloaded), 1280.0, 1.0);
    let rect = h.state().canvas.rect.expect("Canvas rect");
    assert!(rect.max.x >= 1270.0, "Canvas after restart must maintain full width");
}

#[test]
fn ribbon_layer_controls_and_manager_dialog_continue_to_work() {
    let mut app = CadApp::new(Session::default(), Services::default());

    // Layer Properties Manager dialog (button in ribbon)
    app.run("ui.dialog.layers", json!({})).unwrap();
    assert_eq!(app.ui.dialog.as_deref(), Some("layers"));

    // Close dialog
    app.run("ui.dialog.close", json!({})).unwrap();
    assert_eq!(app.ui.dialog, None);

    // Layer query and creation still works
    let res = app.run("layer", json!({})).unwrap();
    assert!(res.is_object() || res.is_array());
}

#[test]
fn render_visual_comparison_screenshots() {
    let dir = "/tmp/cadcraft-shots";
    let _ = std::fs::create_dir_all(dir);

    // 1. "Before": With old docked 300px sidebar
    let mut legacy_state = UiState::default();
    legacy_state.docking = Workspace {
        layout: Layout {
            root: Some(Node::Split {
                axis: SplitAxis::Horizontal,
                size: SplitSize::FixedFirst(220.0),
                first: Box::new(Node::Tabs { active: 0, panels: vec![Panel::ToolSets] }),
                second: Box::new(Node::Split {
                    axis: SplitAxis::Horizontal,
                    size: SplitSize::FixedSecond(300.0),
                    first: Box::new(Node::Tabs { active: 0, panels: vec![Panel::Canvas] }),
                    second: Box::new(Node::Stack {
                        entries: vec![
                            StackEntry { panel: Panel::Layers, open: true, height: Some(135.0) },
                            StackEntry { panel: Panel::Properties, open: true, height: None },
                        ],
                    }),
                }),
            }),
            floating: Vec::new(),
        },
        hidden: Vec::new(),
    };
    legacy_state.show_palettes = true;
    let mut h_before = harness(Some(legacy_state), 1280.0, 1.0);
    if let Ok(img) = h_before.render() {
        let _ = img.save(format!("{dir}/comparison-before-with-sidebar.png"));
    }

    // 2. "After": Clean default layout without sidebar, canvas taking full remaining width
    let mut h_after = harness(None, 1280.0, 1.0);
    if let Ok(img) = h_after.render() {
        let _ = img.save(format!("{dir}/comparison-after-no-sidebar.png"));
    }

    // 3. "On-demand floating": Properties opened as floating window
    let mut h_floating = harness(None, 1280.0, 1.0);
    h_floating.state_mut().run("ui.panel.properties", json!({})).unwrap();
    h_floating.run_steps(4);
    if let Ok(img) = h_floating.render() {
        let _ = img.save(format!("{dir}/comparison-properties-floating.png"));
    }
}
