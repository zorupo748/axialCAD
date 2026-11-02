//! Window chrome: title and tool bar, file tabs, status bar and the Start page.

use egui::{Rect, Sense, Stroke, pos2, vec2};
use serde_json::json;

use crate::CadApp;
use crate::icons::{self, Icon};
use crate::theme::Tokens;

pub mod annoscale;

pub const QUICK_ACCESS: &[(Icon, &str, &str, Option<&str>)] = &[
    (Icon::New, "new", "New Drawing", Some("Cmd+N")),
    (Icon::Open, "open", "Open", Some("Cmd+O")),
    (Icon::Save, "qsave", "Save", Some("Cmd+S")),
    (Icon::Plot, "plot", "Print", Some("Cmd+P")),
    (Icon::Undo, "u", "Undo", Some("Cmd+Z")),
    (Icon::Redo, "redo", "Redo", Some("Cmd+Shift+Z")),
];

/// Render the compact Quick Access Toolbar buttons. Returns the clicked command and the total width consumed.
pub fn render_quick_access(ui: &mut egui::Ui, rect: Rect, mut start_x: f32, btn_size: f32) -> (Option<&'static str>, f32) {
    let t = Tokens::get();
    let mac = ui.ctx().os().is_mac();
    let mut clicked = None;
    let initial_x = start_x;

    for (i, (icon, cmd, label, shortcut)) in QUICK_ACCESS.iter().enumerate() {
        if i == 4 {
            // Subtle vertical separator between Plot and Undo
            start_x += 2.0;
            ui.painter().vline(
                start_x,
                (rect.center().y - btn_size / 2.0 + 3.0)..=(rect.center().y + btn_size / 2.0 - 3.0),
                Stroke::new(1.0, t.border),
            );
            start_x += 5.0;
        }

        let br = Rect::from_min_size(pos2(start_x, rect.center().y - btn_size / 2.0), vec2(btn_size, btn_size));
        let resp = ui.interact(br, ui.id().with(("qat", *cmd)), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(br, 3.0, t.control_hover.gamma_multiply(0.6));
        }
        icons::paint(ui.painter(), br.shrink(3.0), *icon, false);
        let tip = match shortcut {
            Some(sc) => format!("{} ({})", crate::i18n::t(label), crate::menus::shortcut_label(sc, mac)),
            None => crate::i18n::t(label).to_string(),
        };
        icons::describe_control(ui, &resp, &tip, None, false);
        if resp.on_hover_text(tip).clicked() {
            clicked = Some(*cmd);
        }
        start_x += btn_size + 2.0;
    }

    (clicked, start_x - initial_x)
}

/// Height of the macOS integrated title row (#161). AppKit draws the traffic lights itself,
/// centred in the standard 28 pt title-bar band at the top of the window; moving them would take
/// AppKit calls (`unsafe`, which the workspace forbids). So the title row is that band, and what
/// it holds is centred on the same line as the lights.
const MAC_TITLE_BAR: f32 = 28.0;
/// The traffic lights as AppKit lays them out: button diameter, centre-to-centre pitch and the
/// first button's left edge (8–10 pt depending on the macOS version; the larger value keeps the
/// gap after them from shrinking).
const MAC_LIGHT: f32 = 12.0;
const MAC_LIGHT_PITCH: f32 = 20.0;
const MAC_LIGHTS_LEFT: f32 = 10.0;
/// Where title-row content may start to the right of the traffic lights: the end of the three
/// lights plus the same gap the lights keep from the top of the window.
const MAC_CONTENT_LEFT: f32 = MAC_LIGHTS_LEFT + 2.0 * MAC_LIGHT_PITCH + MAC_LIGHT + (MAC_TITLE_BAR - MAC_LIGHT) / 2.0;

/// Left edge of the window title in a title row starting at `left`, `width` wide: centred, but
/// never under the traffic lights (a long drawing name in a narrow window slides right instead).
pub fn title_left(left: f32, width: f32, text_w: f32) -> f32 {
    (left + (width - text_w) / 2.0).max(left + MAC_CONTENT_LEFT)
}

pub fn title_and_toolbar(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    let mac_title = app.integrated_titlebar;
    let h = if mac_title { MAC_TITLE_BAR } else { 28.0 };

    egui::Panel::top("cc_title_toolbar").exact_size(h).frame(egui::Frame::NONE.fill(t.chrome_dark)).show(ui, |ui| {
        let r = ui.max_rect();
        let mut clicked: Option<String> = None;

        let start_x = if mac_title { r.left() + MAC_CONTENT_LEFT + 4.0 } else { r.left() + 8.0 };

        // 1. Quick Access Toolbar buttons (New, Open, Save, Print, Undo, Redo)
        let mut qat_w = 0.0;
        if app.ui.show_toolbar {
            let (cmd, w) = render_quick_access(ui, r, start_x, 20.0);
            if let Some(c) = cmd {
                clicked = Some(c.to_string());
            }
            qat_w = w;

            // 2. Dropdown button (Customize Quick Access Toolbar / Classic menus)
            let drop_x = start_x + qat_w;
            let drop_r = Rect::from_min_size(pos2(drop_x, r.center().y - 10.0), vec2(14.0, 20.0));
            let drop_resp = ui.interact(drop_r, ui.id().with("qat_more"), Sense::click());
            if drop_resp.hovered() {
                ui.painter().rect_filled(drop_r, 3.0, t.control_hover.gamma_multiply(0.6));
            }
            icons::paint(ui.painter(), Rect::from_center_size(drop_r.center(), vec2(7.0, 7.0)), Icon::ChevronDown, false);
            let drop_tip = crate::tl!("Customize Quick Access Toolbar");
            icons::describe_control(ui, &drop_resp, drop_tip, None, false);
            let drop_resp = drop_resp.on_hover_text(drop_tip);

            egui::Popup::from_toggle_button_response(&drop_resp).close_behavior(egui::PopupCloseBehavior::CloseOnClick).show(|ui| {
                let label = if app.ui.in_window_menu { crate::tl!("Hide Menu Bar") } else { crate::tl!("Show Menu Bar") };
                if ui.button(label).clicked() {
                    app.ui.in_window_menu = !app.ui.in_window_menu;
                    ui.close();
                }
                ui.separator();
                for (name, entries) in crate::menus::tree(app) {
                    ui.menu_button(crate::i18n::menu(&name), |ui| {
                        for e in &entries {
                            crate::menus::entry_ui(ui, e, &mut clicked);
                        }
                    });
                }
            });
            qat_w += 16.0;
        }

        // 3. Centered Title: CADCraft - [DocumentName][*]
        let (title_text, dirty) = match app.session.state() {
            Ok(st) if !app.ui.start_tab => {
                let name = if st.title.contains('.') { st.title.clone() } else { format!("{}.dwg", st.title) };
                (format!("CADCraft - {name}"), st.is_dirty())
            }
            _ => (format!("CADCraft - {}", crate::tl!("Start")), false),
        };
        let mark = if dirty { "*" } else { "" };
        let full_title = format!("{title_text}{mark}");

        let min_title_x = if app.ui.show_toolbar { start_x + qat_w + 12.0 } else { start_x };
        let max_title_x = r.right() - 12.0;
        let font_id = egui::FontId::proportional(12.5);
        let galley = ui.painter().layout_no_wrap(full_title.clone(), font_id.clone(), t.text_dim);
        let text_w = galley.size().x;

        let x = (r.left() + (r.width() - text_w) / 2.0).max(min_title_x);
        let available_w = (max_title_x - x).max(0.0);
        let (galley, cut) = elided(ui.painter(), &full_title, font_id, t.text_dim, available_w);
        let title_pos = pos2(x, r.top() + (h - galley.size().y) / 2.0);
        ui.painter().galley(title_pos, galley, t.text_dim);
        if cut {
            let title_rect = Rect::from_min_size(title_pos, vec2(available_w, h));
            ui.interact(title_rect, ui.id().with("title_hover"), Sense::hover()).on_hover_text(&full_title);
        }

        // Allow dragging the window outside QAT buttons
        let drag_rect = Rect::from_min_max(pos2(min_title_x, r.top()), pos2(max_title_x, r.bottom()));
        let resp = ui.interact(drag_rect, ui.id().with("titledrag"), Sense::CLICK | Sense::DRAG);
        if resp.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if resp.double_clicked() {
            let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
        }

        if let Some(c) = clicked {
            if c == "new" {
                app.ui.start_tab = false;
            }
            crate::menus::activate(app, &c);
        }
    });
}

