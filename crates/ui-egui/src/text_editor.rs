//! In-place single-line TEXT editor for TEXTEDIT / DDEDIT.
//!
//! While TEXTEDIT prompts for "Enter new text", an in-place editing field appears over
//! the text entity on screen with its current text, automatically focused.
//! Pressing Enter or clicking OK applies the new text.
//! Pressing Escape or clicking Cancel leaves the text unmodified.

use cadcraft_engine::Input;
use cadcraft_engine::doc::EntityKind;
use egui::{RichText, Stroke, pos2};

use crate::CadApp;
use crate::theme::Tokens;

#[derive(Clone)]
struct State {
    handle: cadcraft_engine::doc::Handle,
    text: String,
}

fn state_id() -> egui::Id {
    egui::Id::new("singleline_text_editor_state")
}

pub fn show(app: &mut CadApp, ctx: &egui::Context) {
    let Some(h) = app.session.editing_text_entity() else {
        ctx.data_mut(|d| d.remove::<State>(state_id()));
        return;
    };
    // Only show if not MText (MText has its own in-place mtext_editor).
    let doc_text = app.session.doc().ok().and_then(|d| {
        let e = d.entity(h)?;
        match &e.kind {
            EntityKind::Text(t) => Some((t.value.clone(), t.justify_point())),
            EntityKind::AttDef(a) => Some((a.text.value.clone(), a.text.justify_point())),
            _ => None,
        }
    });

    let Some((current_val, origin)) = doc_text else {
        ctx.data_mut(|d| d.remove::<State>(state_id()));
        return;
    };

    let asking = app.session.current_prompt().is_some_and(|p| p.accept.text && !p.accept.point);
    if !asking {
        ctx.data_mut(|d| d.remove::<State>(state_id()));
        return;
    }

    let mut st = ctx.data_mut(|d| d.get_temp::<State>(state_id())).unwrap_or_else(|| State {
        handle: h,
        text: current_val.clone(),
    });

    if st.handle != h {
        st = State {
            handle: h,
            text: current_val,
        };
    }

    let t = Tokens::get();
    let rect = ctx.content_rect();
    let xf = app.canvas.xf;
    let screen_pos = xf.map_or(rect.center(), |x| x.to_screen(origin));

    // Clamp inside window bounds
    let box_w = (st.text.chars().count().max(10) as f32 * 11.0).clamp(220.0, (rect.width() - 40.0).max(220.0));
    let pos = pos2(
        screen_pos.x.clamp(rect.left() + 10.0, (rect.right() - box_w - 20.0).max(rect.left() + 10.0)),
        (screen_pos.y - 45.0).clamp(rect.top() + 10.0, (rect.bottom() - 85.0).max(rect.top() + 10.0)),
    );

    let mut apply = false;
    let mut cancel = false;

    egui::Area::new(egui::Id::new("singleline_text_editor_area"))
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(t.panel)
                .stroke(Stroke::new(1.5, t.accent))
                .inner_margin(egui::Margin::same(6))
                .corner_radius(3)
                .show(ui, |ui| {
                    ui.set_width(box_w);
                    ui.vertical(|ui| {
                        let te_id = ui.id().with("inplace_text_edit");
                        let edit = egui::TextEdit::singleline(&mut st.text)
                            .id(te_id)
                            .font(crate::theme::body())
                            .desired_width(box_w);
                        let resp = ui.add(edit);
                        if !resp.has_focus() && !ui.memory(|m| m.has_focus(te_id)) {
                            resp.request_focus();
                        }
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            apply = true;
                        }

                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.button(RichText::new(crate::tl!("OK")).strong()).clicked() {
                                apply = true;
                            }
                            if ui.button(crate::tl!("Cancel")).clicked() {
                                cancel = true;
                            }
                            ui.add_space(4.0);
                            let hint = crate::tl!("Enter to apply • Esc to cancel");
                            ui.label(RichText::new(hint).small().color(t.text_dim));
                        });
                    });
                });
        });

    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        cancel = true;
    } else if ctx.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.any()) {
        apply = true;
    }

    if apply {
        let new_text = std::mem::take(&mut st.text);
        ctx.data_mut(|d| d.remove::<State>(state_id()));
        let _ = app.session.input(Input::Text(new_text));
    } else if cancel {
        ctx.data_mut(|d| d.remove::<State>(state_id()));
        app.session.cancel();
    } else {
        ctx.data_mut(|d| d.insert_temp(state_id(), st));
    }
}
