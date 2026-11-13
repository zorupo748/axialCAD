//! The system clipboard: Cut/Copy/Paste key presses, and the drawing clipboard mirrored as DXF
//! text so it can be pasted into another CADCraft window.
//!
//! egui turns Cmd/Ctrl+C, X and V into `Copy`, `Cut` and `Paste(text)` events instead of key
//! presses, so the clipboard shortcuts are read from those. A `Paste` arrives only when the system
//! clipboard holds text; every copy therefore puts its DXF text there.

use std::hash::{DefaultHasher, Hash, Hasher};

use cadcraft_engine::cmd::clipboard;

use crate::CadApp;

/// egui memory: the clipboard copy last mirrored, and the hash of the text it became.
fn serial_id() -> egui::Id {
    egui::Id::new("cc_clipboard_serial")
}
fn text_id() -> egui::Id {
    egui::Id::new("cc_clipboard_text")
}

/// Line endings normalised (pasted text arrives with `\n`), so our own text is recognised.
fn hash(text: &str) -> u64 {
    let mut h = DefaultHasher::new();
    text.replace("\r\n", "\n").hash(&mut h);
    h.finish()
}

/// Put a new copy (COPYCLIP, CUTCLIP, COPYBASE from anywhere) on the system clipboard, once.
pub fn mirror(app: &CadApp, ctx: &egui::Context) {
    let Some(src) = &app.session.clipboard_source else { return };
    if src.uid.is_none() || ctx.data(|d| d.get_temp::<u64>(serial_id())) == Some(src.serial) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(serial_id(), src.serial));
    if let Some(text) = clipboard::system_text(&app.session) {
        ctx.data_mut(|d| d.insert_temp(text_id(), hash(&text)));
        ctx.copy_text(text);
    }
}

/// The commands the Cut/Copy/Paste events of this frame ask for. Pasted DXF text that isn't our
/// own last copy (another CADCraft's) becomes the clipboard first.
pub fn shortcut_commands(app: &mut CadApp, ctx: &egui::Context) -> Vec<&'static str> {
    let wants_text_paste = crate::cmdline::wants_text_paste(app);
    let (events, shift) = ctx.input(|i| {
        let ev: Vec<egui::Event> =
            i.events.iter().filter(|e| matches!(e, egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_))).cloned().collect();
        (ev, i.modifiers.shift)
    });
    let mut out = Vec::new();
    for e in events {
        match e {
            egui::Event::Copy => out.push(if shift { "copybase" } else { "copyclip" }),
            egui::Event::Cut => out.push("cutclip"),
            egui::Event::Paste(text) => {
                if wants_text_paste {
                    continue;
                }
                let h = hash(&text);
                if ctx.data(|d| d.get_temp::<u64>(text_id())) != Some(h) && clipboard::load_system_text(&mut app.session, &text) {
                    ctx.data_mut(|d| d.insert_temp(text_id(), h));
                }
                out.push(if shift { "pasteblock" } else { "pasteclip" });
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use cadcraft_engine::{Input, Session};

    use crate::{CadApp, Services};

    #[test]
    fn ctrl_v_pastes_text_in_text_prompt_and_keeps_cad_paste_on_canvas() {
        let mut app = CadApp::new(Session::new(), Services::default());
        app.cmdline("RECTANG 0,0 40,30");
        app.cmdline("EXTRUDE ALL  5");
        let solid_h = app.session.doc().unwrap().model.iter().last().unwrap().handle;

        // 1. In STLOUT path prompt (Accept::TEXT): Event::Paste goes to cmd.buffer, NOT pasteclip.
        app.start("stlout");
        let _ = app.session.input(Input::Pick(vec![solid_h]));
        let _ = app.session.input(Input::Enter);
        assert!(crate::cmdline::wants_text_paste(&app));

        let ctx = egui::Context::default();
        let mut raw = egui::RawInput::default();
        raw.events.push(egui::Event::Paste("/tmp/cadcraft-tests/mi pieza.stl\n".into()));
        let mut out1 = ctx.run_ui(raw, |_ui| {
            let cmds = super::shortcut_commands(&mut app, &ctx);
            assert!(cmds.is_empty(), "text prompt must not trigger pasteclip: {cmds:?}");
            crate::cmdline::keyboard(&mut app, &ctx);
        });
        out1.textures_delta.clear();
        assert_eq!(app.cmd.buffer, "/tmp/cadcraft-tests/mi pieza.stl");
        assert!(app.session.prompting(), "pasting a path must not auto-submit");

        // Cancel STLOUT and clear buffer -> back on canvas.
        app.session.cancel();
        app.cmd.buffer.clear();
        assert!(!crate::cmdline::wants_text_paste(&app));

        // 2. Idle on canvas: Event::Paste triggers pasteclip and does NOT insert into cmd.buffer.
        let mut raw2 = egui::RawInput::default();
        raw2.events.push(egui::Event::Paste("not-dxf".into()));
        let mut out2 = ctx.run_ui(raw2, |_ui| {
            let cmds = super::shortcut_commands(&mut app, &ctx);
            assert_eq!(cmds, ["pasteclip"]);
            crate::cmdline::keyboard(&mut app, &ctx);
        });
        out2.textures_delta.clear();
        assert!(app.cmd.buffer.is_empty());
    }

    #[test]
    fn ctrl_shift_c_invokes_copybase_and_ctrl_c_invokes_copyclip() {
        let mut app = CadApp::new(Session::new(), Services::default());
        let ctx = egui::Context::default();

        // 1. Shift+Copy event -> "copybase"
        let raw_shift = egui::RawInput {
            events: vec![
                egui::Event::ModifiersChanged(egui::Modifiers { shift: true, ..Default::default() }),
                egui::Event::Copy,
            ],
            ..Default::default()
        };
        let mut out1 = ctx.run_ui(raw_shift, |_ui| {
            let cmds = super::shortcut_commands(&mut app, &ctx);
            assert_eq!(cmds, ["copybase"]);
        });
        out1.textures_delta.clear();

        // 2. Regular Copy event -> "copyclip"
        let raw_plain = egui::RawInput {
            events: vec![
                egui::Event::ModifiersChanged(egui::Modifiers { shift: false, ..Default::default() }),
                egui::Event::Copy,
            ],
            ..Default::default()
        };
        let mut out2 = ctx.run_ui(raw_plain, |_ui| {
            let cmds = super::shortcut_commands(&mut app, &ctx);
            assert_eq!(cmds, ["copyclip"]);
        });
        out2.textures_delta.clear();
    }
}