/// File tabs: switcher, "+", Start, open drawings.
pub fn file_tabs(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    egui::Panel::top("cc_file_tabs").exact_size(26.0).frame(egui::Frame::NONE.fill(t.chrome_dark)).show(ui, |ui| {
        let r = ui.max_rect();
        let mut x = r.left() + 4.0;
        let ib = Rect::from_min_size(pos2(x, r.top() + 4.0), vec2(18.0, 18.0));
        icons::paint(ui.painter(), ib.shrink(1.0), Icon::Switcher, false);
        x += 24.0;
        let plus = Rect::from_min_size(pos2(x, r.top() + 4.0), vec2(18.0, 18.0));
        let presp = ui.interact(plus, ui.id().with("tab+"), Sense::click());
        icons::paint(ui.painter(), plus.shrink(2.0), Icon::Plus, false);
        icons::describe_control(ui, &presp, "New Drawing", None, false);
        if presp.on_hover_text(crate::tl!("New Drawing")).clicked() {
            app.start("new");
            app.ui.start_tab = false;
        }
        x += 24.0;
        // `label` is the drawing's name; `dirty` adds the unsaved-changes mark, kept visible when a
        // long name is cut short.
        let mut tab =
            |ui: &mut egui::Ui, label: &str, dirty: bool, active: bool, closable: bool, w: f32, id: egui::Id| -> (bool, bool, egui::Response) {
                let tr = Rect::from_min_size(pos2(x, r.top()), vec2(w, r.height()));
                let resp = ui.interact(tr, id, Sense::click());
                // The close button takes the tab's hover, so `resp.hovered()` would make the tab and
                // its X flicker while the pointer is on the X.
                let hovered = ui.rect_contains_pointer(tr);
                ui.painter().rect_filled(
                    tr,
                    0.0,
                    if active {
                        t.tab_active
                    } else if hovered {
                        t.chrome
                    } else {
                        t.chrome_dark
                    },
                );
                // The name ends in "…" when it doesn't fit before the close button (#513).
                let color = if active { t.text } else { t.text_dim };
                let mark = if dirty { "*" } else { "" };
                let text_w = (w - 8.0 - if closable { 24.0 } else { 8.0 }).max(0.0);
                let mark_w = ui.painter().layout_no_wrap(mark.to_string(), crate::theme::body(), color).size().x;
                let (galley, cut) = elided(ui.painter(), label, crate::theme::body(), color, text_w - mark_w);
                let pos = pos2(tr.left() + 8.0, tr.center().y - galley.size().y / 2.0);
                let name_w = galley.size().x;
                ui.painter().galley(pos, galley, color);
                if dirty {
                    ui.painter().text(pos2(pos.x + name_w, tr.center().y), egui::Align2::LEFT_CENTER, mark, crate::theme::body(), color);
                }
                let resp = if cut { resp.on_hover_text(format!("{label}{mark}")) } else { resp };
                let mut close = false;
                if closable && (active || hovered) {
                    let cr = Rect::from_center_size(pos2(tr.right() - 11.0, tr.center().y), vec2(12.0, 12.0));
                    let cresp = ui.interact(cr, id.with("x"), Sense::click());
                    icons::paint(ui.painter(), cr, Icon::Close, false);
                    icons::describe_control(ui, &cresp, &format!("Close {label}"), None, false);
                    close = cresp.clicked();
                }
                icons::describe_control(ui, &resp, label, Some(active), true);
                ui.painter().vline(tr.right(), tr.y_range(), Stroke::new(1.0, t.border));
                x += w;
                (resp.clicked(), close, resp)
            };
        let (c, _, _) = tab(ui, crate::tl!("Start"), false, app.ui.start_tab || app.session.docs.is_empty(), false, 110.0, ui.id().with("tab_start"));
        if c {
            app.ui.start_tab = true;
        }
        let mut switch_to = None;
        let mut close = None;
        let titles: Vec<(usize, String, bool, bool)> =
            app.session.docs.iter().enumerate().map(|(i, d)| (i, d.title.clone(), d.is_dirty(), i == app.session.active)).collect();
        for (i, title, dirty, active) in titles {
            let (c, x, resp) = tab(ui, &title, dirty, active && !app.ui.start_tab, true, 170.0, ui.id().with(("tab", i)));
            crate::shortcut_menus::attach(app, &resp, |app| crate::shortcut_menus::drawing_tab_entries(app, i));
            if c {
                switch_to = Some(i);
            }
            if x {
                close = Some(i);
            }
        }
        if let Some(i) = switch_to {
            let _ = app.run("document.switch", json!({ "index": i }));
            app.ui.start_tab = false;
        }
        if let Some(i) = close {
            app.request_close(Some(i));
        }
    });
}

