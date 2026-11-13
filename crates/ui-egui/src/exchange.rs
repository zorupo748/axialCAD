//! EXPORT, IMPORT, PUBLISH and PREVIEW from a menu, the tool bar or the command line: the first
//! three ask for a file, PREVIEW shows the plot in a window. Calls with parameters (scripts, the
//! control channel, MCP) never get here and never open a dialog: they run the engine commands.

use std::sync::atomic::{AtomicU32, Ordering};

use egui::TextureHandle;
use serde_json::{Value, json};

use crate::CadApp;

/// Bumped each time PREVIEW is started, so the window renders the drawing as it is now.
static PREVIEW_GEN: AtomicU32 = AtomicU32::new(0);

/// Handle a typed or menu-invoked file exchange command (`id` may be an alias).
pub fn route(app: &mut CadApp, id: &str) -> Result<Value, String> {
    let title = app.session.state().map(|s| s.title.clone()).unwrap_or_default();
    let stem = std::path::Path::new(&title).file_stem().map(|s| s.to_string_lossy().to_string()).filter(|s| !s.is_empty());
    let stem = stem.unwrap_or_else(|| "Drawing".into());
    match id {
        "preview" | "pre" => {
            PREVIEW_GEN.fetch_add(1, Ordering::Relaxed);
            app.ui.dialog = Some("preview".into());
            Ok(Value::Null)
        }
        "import" | "imp" => {
            let Some(pick) = app.services.pick_open.as_ref() else { return no_picker(app, "IMPORT", r#"import {"path": "parts.dxf"}"#) };
            match pick(None) {
                Some(path) => app.run("import", json!({ "path": path })),
                None => Ok(Value::Null),
            }
        }
        "publish" => {
            let Some(pick) = app.services.pick_save.as_ref() else { return no_picker(app, "PUBLISH", "publish {}") };
            match pick(&format!("{stem}.pdf"), None) {
                Some(path) => app.run("publish", json!({ "path": path })),
                None => Ok(Value::Null),
            }
        }
        _ => {
            let Some(pick) = app.services.pick_save.as_ref() else { return no_picker(app, "EXPORT", r#"export {"format": "svg"}"#) };
            match pick(&format!("{stem}.svg"), None) {
                Some(path) => app.run("export", json!({ "path": path })),
                None => Ok(Value::Null),
            }
        }
    }
}

fn no_picker(app: &mut CadApp, name: &str, example: &str) -> Result<Value, String> {
    let msg = format!("{name}: there is no file dialog here; run it with parameters, e.g. {example}");
    app.session.echo(msg.clone());
    Err(msg)
}

/// Open the host save-file dialog when an interactive prompt requests [`Picker::SaveFile`].
pub(crate) fn resume_save_picker(app: &mut CadApp) {
    if app.session.in_script || app.ui.dialog.is_some() {
        return;
    }
    let Some(cadcraft_engine::prompt::Picker::SaveFile { default_name, extension }) = app.session.current_prompt().and_then(|p| p.picker) else {
        return;
    };
    let Some(pick) = app.services.pick_save.as_ref() else { return };
    match pick(&default_name, None) {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                app.session.cancel();
                return;
            }
            let path = if std::path::Path::new(trimmed).extension().is_none() && !extension.is_empty() {
                format!("{trimmed}.{extension}")
            } else {
                trimmed.to_string()
            };
            if let Err(e) = app.session.input(cadcraft_engine::Input::Text(path)) {
                app.session.echo(e.to_string());
                app.session.cancel();
            }
        }
        None => {
            app.session.cancel();
            app.session.echo("Export cancelled.");
        }
    }
}

#[derive(Clone)]
struct Preview {
    generation: u32,
    shown: Result<(TextureHandle, String), String>,
}

/// A preview the Plot dialog asked for: its PLOT parameters (PNG output) and a caption.
#[derive(Clone)]
struct FromPlot {
    params: Value,
    info: String,
}

const FROM_PLOT: &str = "cc_plot_preview_from_plot";

/// Show the Plot dialog's settings as they plot (PLOT with `params`, a PNG); Print... and Close
/// return to the dialog.
pub(crate) fn preview_plot(app: &mut CadApp, ctx: &egui::Context, params: Value, info: String) {
    PREVIEW_GEN.fetch_add(1, Ordering::Relaxed);
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(FROM_PLOT), FromPlot { params, info }));
    app.ui.dialog = Some("preview".into());
}