/// `text` laid out on one line no wider than `max_width`, ending in "…" when it had to be cut;
/// and whether it was.
fn elided(p: &egui::Painter, text: &str, font: egui::FontId, color: egui::Color32, max_width: f32) -> (std::sync::Arc<egui::Galley>, bool) {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping { max_width: max_width.max(0.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    let galley = p.layout_job(job);
    let cut = galley.elided;
    (galley, cut)
}

/// Widest layout tab name before it is cut with "…".
const LAYOUT_TAB_TEXT: f32 = 150.0;
/// Room the layout tabs leave for the coordinates and messages.
const STATUS_READOUT_MIN: f32 = 150.0;

/// Which of the tabs with these `widths` to show in `room`: all of them when they fit, else a run
/// that holds the `active` one (shown even when it alone is too wide), starting as early as it can.
fn visible_tabs(widths: &[f32], active: Option<usize>, room: f32) -> std::ops::Range<usize> {
    let fits = |range: std::ops::Range<usize>| widths.get(range).is_some_and(|w| w.iter().sum::<f32>() <= room);
    let mut end = 0;
    while end < widths.len() && fits(0..end + 1) {
        end += 1;
    }
    let Some(active) = active.filter(|a| *a >= end && *a < widths.len()) else { return 0..end };
    let mut start = active;
    while start > 0 && fits(start - 1..active + 1) {
        start -= 1;
    }
    start..active + 1
}

/// Status bar: layout tabs on the left, coordinates and drafting toggles on the right.
fn format_cursor_coordinates(cursor: cadcraft_geom::Vec3, plane: cadcraft_geom::DrawingPlane, lu: i64, lp: i64) -> String {
    if plane == cadcraft_geom::DrawingPlane::XY && cursor.z.abs() < 1e-9 {
        format!(
            "{}, {}, {}",
            cadcraft_engine::units::format_distance(cursor.x, lu, lp),
            cadcraft_engine::units::format_distance(cursor.y, lu, lp),
            cadcraft_engine::units::format_distance(0.0, lu, lp)
        )
    } else {
        format!(
            "[WCS] {}, {}, {}",
            cadcraft_engine::units::format_distance(cursor.x, lu, lp),
            cadcraft_engine::units::format_distance(cursor.y, lu, lp),
            cadcraft_engine::units::format_distance(cursor.z, lu, lp)
        )
    }
}

pub fn status_bar(app: &mut CadApp, ui: &mut egui::Ui) {
    let t = Tokens::get();
    egui::Panel::bottom("cc_status").exact_size(26.0).frame(egui::Frame::NONE.fill(t.chrome)).show(ui, |ui| {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        let mut x = r.left() + 8.0;
        // Clear the status message on time even when no input arrives (the app only repaints on input).
        if let Some((_, at)) = &app.status
            && crate::now_ms() - at < 5000.0
        {
            let left = std::time::Duration::try_from_secs_f64((5000.0 - (crate::now_ms() - at)) / 1000.0).unwrap_or_default();
            ui.ctx().request_repaint_after(left);
        }
        let mut list_button = None;
        for (icon, tip) in [(Icon::Plus, "New layout"), (Icon::Menu, "Layout list")] {
            let br = Rect::from_min_size(pos2(x, r.top() + 4.0), vec2(18.0, 18.0));
            let resp = ui.interact(br, ui.id().with(("sb", tip)), Sense::click());
            icons::paint(&p, br.shrink(2.0), icon, false);
            icons::describe_control(ui, &resp, tip, None, false);
            let resp = resp.on_hover_text(crate::i18n::t(tip));
            if icon == Icon::Plus {
                if resp.clicked() {
                    let _ = app.run("layout.new", json!({}));
                }
            } else {
                list_button = Some(resp);
            }
            x += 22.0;
        }
        x = x.max(r.left() + if app.ui.show_toolsets { 232.0 } else { 60.0 });
        // Right side: toggles, right-aligned.
        let s = app.session.settings.clone();
        let anno_all = app.session.doc().is_ok_and(|d| d.anno_all_visible());
        use cadcraft_engine::snap::mode;
        let toggles: Vec<(Icon, bool, &str, &str)> = vec![
            (Icon::Grid, s.gridmode, "grid", "Grid (F7)"),
            (Icon::Snap, s.snapmode, "snap", "Snap Mode (F9)"),
            (Icon::Ortho, s.orthomode, "ortho", "Ortho Mode (F8)"),
            (Icon::AxisTrack, s.axistrack, "axistrack", "3D Axis Tracking"),
            (Icon::Polar, s.polarmode, "polar", "Polar Tracking (F10)"),
            (Icon::Isodraft, s.isodraft, "isodraft", "Isometric Drafting"),
            (Icon::OTrack, s.otrack, "otrack", "Object Snap Tracking (F11)"),
            (Icon::Osnap, s.osmode & mode::OFF == 0 && s.osmode != 0, "osnap", "Object Snap (F3)"),
            (Icon::Lineweight, s.lwdisplay, "lwdisplay", "Show/Hide Lineweight"),
            (Icon::Transparency, s.transparency_display, "transparencydisplay", "Show/Hide Transparency"),
            (Icon::DynInput, s.dynmode, "dynmode", "Dynamic Input (F12)"),
            (Icon::QuickProps, s.qpmode, "qpmode", "Quick Properties"),
            (Icon::Annotation, anno_all, "annovisible.toggle", "Annotation Visibility"),
            (Icon::Workspace, false, "ui.noop", "Workspace Switching"),
            (Icon::Gear, false, "ui.dialog.dsettings", "Drafting Settings"),
        ];
        let size = 20.0;
        let mut rx = r.right() - 8.0 - toggles.len() as f32 * (size + 4.0);
        // Coordinates.
        let coord = app.canvas.cursor3.map(|cursor| {
            let (lu, lp) = app.session.doc().map(|d| (d.header.i64("LUNITS", 2), d.header.i64("LUPREC", 4))).unwrap_or((2, 4));
            format_cursor_coordinates(cursor, app.session.drawing_plane(), lu, lp)
        });
        // MODEL / PAPER (layouts only): toggles MSPACE and PSPACE like AutoCAD's status button.
        let mut coord_right = rx - 8.0;

        // Drawing plane button: XY / XZ / YZ
        let plane = app.session.drawing_plane();
        let plane_text = crate::tf!("Plane: {plane}", plane = plane.name());
        let br_plane = Rect::from_min_max(pos2(coord_right - 62.0, r.center().y - 10.0), pos2(coord_right - 4.0, r.center().y + 10.0));
        let resp_plane = ui.interact(br_plane, ui.id().with("drawing_plane_toggle"), Sense::click());
        let plane_hovered = resp_plane.hovered();
        let is_non_xy = plane != cadcraft_geom::DrawingPlane::XY;
        icons::describe_control(ui, &resp_plane, &plane_text, Some(is_non_xy), false);
        p.rect_filled(
            br_plane,
            3.0,
            if plane_hovered {
                t.control_hover
            } else if is_non_xy {
                t.toggle_on.gamma_multiply(0.25)
            } else {
                t.chrome
            },
        );
        p.rect_stroke(
            br_plane,
            3.0,
            Stroke::new(
                1.0,
                if is_non_xy {
                    t.toggle_on
                } else if plane_hovered {
                    t.text_dim
                } else {
                    t.border
                },
            ),
            egui::StrokeKind::Inside,
        );
        p.text(br_plane.center(), egui::Align2::CENTER_CENTER, &plane_text, crate::theme::small(), if is_non_xy { t.toggle_on } else { t.text });
        if resp_plane.on_hover_text(crate::tf!("Drawing plane: {plane} (click to cycle XY -> XZ -> YZ)", plane = plane.name())).clicked() {
            let next = match plane {
                cadcraft_geom::DrawingPlane::XY => cadcraft_geom::DrawingPlane::XZ,
                cadcraft_geom::DrawingPlane::XZ => cadcraft_geom::DrawingPlane::YZ,
                cadcraft_geom::DrawingPlane::YZ => cadcraft_geom::DrawingPlane::XY,
            };
            app.session.set_drawing_plane(next);
            app.session.echo(crate::tf!("Active drawing plane: {plane}.", plane = next.name()));
        }
        coord_right = br_plane.left() - 6.0;

        // Elevation / Cota button with mouse adjust and origin return
        let elev_offset = app.session.effective_drawing_plane_offset();
        let elev_manual = app.session.drawing_plane_offset_is_manual();
        let elev_active = elev_manual || app.canvas.height_adjust;
        let elev_text = if !elev_manual {
            crate::tl!("Elev: Auto").to_string()
        } else if elev_offset.abs() < 1e-6 {
            crate::tl!("Elev: 0.00").to_string()
        } else {
            crate::tf!("Elev: {offset}", offset = format!("{elev_offset:+.2}"))
        };
        let br_elev = Rect::from_min_max(pos2(coord_right - 88.0, r.center().y - 10.0), pos2(coord_right - 4.0, r.center().y + 10.0));
        let resp_elev = ui.interact(br_elev, ui.id().with("drawing_plane_elev_toggle"), Sense::click());
        let elev_hovered = resp_elev.hovered();
        icons::describe_control(ui, &resp_elev, &elev_text, Some(elev_active), false);
        p.rect_filled(
            br_elev,
            3.0,
            if app.canvas.height_adjust {
                t.toggle_on
            } else if elev_hovered {
                t.control_hover
            } else if elev_active {
                t.toggle_on.gamma_multiply(0.25)
            } else {
                t.chrome
            },
        );
        p.rect_stroke(
            br_elev,
            3.0,
            Stroke::new(
                1.0,
                if app.canvas.height_adjust || elev_active {
                    t.toggle_on
                } else if elev_hovered {
                    t.text_dim
                } else {
                    t.border
                },
            ),
            egui::StrokeKind::Inside,
        );
        p.text(
            br_elev.center(),
            egui::Align2::CENTER_CENTER,
            &elev_text,
            crate::theme::small(),
            if app.canvas.height_adjust {
                t.chrome_dark
            } else if elev_active {
                t.toggle_on
            } else {
                t.text
            },
        );
        let elev_clicked = resp_elev.clicked();
        let elev_sec_clicked = resp_elev.secondary_clicked();
        let _ = resp_elev.on_hover_text(crate::tf!(
            "Drawing plane offset: {offset} (Click: adjust manually, right-click: return to automatic)",
            offset = if elev_manual { format!("{elev_offset:.2}") } else { crate::tl!("Automatic").to_string() }
        ));
        if elev_clicked {
            app.canvas.height_adjust = !app.canvas.height_adjust;
            if app.canvas.height_adjust {
                let plane = app.session.drawing_plane();
                app.canvas.height_adjust_anchor = Some(plane.embed(plane.coordinates(app.session.last_point3), elev_offset));
                app.canvas.height_adjust_start_offset = app.session.drawing_plane_offset();
                app.canvas.height_adjust_start_manual = elev_manual;
                app.canvas.height_adjust_start_plane_manual = app.session.drawing_plane_is_manual();
                if !app.canvas.height_adjust_start_plane_manual {
                    app.session.set_drawing_plane(plane);
                }
                app.session.set_drawing_plane_offset(elev_offset);
                app.session.echo(crate::tl!("Graphical plane offset adjustment started (move the mouse, click to fix, Esc to cancel)."));
            }
        }
        if elev_sec_clicked {
            app.canvas.height_adjust = false;
            app.session.set_drawing_plane_automatic();
            app.session.set_drawing_plane_offset_automatic();
            app.session.echo(crate::tl!("Drawing plane elevation returned to automatic mode."));
        }
        coord_right = br_elev.left() - 6.0;

        let zlock = app.session.settings.zlock;
        let br_zlock = Rect::from_min_max(pos2(coord_right - 64.0, r.center().y - 10.0), pos2(coord_right - 4.0, r.center().y + 10.0));
        let resp_zlock = ui.interact(br_zlock, ui.id().with("zlock_toggle"), Sense::click());
        let zlock_hovered = resp_zlock.hovered();
        let zlock_text = if zlock { "● V-LOCK" } else { "V-LOCK" };
        icons::describe_control(ui, &resp_zlock, zlock_text, Some(zlock), false);
        if zlock {
            p.rect_filled(br_zlock, 3.0, t.toggle_on);
            p.rect_stroke(br_zlock, 3.0, Stroke::new(1.5, egui::Color32::WHITE), egui::StrokeKind::Inside);
            p.text(br_zlock.center(), egui::Align2::CENTER_CENTER, zlock_text, crate::theme::small(), t.chrome_dark);
        } else {
            p.rect_filled(br_zlock, 3.0, if zlock_hovered { t.control_hover } else { t.chrome });
            p.rect_stroke(br_zlock, 3.0, Stroke::new(1.0, if zlock_hovered { t.text_dim } else { t.border }), egui::StrokeKind::Inside);
            p.text(
                br_zlock.center(),
                egui::Align2::CENTER_CENTER,
                zlock_text,
                crate::theme::small(),
                if zlock_hovered { t.text } else { t.text_dim },
            );
        }
        if resp_zlock
            .on_hover_text(if zlock {
                crate::tl!("Vertical lock (V): ON (click to turn off)")
            } else {
                crate::tl!("Vertical lock (V): OFF (click to turn on)")
            })
            .clicked()
        {
            app.session.settings.zlock = !app.session.settings.zlock;
            let state = if app.session.settings.zlock { crate::tl!("On") } else { crate::tl!("Off") };
            app.session.echo(crate::tf!("Vertical lock {state}.", state = state));
        }
        coord_right = br_zlock.left() - 6.0;
        // The annotation scale list (model space).
        coord_right = annoscale::status(app, ui, coord_right, r.center().y);
        if matches!(app.session.layout_space(), cadcraft_doc::Space::Paper(_)) {
            let model = app.session.state().is_ok_and(|st| st.mspace.is_some());
            let label = if model { "MODEL" } else { "PAPER" };
            let br = Rect::from_min_max(pos2(rx - 60.0, r.center().y - 9.0), pos2(rx - 6.0, r.center().y + 9.0));
            let resp = ui.interact(br, ui.id().with("mspace_toggle"), Sense::click());
            p.rect_filled(br, 3.0, if resp.hovered() { t.control_hover } else { t.toggle_on.gamma_multiply(0.25) });
            p.text(br.center(), egui::Align2::CENTER_CENTER, crate::i18n::t(label), crate::theme::small(), t.text);
            icons::describe_control(ui, &resp, label, None, false);
            if resp.on_hover_text(crate::tl!("Switch between model space in a viewport and paper space")).clicked() {
                let _ = app.run(if model { "pspace" } else { "mspace" }, json!({}));
                app.canvas.list = None;
            }
            coord_right = br.left() - 10.0;
            // Scale and lock of the active or selected viewport.
            coord_right = crate::vpscale::status(app, ui, coord_right + 4.0, r.center().y) - 10.0;
        }
        // Layout tabs: Model, "+", then the layouts that fit before the right-hand controls (the
        // current one always); the layout list reaches the rest.
        let mut tabs: Vec<String> = vec!["Model".into()];
        let cur = app.session.state().map(|s| match &s.space {
            cadcraft_doc::Space::Model => "Model".to_string(),
            cadcraft_doc::Space::Paper(n) => n.clone(),
        });
        if let Ok(st) = app.session.state() {
            let mut ls: Vec<_> = st.doc.layouts.iter().collect();
            ls.sort_by_key(|l| l.tab_order);
            tabs.extend(ls.iter().map(|l| l.name.clone()));
        }
        let shown_name = |i: usize, name: &str| if i == 0 { crate::tl!("Model").to_string() } else { name.to_string() };
        let limit = (coord_right - STATUS_READOUT_MIN).max(x);
        let labels: Vec<_> =
            tabs.iter().enumerate().map(|(i, n)| elided(&p, &shown_name(i, n), crate::theme::body(), t.text, LAYOUT_TAB_TEXT)).collect();
        let widths: Vec<f32> = labels.iter().map(|(g, _)| g.size().x + 26.0 + 2.0).collect();
        let active_tab = tabs.iter().position(|n| cur.as_deref().is_ok_and(|c| c == n));
        // Model and "+" stay; the layouts share what is left.
        let layouts = widths.get(1..).unwrap_or_default();
        let room = limit - x - widths.first().copied().unwrap_or(0.0) - 22.0;
        let active_layout = active_tab.and_then(|a| a.checked_sub(1));
        let mut shown = visible_tabs(layouts, active_layout, room);
        let hidden = shown.len() < layouts.len();
        if hidden {
            shown = visible_tabs(layouts, active_layout, room - 22.0);
        }
        let clip = Rect::from_min_max(r.min, pos2(limit.max(r.left()), r.bottom()));
        let tp = p.with_clip_rect(clip);
        let mut switch = None;
        for (i, (name, (g, cut))) in tabs.iter().zip(labels).enumerate() {
            if i > 0 && !shown.contains(&(i - 1)) {
                continue;
            }
            let w = g.size().x + 26.0;
            let tr = Rect::from_min_size(pos2(x, r.top() + 1.0), vec2(w, r.height() - 2.0)).intersect(clip);
            let active = active_tab == Some(i);
            let resp = ui.interact(tr, ui.id().with(("lt", i)), Sense::click());
            if active {
                tp.rect_filled(tr, 2.0, t.tab_active);
            } else if resp.hovered() {
                tp.rect_filled(tr, 2.0, t.control.gamma_multiply(0.5));
            }
            tp.galley(pos2(tr.left() + 13.0, tr.center().y - g.size().y / 2.0), g, if active { t.text } else { t.text_dim });
            icons::describe_control(ui, &resp, name, Some(active), true);
            let resp = if cut { resp.on_hover_text(shown_name(i, name)) } else { resp };
            if resp.clicked() {
                switch = Some(name.clone());
            }
            crate::shortcut_menus::attach(app, &resp, |app| crate::shortcut_menus::layout_tab_entries(app, name));
            x += w + 2.0;
            if i == 0 {
                let pr = Rect::from_min_size(pos2(x, r.top() + 5.0), vec2(16.0, 16.0));
                let presp = ui.interact(pr, ui.id().with("layout_plus"), Sense::click());
                icons::paint(&p, pr.shrink(2.0), Icon::Plus, false);
                icons::describe_control(ui, &presp, "New layout", None, false);
                if presp.on_hover_text(crate::tl!("New layout")).clicked()
                    && let Ok(v) = app.run("layout.new", json!({}))
                    && let Some(n) = v.get("name").and_then(serde_json::Value::as_str)
                {
                    switch = Some(n.to_string());
                }
                x += 22.0;
            }
        }
        // The layouts that didn't fit, behind a button after the last tab shown.
        let more = hidden.then(|| {
            let mr = Rect::from_min_size(pos2(x + 2.0, r.top() + 5.0), vec2(16.0, 16.0));
            let resp = ui.interact(mr, ui.id().with("layout_more"), Sense::click());
            if resp.hovered() {
                p.rect_filled(mr, 3.0, t.control_hover.gamma_multiply(0.5));
            }
            icons::paint(&p, mr.shrink(2.0), Icon::ChevronDown, false);
            icons::describe_control(ui, &resp, "Layout list", None, false);
            x += 22.0;
            resp.on_hover_text(crate::tl!("Layout list"))
        });
        for resp in list_button.iter().chain(&more) {
            egui::Popup::from_toggle_button_response(resp).width(200.0).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for (i, name) in tabs.iter().enumerate() {
                        if ui.add(egui::Button::selectable(active_tab == Some(i), shown_name(i, name)).truncate()).clicked() {
                            switch = Some(name.clone());
                            ui.close();
                        }
                    }
                });
            });
        }
        if let Some(n) = switch {
            let _ = app.run("layout.set", json!({ "name": n }));
        }
        // A transient message occupies the same bounded readout slot as coordinates.
        // Ellipsis and a tooltip keep either value readable without covering layout tabs.
        let message = app.status.as_ref().filter(|(_, at)| crate::now_ms() - at < 5000.0).map(|(message, _)| message);
        let readout = message.cloned().or(coord);
        if let Some(text) = readout {
            let left = x + 8.0;
            let width = (coord_right - left).max(0.0);
            if width > 4.0 {
                let color = if message.is_some() { t.warn } else { t.text };
                let mut job = egui::text::LayoutJob::simple_singleline(text.clone(), crate::theme::body(), color);
                job.wrap.max_width = width;
                job.wrap.max_rows = 1;
                job.wrap.break_anywhere = true;
                let galley = p.layout_job(job);
                let rect = Rect::from_min_max(pos2(left, r.top()), pos2(coord_right, r.bottom()));
                let text_x = if message.is_some() { left } else { coord_right - galley.size().x };
                p.with_clip_rect(rect).galley(pos2(text_x, r.center().y - galley.size().y / 2.0), galley, color);
                ui.interact(rect, ui.id().with("status_readout"), Sense::hover()).on_hover_text(text);
            }
        }
        let mut toggle_cmd = None;
        for (icon, on, cmd, tip) in toggles {
            let br = Rect::from_min_size(pos2(rx, r.center().y - size / 2.0), vec2(size, size));
            let resp = ui.interact(br, ui.id().with(("tg", tip)), Sense::click());
            if on {
                p.rect_filled(br, 3.0, t.toggle_on.gamma_multiply(0.35));
            } else if resp.hovered() {
                p.rect_filled(br, 3.0, t.control_hover.gamma_multiply(0.5));
            }
            icons::paint(&p, br.shrink(2.5), icon, false);
            if on {
                // Re-tint the icon blue-ish by an underline.
                p.hline(br.x_range().shrink(4.0), br.bottom() - 1.0, Stroke::new(1.5, t.toggle_on));
            }
            icons::describe_control(ui, &resp, tip, Some(on), false);
            if resp.on_hover_text(crate::i18n::t(tip)).clicked() {
                toggle_cmd = Some(cmd);
            }
            rx += size + 4.0;
        }
        // The toggles don't cancel the active command (#1108); the dialog buttons start theirs.
        match toggle_cmd {
            Some(c) if c.starts_with("ui.") => app.start(c),
            Some(c) => crate::menus::run_transparent(app, c),
            None => {}
        }
    });
}

/// The Start page (no drawing open, or the Start tab).
pub fn start_page(app: &mut CadApp, ui: &mut egui::Ui) {
    use egui::Color32;
    let t = Tokens::get();
    let r = ui.max_rect();
    let p = ui.painter().clone();
    let left = Rect::from_min_size(r.min, vec2(280.0, r.height()));
    p.rect_filled(left, 0.0, t.chrome_dark);
    p.rect_filled(Rect::from_min_max(pos2(left.right(), r.top()), r.max), 0.0, t.chrome);

    // Left branding
    p.text(pos2(left.left() + 34.0, left.top() + 70.0), egui::Align2::LEFT_CENTER, crate::tl!("CADCraft"), egui::FontId::proportional(28.0), t.text);
    p.text(
        pos2(left.left() + 34.0, left.top() + 98.0),
        egui::Align2::LEFT_CENTER,
        crate::tf!("Version {version}", version = env!("CARGO_PKG_VERSION")),
        crate::theme::small(),
        t.text_faint,
    );

    let mut y = left.top() + 140.0;
    let mut action = None;
    for (label, cmd) in [
        ("New", "new"),
        ("New from Template...", "ui.newtemplate"),
        ("New (metric)", "new.metric"),
        ("Open...", "open"),
        ("Open sample drawing", "sample"),
    ] {
        let br = Rect::from_min_size(pos2(left.left() + 34.0, y), vec2(212.0, 32.0));
        let resp = ui.interact(br, ui.id().with(("start", cmd)), Sense::click());
        p.rect_stroke(br, 2.0, Stroke::new(1.0, if resp.hovered() { t.text } else { t.text_dim }), egui::StrokeKind::Inside);
        p.text(pos2(br.left() + 14.0, br.center().y), egui::Align2::LEFT_CENTER, crate::i18n::t(label), crate::theme::body(), t.text);
        if resp.clicked() {
            action = Some(cmd);
        }
        y += 40.0;
    }

    // Template section in sidebar
    y += 16.0;
    p.line_segment([pos2(left.left() + 34.0, y), pos2(left.right() - 34.0, y)], Stroke::new(1.0, t.border));
    y += 16.0;
    p.text(pos2(left.left() + 34.0, y), egui::Align2::LEFT_CENTER, crate::tl!("Default Template:"), crate::theme::small(), t.text_dim);
    y += 18.0;

    let current_tmpl = cadcraft_engine::cmd::file::qnew_template();
    let tmpl_str = if current_tmpl.is_empty() { crate::tl!("None (Blank drawing)").to_string() } else { crate::menus::file_name(&current_tmpl) };
    let tmpl_rect = Rect::from_min_size(pos2(left.left() + 34.0, y - 8.0), vec2(212.0, 20.0));
    p.with_clip_rect(tmpl_rect).text(pos2(left.left() + 34.0, y), egui::Align2::LEFT_CENTER, tmpl_str, crate::theme::body(), t.text);
    y += 24.0;

    let choose_rect = Rect::from_min_size(pos2(left.left() + 34.0, y), vec2(100.0, 26.0));
    let resp_choose = ui.interact(choose_rect, ui.id().with("start_choose_template"), Sense::click());
    p.rect_stroke(choose_rect, 2.0, Stroke::new(1.0, if resp_choose.hovered() { t.text } else { t.text_dim }), egui::StrokeKind::Inside);
    p.text(choose_rect.center(), egui::Align2::CENTER_CENTER, crate::tl!("Choose..."), crate::theme::small(), t.text);
    if resp_choose.clicked() {
        action = Some("ui.defaulttemplate.choose");
    }

    if !current_tmpl.is_empty() {
        let clear_rect = Rect::from_min_size(pos2(left.left() + 144.0, y), vec2(80.0, 26.0));
        let resp_clear = ui.interact(clear_rect, ui.id().with("start_clear_template"), Sense::click());
        p.rect_stroke(clear_rect, 2.0, Stroke::new(1.0, if resp_clear.hovered() { t.text } else { t.text_dim }), egui::StrokeKind::Inside);
        p.text(clear_rect.center(), egui::Align2::CENTER_CENTER, crate::tl!("Clear"), crate::theme::small(), t.text);
        if resp_clear.clicked() {
            action = Some("ui.defaulttemplate.clear");
        }
    }

    // Main area
    let main_rect = Rect::from_min_max(pos2(left.right() + 32.0, r.top() + 28.0), pos2(r.right() - 24.0, r.bottom() - 16.0));
    let mut to_open = None;
    let mut to_pin = None;
    let mut to_forget = None;

    ui.scope_builder(egui::UiBuilder::new().max_rect(main_rect), |ui| {
        // Header
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(crate::tl!("Recent Drawings")).font(egui::FontId::proportional(22.0)).color(t.text));
            ui.add_space(24.0);

            // Search
            ui.label(egui::RichText::new(crate::tl!("Search:")).color(t.text_dim));
            ui.add(egui::TextEdit::singleline(&mut app.ui.start_search).hint_text(crate::tl!("Filter by name or path...")));
            if !app.ui.start_search.is_empty() && ui.small_button("×").clicked() {
                app.ui.start_search.clear();
            }

            // View toggle
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.selectable_label(!app.ui.start_grid_view, crate::tl!("List")).clicked() {
                    app.ui.start_grid_view = false;
                }
                if ui.selectable_label(app.ui.start_grid_view, crate::tl!("Grid")).clicked() {
                    app.ui.start_grid_view = true;
                }
            });
        });

        ui.add_space(14.0);

        // Filter and sort items: combine pinned and recent files (deduplicating with pinned first)
        // so that pinned favorites are preserved even if they drop out of the limited recent_files list.
        let mut combined_files: Vec<String> = Vec::new();
        for p in &app.ui.pinned_files {
            if !combined_files.contains(p) {
                combined_files.push(p.clone());
            }
        }
        for p in &app.ui.recent_files {
            if !combined_files.contains(p) {
                combined_files.push(p.clone());
            }
        }

        let query = app.ui.start_search.trim().to_lowercase();
        let files: Vec<String> = combined_files
            .into_iter()
            .filter(|p| {
                if query.is_empty() { true } else { crate::menus::file_name(p).to_lowercase().contains(&query) || p.to_lowercase().contains(&query) }
            })
            .collect();
        app.prune_thumbnail_textures();

        if files.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(80.0);
                ui.label(egui::RichText::new(crate::tl!("There is nothing here yet.")).font(egui::FontId::proportional(18.0)).color(t.text));
                ui.add_space(8.0);
                ui.label(egui::RichText::new(crate::tl!("To get started, create or open a drawing.")).color(t.text_dim));
            });
            return;
        }

        let ctx = ui.ctx().clone();
        egui::ScrollArea::vertical().id_salt("start_recents_scroll").show(ui, |ui| {
            if app.ui.start_grid_view {
                // Card Grid View
                let card_w = 230.0f32;
                let card_h = 180.0f32;
                let gap = 16.0f32;
                let avail_w = ui.available_width();
                let cols = ((avail_w + gap) / (card_w + gap)).floor().max(1.0) as usize;

                for chunk in files.chunks(cols) {
                    ui.horizontal(|ui| {
                        for path in chunk {
                            let (card_rect, resp) = ui.allocate_exact_size(vec2(card_w, card_h), Sense::click());
                            let cp = ui.painter().with_clip_rect(card_rect);
                            let is_pinned = app.ui.pinned_files.contains(path);
                            let exists = crate::thumbnails::file_exists(path);

                            // Card background & border
                            let bg = if resp.hovered() { t.control_hover.gamma_multiply(0.35) } else { t.panel };
                            let border_color = if is_pinned {
                                t.accent
                            } else if resp.hovered() {
                                t.text_dim
                            } else {
                                t.border
                            };
                            cp.rect_filled(card_rect, 4.0, bg);
                            cp.rect_stroke(card_rect, 4.0, Stroke::new(if is_pinned { 1.5 } else { 1.0 }, border_color), egui::StrokeKind::Inside);

                            // Thumbnail image area
                            let thumb_rect = Rect::from_min_size(pos2(card_rect.left() + 6.0, card_rect.top() + 6.0), vec2(card_w - 12.0, 106.0));
                            if !app.thumbnail_textures.contains_key(path) {
                                if let Some(tex) = crate::thumbnails::load_thumbnail_texture(&ctx, path) {
                                    app.thumbnail_textures.insert(path.clone(), tex);
                                }
                            }

                            if let Some(tex) = app.thumbnail_textures.get(path) {
                                cp.image(tex.id(), thumb_rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                            } else {
                                cp.rect_filled(thumb_rect, 2.0, t.canvas);
                                cp.text(thumb_rect.center(), egui::Align2::CENTER_CENTER, "📐", egui::FontId::proportional(26.0), t.text_faint);
                            }

                            // Missing file badge
                            if !exists {
                                let badge_r = Rect::from_min_size(pos2(thumb_rect.left() + 6.0, thumb_rect.top() + 6.0), vec2(60.0, 18.0));
                                cp.rect_filled(badge_r, 2.0, Color32::from_rgb(180, 50, 40));
                                cp.text(badge_r.center(), egui::Align2::CENTER_CENTER, crate::tl!("Missing"), crate::theme::small(), Color32::WHITE);

                                // Remove button
                                let rm_r = Rect::from_min_size(pos2(card_rect.right() - 50.0, card_rect.top() + 8.0), vec2(18.0, 18.0));
                                let rm_resp = ui.interact(rm_r, ui.id().with(("rm", path)), Sense::click());
                                cp.rect_filled(
                                    rm_r,
                                    2.0,
                                    if rm_resp.hovered() { Color32::from_rgb(200, 40, 40) } else { Color32::from_black_alpha(140) },
                                );
                                cp.text(rm_r.center(), egui::Align2::CENTER_CENTER, "×", crate::theme::body(), Color32::WHITE);
                                if rm_resp.clicked() {
                                    to_forget = Some(path.clone());
                                }
                            }

                            // Pin toggle button
                            let pin_r = Rect::from_min_size(pos2(card_rect.right() - 26.0, card_rect.top() + 8.0), vec2(18.0, 18.0));
                            let pin_resp = ui.interact(pin_r, ui.id().with(("pin", path)), Sense::click());
                            cp.rect_filled(pin_r, 2.0, if pin_resp.hovered() { t.control_hover } else { Color32::from_black_alpha(140) });
                            let pin_icon = if is_pinned { "★" } else { "☆" };
                            let pin_color = if is_pinned { t.accent } else { Color32::WHITE };
                            cp.text(pin_r.center(), egui::Align2::CENTER_CENTER, pin_icon, egui::FontId::proportional(12.0), pin_color);
                            if pin_resp.clicked() {
                                to_pin = Some(path.clone());
                            }

                            // Metadata text
                            let text_area = Rect::from_min_max(
                                pos2(card_rect.left() + 8.0, card_rect.top() + 118.0),
                                pos2(card_rect.right() - 8.0, card_rect.bottom() - 4.0),
                            );
                            let text_p = cp.with_clip_rect(text_area);
                            let fname = crate::menus::file_name(path);
                            text_p.text(pos2(text_area.left(), text_area.top() + 2.0), egui::Align2::LEFT_TOP, fname, crate::theme::body(), t.text);

                            let dir = std::path::Path::new(path).parent().map(|d| d.display().to_string()).unwrap_or_default();
                            text_p.text(
                                pos2(text_area.left(), text_area.top() + 20.0),
                                egui::Align2::LEFT_TOP,
                                dir,
                                crate::theme::small(),
                                t.text_faint,
                            );

                            let mtime = crate::thumbnails::file_modified_time(path);
                            text_p.text(
                                pos2(text_area.left(), text_area.top() + 38.0),
                                egui::Align2::LEFT_TOP,
                                mtime,
                                crate::theme::small(),
                                t.text_dim,
                            );

                            if resp.on_hover_text(path).clicked() && to_pin.is_none() && to_forget.is_none() {
                                to_open = Some(path.clone());
                            }
                        }
                    });
                    ui.add_space(gap);
                }
            } else {
                // List View
                for (i, path) in files.iter().enumerate() {
                    let (row_r, resp) = ui.allocate_exact_size(vec2(ui.available_width().clamp(200.0, 720.0), 46.0), Sense::click());
                    let cp = ui.painter().with_clip_rect(row_r);
                    let is_pinned = app.ui.pinned_files.contains(path);
                    let exists = crate::thumbnails::file_exists(path);

                    if resp.hovered() {
                        cp.rect_filled(row_r, 2.0, t.control_hover.gamma_multiply(0.4));
                    }
                    if is_pinned {
                        cp.rect_stroke(row_r, 2.0, Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
                    }

                    // Mini thumbnail
                    let mini_r = Rect::from_min_size(pos2(row_r.left() + 6.0, row_r.top() + 6.0), vec2(50.0, 34.0));
                    if !app.thumbnail_textures.contains_key(path) {
                        if let Some(tex) = crate::thumbnails::load_thumbnail_texture(&ctx, path) {
                            app.thumbnail_textures.insert(path.clone(), tex);
                        }
                    }
                    if let Some(tex) = app.thumbnail_textures.get(path) {
                        cp.image(tex.id(), mini_r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    } else {
                        cp.rect_filled(mini_r, 2.0, t.canvas);
                        cp.text(mini_r.center(), egui::Align2::CENTER_CENTER, "📐", egui::FontId::proportional(16.0), t.text_faint);
                    }

                    // Text labels
                    let fname = crate::menus::file_name(path);
                    let dir = std::path::Path::new(path).parent().map(|d| d.display().to_string()).unwrap_or_default();
                    let mtime = crate::thumbnails::file_modified_time(path);

                    cp.text(pos2(row_r.left() + 66.0, row_r.top() + 14.0), egui::Align2::LEFT_CENTER, fname, crate::theme::body(), t.text);
                    cp.text(pos2(row_r.left() + 66.0, row_r.top() + 32.0), egui::Align2::LEFT_CENTER, dir, crate::theme::small(), t.text_faint);
                    let mtime_label = if mtime.is_empty() { String::new() } else { format!("{}: {mtime}", crate::tl!("Modified")) };
                    cp.text(pos2(row_r.right() - 80.0, row_r.center().y), egui::Align2::RIGHT_CENTER, mtime_label, crate::theme::small(), t.text_dim);

                    // Pin button
                    let pin_r = Rect::from_min_size(pos2(row_r.right() - 30.0, row_r.center().y - 10.0), vec2(20.0, 20.0));
                    let pin_resp = ui.interact(pin_r, ui.id().with(("list_pin", i)), Sense::click());
                    let pin_icon = if is_pinned { "★" } else { "☆" };
                    let pin_color = if is_pinned { t.accent } else { t.text_dim };
                    cp.text(pin_r.center(), egui::Align2::CENTER_CENTER, pin_icon, egui::FontId::proportional(14.0), pin_color);
                    if pin_resp.clicked() {
                        to_pin = Some(path.clone());
                    }

                    // Missing badge & remove button
                    if !exists {
                        let badge_r = Rect::from_min_size(pos2(row_r.right() - 150.0, row_r.center().y - 9.0), vec2(56.0, 18.0));
                        cp.rect_filled(badge_r, 2.0, Color32::from_rgb(180, 50, 40));
                        cp.text(badge_r.center(), egui::Align2::CENTER_CENTER, crate::tl!("Missing"), crate::theme::small(), Color32::WHITE);

                        let rm_r = Rect::from_min_size(pos2(row_r.right() - 54.0, row_r.center().y - 9.0), vec2(18.0, 18.0));
                        let rm_resp = ui.interact(rm_r, ui.id().with(("list_rm", i)), Sense::click());
                        cp.text(rm_r.center(), egui::Align2::CENTER_CENTER, "×", crate::theme::body(), t.text_dim);
                        if rm_resp.clicked() {
                            to_forget = Some(path.clone());
                        }
                    }

                    if resp.on_hover_text(path).clicked() && to_pin.is_none() && to_forget.is_none() {
                        to_open = Some(path.clone());
                    }
                    ui.add_space(4.0);
                }
            }
        });
    });

    if let Some(path) = to_pin {
        app.toggle_pinned(&path);
    }
    if let Some(path) = to_forget {
        app.forget_recent(&path);
    }
    if let Some(path) = to_open {
        app.open_path(&path);
    }

    match action {
        Some("open") => app.start("ui.open"),
        Some("new") => {
            app.start("new");
            app.ui.start_tab = false;
        }
        Some("new.metric") => {
            let _ = app.run("new", json!({ "metric": true }));
            app.ui.start_tab = false;
        }
        Some("sample") => {
            app.start("ui.sample");
        }
        Some("ui.newtemplate") => {
            app.start("ui.newtemplate");
        }
        Some("ui.defaulttemplate.choose") => {
            app.start("ui.defaulttemplate.choose");
        }
        Some("ui.defaulttemplate.clear") => {
            app.start("ui.defaulttemplate.clear");
        }
        _ => {}
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use cadcraft_geom::{DrawingPlane, Vec2, Vec3};

    #[test]
    fn mac_title_row_centres_and_clears_the_traffic_lights() {
        // The lights are centred in the row, and content keeps the same gap after them.
        let margin = (MAC_TITLE_BAR - MAC_LIGHT) / 2.0;
        let lights_end = MAC_LIGHTS_LEFT + 2.0 * MAC_LIGHT_PITCH + MAC_LIGHT;
        assert_eq!(MAC_CONTENT_LEFT - lights_end, margin);
        // A short title is centred; one too wide for that starts after the lights.
        assert_eq!(title_left(100.0, 1000.0, 200.0), 500.0);
        assert_eq!(title_left(100.0, 900.0, 800.0), 100.0 + MAC_CONTENT_LEFT);
    }

    #[test]
    fn layout_tabs_that_do_not_fit_keep_the_current_one() {
        let w = [50.0; 6];
        // All fit, with or without a current tab.
        assert_eq!(visible_tabs(&w, None, 300.0), 0..6);
        assert_eq!(visible_tabs(&w, Some(5), 300.0), 0..6);
        // Too many: the first ones, unless the current one is further on.
        assert_eq!(visible_tabs(&w, None, 170.0), 0..3);
        assert_eq!(visible_tabs(&w, Some(1), 170.0), 0..3);
        assert_eq!(visible_tabs(&w, Some(4), 170.0), 2..5);
        // No room at all: nothing but the current tab; hostile numbers don't panic.
        assert_eq!(visible_tabs(&w, None, -10.0), 0..0);
        assert_eq!(visible_tabs(&w, Some(3), 0.0), 3..4);
        assert_eq!(visible_tabs(&w, Some(9), f32::NAN), 0..0);
        assert_eq!(visible_tabs(&[], Some(0), 100.0), 0..0);
    }

    #[test]
    fn status_coordinates_show_world_axes_during_non_xy_line_prompts() {
        let cursor = Vec3::new(3.0, 9.0, 4.0);
        assert_eq!(format_cursor_coordinates(cursor, DrawingPlane::XZ, 2, 4), "[WCS] 3.0000, 9.0000, 4.0000");
        assert_eq!(DrawingPlane::XZ.embed(Vec2::new(3.0, 4.0), 9.0), cursor);
    }
}