/// Render the plot preview of the current layout (or model), or of the Plot dialog's settings,
/// as a texture.
fn render(app: &mut CadApp, ctx: &egui::Context) -> Result<(TextureHandle, String), String> {
    if let Some(f) = ctx.data(|d| d.get_temp::<FromPlot>(egui::Id::new(FROM_PLOT))) {
        let r = app.session.execute("plot", &f.params).map_err(|e| e.to_string())?;
        let png = r["data"].as_str().and_then(cadcraft_engine::cmd::file::base64_decode).ok_or("the preview has no image")?;
        let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png).map_err(|e| e.to_string())?.to_rgba8();
        let size = [img.width() as usize, img.height() as usize];
        let tex = ctx.load_texture("plot-preview", egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), egui::TextureOptions::LINEAR);
        return Ok((tex, f.info));
    }
    let r = app.session.execute("preview", &json!({ "width": 1600 })).map_err(|e| e.to_string())?;
    let png = r["data"].as_str().and_then(cadcraft_engine::cmd::file::base64_decode).ok_or("the preview has no image")?;
    let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png).map_err(|e| e.to_string())?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let tex = ctx.load_texture("plot-preview", egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()), egui::TextureOptions::LINEAR);
    let mut info = format!("{} · {}", r["layout"].as_str().unwrap_or(""), r["paper"].as_str().unwrap_or(""));
    if r["landscape"].as_bool() == Some(true) {
        info.push_str(" · landscape");
    }
    Ok((tex, info))
}

/// The Plot Preview window: the sheet as it plots, with Print and Close.
pub fn dialog(app: &mut CadApp, ctx: &egui::Context, open: &mut bool) {
    let id = egui::Id::new("cc_plot_preview");
    let generation = PREVIEW_GEN.load(Ordering::Relaxed);
    let cached = ctx.data(|d| d.get_temp::<Preview>(id)).filter(|p| p.generation == generation);
    let preview = match cached {
        Some(p) => p,
        None => {
            let p = Preview { generation, shown: render(app, ctx) };
            ctx.data_mut(|d| d.insert_temp(id, p.clone()));
            p
        }
    };
    let (mut close, mut print) = (false, false);
    egui::Window::new("Plot Preview").open(open).collapsible(false).resizable(false).show(ctx, |ui| {
        match &preview.shown {
            Ok((tex, info)) => {
                ui.label(info);
                let size = tex.size_vec2();
                let k = (760.0 / size.x).min(540.0 / size.y).min(1.0);
                ui.image((tex.id(), size * k));
            }
            Err(e) => {
                ui.label(e);
            }
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            print = ui.button("Print...").clicked();
            close = ui.button("Close").clicked();
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    if close || print {
        *open = false;
    }
    let from_plot = ctx.data(|d| d.get_temp::<FromPlot>(egui::Id::new(FROM_PLOT))).is_some();
    if !*open {
        ctx.data_mut(|d| {
            d.remove::<Preview>(id);
            d.remove::<FromPlot>(egui::Id::new(FROM_PLOT));
        });
    }
    if from_plot && !*open {
        // Back to the Plot dialog with its settings; Print... plots them.
        crate::managers::set_flag(ctx, crate::managers::PLOT_NOW, print);
        app.ui.dialog = Some("plot".into());
    } else if print {
        app.start("plot");
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use cadcraft_engine::Session;
    use serde_json::json;

    use crate::{CadApp, Services};

    /// An app whose file dialogs record what they were asked and cancel.
    fn app() -> (CadApp, Rc<RefCell<Vec<String>>>) {
        let asked = Rc::new(RefCell::new(Vec::new()));
        let (a, b) = (asked.clone(), asked.clone());
        let services = Services {
            pick_open: Some(Box::new(move |_dir| {
                a.borrow_mut().push("open".into());
                None
            })),
            pick_save: Some(Box::new(move |name: &str, _dir| {
                b.borrow_mut().push(name.to_string());
                None
            })),
            ..Services::default()
        };
        (CadApp::new(Session::new(), services), asked)
    }

    #[test]
    fn typed_and_menu_commands_ask_json_calls_never_do() {
        let (mut app, asked) = app();
        app.start("export");
        app.cmdline("PUBLISH");
        app.start("imp");
        assert_eq!(*asked.borrow(), ["Drawing1.svg", "Drawing1.pdf", "open"]);
        assert!(app.ui.dialog.is_none());
        app.cmdline("pre");
        assert_eq!(app.ui.dialog.as_deref(), Some("preview"));
        app.ui.dialog = None;
        app.start("preview");
        assert_eq!(app.ui.dialog.as_deref(), Some("preview"));
        app.ui.dialog = None;

        let _ = app.run("export", json!({ "format": "svg" }));
        let _ = app.run("import", json!({}));
        let _ = app.run("publish", json!({ "layouts": [] }));
        let r = app.run("preview", json!({ "width": 64 })).unwrap();
        assert!(r["data"].is_string());
        assert_eq!(asked.borrow().len(), 3, "JSON calls never open a file dialog");
        assert!(app.ui.dialog.is_none(), "nor the preview window");

        let e = app.run("share", json!({})).unwrap_err();
        assert!(e.contains("not available yet"), "{e}");
    }

    #[test]
    fn stlout_save_picker_appends_stl_extension_and_cancels_cleanly() {
        let out_no_ext = std::env::temp_dir().join(format!("cadcraft_stl_picker_test_{}", std::process::id()));
        let out_with_ext = out_no_ext.with_extension("stl");
        let _ = std::fs::remove_file(&out_no_ext);
        let _ = std::fs::remove_file(&out_with_ext);

        let asked = Rc::new(RefCell::new(Vec::new()));
        let asked_c = asked.clone();
        let ret_path = Rc::new(RefCell::new(Some(out_no_ext.to_string_lossy().to_string())));
        let ret_c = ret_path.clone();
        let services = Services {
            pick_save: Some(Box::new(move |name: &str, _dir| {
                asked_c.borrow_mut().push(name.to_string());
                ret_c.borrow().clone()
            })),
            ..Services::default()
        };

        let mut app = CadApp::new(Session::new(), services);
        app.cmdline("RECTANG 0,0 40,30");
        app.cmdline("EXTRUDE ALL  5");
        assert_eq!(app.session.settings.vscurrent, "Conceptual");

        // Cancel in dialog: no file created, command ends cleanly
        *ret_path.borrow_mut() = None;
        app.start("stlout");
        app.cmdline("ALL");
        app.cmdline("");
        super::resume_save_picker(&mut app);
        assert_eq!(*asked.borrow(), ["Drawing1.stl"]);
        assert!(!app.session.prompting(), "cancelling dialog must finish STLOUT cleanly");
        assert!(!out_with_ext.exists());

        // Accept path without extension: appends .stl and writes valid binary STL
        *ret_path.borrow_mut() = Some(out_no_ext.to_string_lossy().to_string());
        app.start("stlout");
        app.cmdline("ALL");
        app.cmdline("");
        super::resume_save_picker(&mut app);
        assert!(!app.session.prompting(), "accepting dialog must finish STLOUT");
        assert!(out_with_ext.exists(), "must append .stl when picker returns name without extension");
        let bytes = std::fs::read(&out_with_ext).unwrap();
        assert_eq!(bytes.len(), 684);
        assert!(bytes[..80].starts_with(b"axialCAD binary STL"));
        let _ = std::fs::remove_file(&out_with_ext);
    }
}
