//! View menu: ZOOM (with Realtime and Dynamic), PAN, REGEN, REDRAW, named views (VIEW), layout
//! switching.

use cadcraft_doc::{Space, entity_bounds};
use cadcraft_geom::{Bounds2, Vec2};
use serde_json::{Value, json};

use super::machines::{SelectRun, number};
use super::*;
use crate::{Accept, Input, Interactive, Prompt, Result, Session, Step, View};

mod named_views;
mod vports;

pub(super) use vports::{config_names, config_tiles, model_vports, tile_model};

pub fn specs() -> Vec<CommandSpec> {
    let mut v = vec![
        CommandSpec::new("zoom", "Zoom", run_zoom)
            .alias(&["z"])
            .params("{mode: extents|all|window|previous|in|out|center|dynamic|scale|realtime|object, p1?, p2?, center?, height?, factor?, xp?: bool (scale relative to paper space: in a viewport, `factor` paper units per model unit), limits?: bool (scale relative to the limits)} (center/dynamic: `center` + `height`, or `factor` with `xp`/`limits`; realtime: `factor` about the view centre; typed: Enter at the first prompt starts Realtime)")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(ZoomM::default()))),
        CommandSpec::new("zoom.realtime", "Zoom Realtime", |s, p| zoom(s, &with_mode(p.clone(), "realtime", None)))
            .menu(&["View", "Zoom", "Realtime"])
            .params("{factor} (> 1 zooms in, about the view centre); interactive: drag up to zoom in, down to zoom out, Esc or Enter ends")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(ZoomM { start: ZoomStart::Realtime, ..Default::default() }))),
        CommandSpec::new("zoom.dynamic", "Zoom Dynamic", |s, p| zoom(s, &with_mode(p.clone(), "dynamic", None)))
            .menu(&["View", "Zoom", "Dynamic"])
            .params("{center, height}; interactive: a view box over the drawing extents, a point places it, the next sizes it, Enter accepts")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(ZoomM { start: ZoomStart::Dynamic, ..Default::default() }))),
        CommandSpec::new("zoom.previous", "Zoom Previous", |s, _| zoom(s, &json!({"mode":"previous"})))
            .menu(&["View", "Zoom", "Previous"])
            .noundo()
            .transparent(),
        CommandSpec::new("zoom.window", "Zoom Window", run_zoom)
            .menu(&["View", "Zoom", "Window"])
            .params("{p1, p2}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(ZoomM { window: true, ..Default::default() }))),
        CommandSpec::new("zoom.in", "Zoom In", |s, _| zoom(s, &json!({"mode":"in"}))).menu(&["View", "Zoom", "In"]).noundo().transparent(),
        CommandSpec::new("zoom.out", "Zoom Out", |s, _| zoom(s, &json!({"mode":"out"}))).menu(&["View", "Zoom", "Out"]).noundo().transparent(),
        CommandSpec::new("zoom.all", "Zoom All", |s, _| zoom(s, &json!({"mode":"all"}))).menu(&["View", "Zoom", "All"]).noundo().transparent(),
        CommandSpec::new("zoom.extents", "Zoom Extents", |s, _| zoom(s, &json!({"mode":"extents"})))
            .menu(&["View", "Zoom", "Extents"])
            .noundo()
            .transparent(),
        CommandSpec::new("zoom.object", "Zoom Object", |s, p| {
            zoom(s, &json!({"mode":"object", "handles": p.get("handles").cloned().unwrap_or(Value::Null)}))
        })
        .menu(&["View", "Zoom", "Object"])
        .params("{handles?}")
        .noundo()
        .transparent()
        .interactive(|_| Ok(Box::new(SelectRun::new("zoom.object", "ZOOM")))),
        CommandSpec::new("pan", "Pan", run_pan)
            .menu(&["View", "Pan", "Realtime"])
            .alias(&["p"])
            .params("{delta: [dx,dy]} | {from, to} | {center}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(PanM::default()))),
        CommandSpec::new("pan.left", "Pan Left", |s, _| pan_frac(s, Vec2::new(-0.25, 0.0))).menu(&["View", "Pan", "Left"]).noundo().transparent(),
        CommandSpec::new("pan.right", "Pan Right", |s, _| pan_frac(s, Vec2::new(0.25, 0.0))).menu(&["View", "Pan", "Right"]).noundo().transparent(),
        CommandSpec::new("pan.up", "Pan Up", |s, _| pan_frac(s, Vec2::new(0.0, 0.25))).menu(&["View", "Pan", "Up"]).noundo().transparent(),
        CommandSpec::new("pan.down", "Pan Down", |s, _| pan_frac(s, Vec2::new(0.0, -0.25))).menu(&["View", "Pan", "Down"]).noundo().transparent(),
        CommandSpec::new("regen", "Regen", |s, _| {
            // Table formulas are recalculated (as on UPDATEFIELD).
            super::table::recalc(s, None)?;
            s.touch();
            Ok(json!({"message": "Regenerating model."}))
        })
        .menu(&["View", "Regen"])
        .alias(&["re"])
        .noundo(),
        CommandSpec::new("regenall", "Regen All", |s, _| {
            s.touch();
            Ok(json!({"message": "Regenerating all viewports."}))
        })
        .menu(&["View", "Regen All"])
        .alias(&["rea"])
        .noundo(),
        CommandSpec::new("redraw", "Redraw", |s, _| {
            s.touch();
            ok()
        })
        .menu(&["View", "Redraw"])
        .alias(&["r"])
        .noundo()
        .transparent(),
        CommandSpec::new("view.set", "Set View", run_view_set).params("{center: [x,y], height}").noundo(),
        CommandSpec::new("view.get", "Get View", |s, _| {
            let v = s.state()?.view();
            Ok(json!({"center": [v.center.x, v.center.y], "height": v.height, "viewportPx": [s.viewport_px.0, s.viewport_px.1]}))
        })
        .noundo(),
        CommandSpec::new("layout.set", "Switch Layout", run_layout_set).params("{name: \"Model\" | layout name}").noundo(),
        CommandSpec::new("plan", "Plan View", run_plan)
            .menu(&["View", "3D Views", "Plan View", "Current UCS"])
            .alias(&["planview"])
            .params("{option?: current|world|ucs, name?: string} (zooms to extents of current/world/named UCS)")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(PlanM::default()))),
        CommandSpec::new("plan.world", "World UCS", |s, _| run_plan(s, &json!({"option":"world"})))
            .menu(&["View", "3D Views", "Plan View", "World UCS"])
            .noundo()
            .transparent(),
        CommandSpec::new("plan.current", "Current UCS", |s, _| run_plan(s, &json!({"option":"current"})))
            .menu(&["View", "3D Views", "Plan View", "Current UCS"])
            .noundo()
            .transparent(),
        CommandSpec::new("vpoint", "Viewpoint", run_vpoint)
            .menu(&["View", "3D Views", "Viewpoint"])
            .alias(&["viewpoint"])
            .params("{direction?: [x,y,z]}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(VpointM))),
        CommandSpec::new("view.top", "Top", |s, _| run_plan(s, &json!({"option":"world"})))
            .menu(&["View", "3D Views", "Top"])
            .noundo()
            .transparent(),
        CommandSpec::new("view.bottom", "Bottom", |s, _| run_view_preset(s, "bottom"))
            .menu(&["View", "3D Views", "Bottom"])
            .noundo()
            .transparent(),
        CommandSpec::new("view.left", "Left", |s, _| run_view_preset(s, "left"))
            .menu(&["View", "3D Views", "Left"])
            .noundo()
            .transparent(),
        CommandSpec::new("view.right", "Right", |s, _| run_view_preset(s, "right"))
            .menu(&["View", "3D Views", "Right"])
            .noundo()
            .transparent(),
        CommandSpec::new("view.front", "Front", |s, _| run_view_preset(s, "front"))
            .menu(&["View", "3D Views", "Front"])
            .noundo()
            .transparent(),
        CommandSpec::new("view.back", "Back", |s, _| run_view_preset(s, "back"))
            .menu(&["View", "3D Views", "Back"])
            .noundo()
            .transparent(),
        CommandSpec::new("view.swiso", "SW Isometric", |s, _| run_view_preset(s, "swiso"))
            .menu(&["View", "3D Views", "SW Isometric"])
            .noundo()
            .transparent(),
        CommandSpec::new("view.seiso", "SE Isometric", |s, _| run_view_preset(s, "seiso"))
            .menu(&["View", "3D Views", "SE Isometric"])
            .noundo()
            .transparent(),
        CommandSpec::new("view.neiso", "NE Isometric", |s, _| run_view_preset(s, "neiso"))
            .menu(&["View", "3D Views", "NE Isometric"])
            .noundo()
            .transparent(),
        CommandSpec::new("view.nwiso", "NW Isometric", |s, _| run_view_preset(s, "nwiso"))
            .menu(&["View", "3D Views", "NW Isometric"])
            .noundo()
            .transparent(),
        CommandSpec::new("3dorbit", "Orbit", run_3dorbit)
            .menu(&["View", "3D Views", "3D Orbit"])
            .alias(&["3do", "orbit"])
            .params("{direction?: [x,y,z], azimuth?: deg, elevation?: deg}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(OrbitM))),
        CommandSpec::new("vscurrent", "Visual Style", run_vscurrent)
            .menu(&["View", "Visual Styles"])
            .alias(&["vs"])
            .params("{style: 2dwireframe|wireframe|hidden|realistic|conceptual|shaded|shaded_with_edges|shades_of_gray|sketchy|xray}")
            .noundo()
            .transparent()
            .interactive(|_| Ok(Box::new(VscurrentM))),
    ];
    v.extend(named_views::specs());
    v.extend(vports::specs());
    v
}

fn ext_bounds(s: &Session) -> Result<Bounds2> {
    let space = s.space();
    Ok(s.doc()?.extents(&space))
}

fn fit(s: &mut Session, b: Bounds2) -> Result<()> {
    if b.is_empty() {
        return Ok(());
    }
    let (w, h) = s.viewport_px;
    let st = s.state_mut()?;
    let aspect = st.zoom_aspect(w / h.max(1.0));
    let height = b.height().max(b.width() / aspect.max(1e-6)).max(1e-9);
    st.set_zoom_frame(View { center: b.center(), height });
    st.set_ref_height(height);
    Ok(())
}

pub(crate) fn zoom(s: &mut Session, p: &Value) -> Result<Value> {
    let mode = str_param(p, "mode").unwrap_or("extents").to_ascii_lowercase();
    match mode.as_str() {
        "extents" | "e" => s.zoom_extents()?,
        "all" | "a" => {
            let b = ext_bounds(s)?.union(&s.zoom_limits()?);
            fit(s, b.expand(b.height().max(b.width()) * 0.02))?;
        }
        "window" | "w" => {
            let a = point_req("zoom", p, "p1")?;
            let b = point_req("zoom", p, "p2")?;
            fit(s, Bounds2::new(a, b))?;
        }
        "previous" | "p" => {
            let st = s.state_mut()?;
            if let Some(v) = st.view_history.pop() {
                st.set_view_quiet(v);
            } else {
                return Ok(json!({"message": "No previous view saved."}));
            }
        }
        "in" | "out" => {
            let f = if mode == "in" { 2.0 } else { 0.5 };
            let st = s.state_mut()?;
            let v = st.zoom_frame();
            st.set_zoom_frame(View { center: v.center, height: v.height / f });
        }
        "center" | "c" | "dynamic" | "d" => {
            let c = point_req("zoom", p, "center")?;
            let cur = s.state()?.zoom_frame().height;
            let h = if p.get("factor").is_some() { factor_height(s, p, cur)? } else { f64_or(p, "height", cur) };
            s.state_mut()?.set_zoom_frame(View { center: c, height: h.max(1e-9) });
        }
        "scale" | "s" => {
            let v = s.state()?.zoom_frame();
            let height = factor_height(s, p, v.height)?;
            s.state_mut()?.set_zoom_frame(View { center: v.center, height });
        }
        "realtime" => {
            let v = s.state()?.zoom_frame();
            let f = f64_req("zoom", p, "factor")?;
            if f <= 0.0 {
                return Err(bad("zoom", "factor must be positive"));
            }
            s.state_mut()?.set_zoom_frame(View { center: v.center, height: (v.height / f).clamp(1e-9, 1e12) });
        }
        "object" | "o" => {
            let hs = targets(s, p)?;
            let d = s.doc()?;
            let b = hs.iter().filter_map(|h| d.entity(*h)).fold(Bounds2::EMPTY, |a, e| a.union(&entity_bounds(d, e, 0)));
            fit(s, b.expand(b.height().max(b.width()) * 0.05))?;
        }
        _ => return Err(bad("zoom", "unknown mode")),
    }
    // No `touch`: a zoom changes the view, not the drawing. The canvas follows the view by
    // itself and regenerates in the background (AutoCAD redraws instead of regenerating);
    // zooming an unlocked viewport changes the drawing, which shows on its own.
    Ok(Value::Null)
}

/// The view height for a zoom `factor`: relative to the current height `cur` (nX), to paper space
/// with `xp` (nXP; in a viewport `factor` paper units per model unit, elsewhere like nX), or to
/// the limits with `limits` (a plain n: 1 fits the limits).
fn factor_height(s: &Session, p: &Value, cur: f64) -> Result<f64> {
    let f = f64_req("zoom", p, "factor")?;
    if f <= 0.0 {
        return Err(bad("zoom", "factor must be positive"));
    }
    let st = s.state()?;
    if bool_or(p, "xp", false)
        && let Some(vp) = st.zoom_viewport()
    {
        return Ok(vp.height / f);
    }
    if bool_or(p, "limits", false) {
        let lim = s.zoom_limits()?;
        let (w, h) = s.viewport_px;
        let aspect = st.zoom_aspect(w / h.max(1.0)).max(1e-6);
        return Ok(lim.height().max(lim.width() / aspect).max(1e-9) / f);
    }
    Ok(cur / f)
}

/// A typed zoom factor: `n` (relative to the limits), `nX` (to the current view) or `nXP` (to
/// paper space), as `zoom` scale parameters.
fn typed_factor(t: &str) -> Option<Value> {
    let tl = t.trim().to_ascii_lowercase();
    let (num, rel) = if let Some(n) = tl.strip_suffix("xp") {
        (n, "xp")
    } else if let Some(n) = tl.strip_suffix('x') {
        (n, "x")
    } else {
        (tl.as_str(), "limits")
    };
    let f = number(num.trim()).filter(|f| *f > 0.0)?;
    Some(json!({ "factor": f, "xp": rel == "xp", "limits": rel == "limits" }))
}

fn with_mode(mut p: Value, mode: &str, center: Option<Vec2>) -> Value {
    if let Some(o) = p.as_object_mut() {
        o.insert("mode".into(), json!(mode));
        if let Some(c) = center {
            o.insert("center".into(), json!([c.x, c.y]));
        }
    }
    p
}

fn run_zoom(s: &mut Session, p: &Value) -> Result<Value> {
    zoom(s, if p.is_null() { &Value::Null } else { p })
}

fn pan_frac(s: &mut Session, f: Vec2) -> Result<Value> {
    let (w, h) = s.viewport_px;
    let st = s.state_mut()?;
    let v = st.view();
    let ww = v.height * w / h.max(1.0);
    st.set_view(View { center: v.center + Vec2::new(f.x * ww, f.y * v.height), height: v.height });
    ok()
}

fn run_pan(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.state_mut()?;
    let v = st.view();
    let c = if let Some(c) = point_param(p, "center") {
        c
    } else if let Some(d) = point_param(p, "delta") {
        v.center - d
    } else {
        let a = point_req("pan", p, "from")?;
        let b = point_req("pan", p, "to")?;
        v.center - (b - a)
    };
    st.set_view(View { center: c, height: v.height });
    ok()
}

fn run_view_set(s: &mut Session, p: &Value) -> Result<Value> {
    let c = point_req("view.set", p, "center")?;
    let h = f64_req("view.set", p, "height")?;
    if h <= 0.0 {
        return Err(bad("view.set", "height must be positive"));
    }
    if !s.state_mut()?.set_view(View { center: c, height: h }) {
        return Err(bad("view.set", "the view is too large to show. Nothing was changed"));
    }
    ok()
}

fn run_layout_set(s: &mut Session, p: &Value) -> Result<Value> {
    let name = str_param(p, "name").unwrap_or("Model");
    let space = if name.eq_ignore_ascii_case("model") {
        Space::Model
    } else {
        let l = s.doc()?.layout(name).ok_or_else(|| bad("layout.set", format!("no layout `{name}`")))?;
        Space::Paper(l.name.clone())
    };
    s.pending_window = None;
    let st = s.state_mut()?;
    st.space = space.clone();
    st.mspace = None;
    st.selection.clear();
    if let Space::Paper(n) = &space
        && !st.views.iter().any(|(sp, _)| sp == &space)
    {
        // Default paper view: the sheet.
        let page = st.doc.layout(n).map(|l| l.page.clone()).unwrap_or_default();
        let (w, h) = if page.landscape { (page.height_mm, page.width_mm) } else { (page.width_mm, page.height_mm) };
        let unit = if st.doc.header.i64("MEASUREMENT", 0) == 1 { 1.0 } else { 1.0 / 25.4 };
        st.set_view_quiet(View { center: Vec2::new(w * unit / 2.0, h * unit / 2.0), height: h * unit * 1.15 });
    } else if space == Space::Model && st.camera.is_plan() {
        let v = st.view();
        st.camera.target = cadcraft_geom::Vec3::new(v.center.x, v.center.y, st.camera.target.z);
        st.camera.height = v.height;
    }
    s.touch();
    ok()
}

#[derive(Default)]
struct ZoomM {
    window: bool,
    first: Option<Vec2>,
    stage: ZoomStage,
    /// Where the command starts (View > Zoom > Realtime / Dynamic skip the first prompt).
    start: ZoomStart,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum ZoomStart {
    #[default]
    Prompt,
    Realtime,
    Dynamic,
}

/// The Center, Scale, Realtime and Dynamic options' own prompts.
#[derive(Clone, Copy, Default)]
enum ZoomStage {
    #[default]
    Main,
    Center,
    CenterHeight(Vec2),
    Scale,
    Realtime,
    Dynamic(DynBox),
}

/// The prompt of Zoom Realtime; [`Session::realtime_zoom_active`] recognises the mode by it.
const REALTIME_PROMPT: &str = "Drag up to zoom in or down to zoom out; press Esc or Enter to exit";

/// Zoom Dynamic: the view box shown over the drawing extents. A point places the box, the next
/// one sizes it (its left edge stays), and so on; Enter shows what the box holds.
#[derive(Clone, Copy)]
struct DynBox {
    /// The screen view and the zoom frame before the overview.
    screen: View,
    frame: View,
    /// The drawing extents (the limits when empty).
    extents: Bounds2,
    center: Vec2,
    height: f64,
    /// Width / height of the area being zoomed.
    aspect: f64,
    /// The next point sizes the box (else it places it).
    sizing: bool,
}

impl DynBox {
    fn rect(c: Vec2, h: f64, aspect: f64) -> Bounds2 {
        let half = Vec2::new(h * aspect / 2.0, h / 2.0);
        Bounds2::new(c - half, c + half)
    }
    /// The box a point `p` makes: centred on it, or (sizing) reaching it from the left edge.
    fn moved(&self, p: Vec2) -> (Vec2, f64) {
        if !self.sizing {
            return (p, self.height);
        }
        let left = self.center.x - self.height * self.aspect / 2.0;
        let w = (p.x - left).max(self.height * self.aspect * 1e-3).max(1e-9);
        (Vec2::new(left + w / 2.0, self.center.y), w / self.aspect)
    }
}

impl ZoomM {
    /// Start Realtime: one view history entry for the whole drag session (Zoom Previous).
    fn realtime(&mut self, s: &mut Session) -> Result<Step> {
        let st = s.state_mut()?;
        let v = st.view();
        st.view_history.push(v);
        if st.view_history.len() > 50 {
            st.view_history.remove(0);
        }
        self.stage = ZoomStage::Realtime;
        Ok(Step::Continue)
    }
    /// Start Dynamic: zoom out to show the extents and the current view, and put the view box
    /// where the view was. (Esc leaves the overview; Zoom Previous returns.)
    fn dynamic(&mut self, s: &mut Session) -> Result<Step> {
        let (w, h) = s.viewport_px;
        let st = s.state()?;
        let (screen, frame) = (st.view(), st.zoom_frame());
        let aspect = st.zoom_aspect(w / h.max(1.0)).max(1e-6);
        let ext = ext_bounds(s)?;
        let extents = if ext.is_empty() { s.zoom_limits()? } else { ext };
        let all = extents.union(&DynBox::rect(frame.center, frame.height, aspect));
        fit(s, all.expand(all.height().max(all.width()) * 0.05))?;
        self.stage = ZoomStage::Dynamic(DynBox { screen, frame, extents, center: frame.center, height: frame.height, aspect, sizing: false });
        Ok(Step::Continue)
    }
}

impl Interactive for ZoomM {
    fn name(&self) -> &'static str {
        "ZOOM"
    }
    fn begin(&mut self, s: &mut Session) -> Result<Step> {
        match self.start {
            ZoomStart::Prompt => Ok(Step::Continue),
            ZoomStart::Realtime => self.realtime(s),
            ZoomStart::Dynamic => self.dynamic(s),
        }
    }
    fn prompt(&self, s: &Session) -> Prompt {
        match self.stage {
            ZoomStage::Main => {}
            ZoomStage::Realtime => return Prompt::new(REALTIME_PROMPT, Accept { enter: true, ..Accept::default() }),
            ZoomStage::Dynamic(b) if b.sizing => {
                let left = Vec2::new(b.center.x - b.height * b.aspect / 2.0, b.center.y);
                return Prompt::new("Specify size of view box", Accept::POINT).default("accept").base(left);
            }
            ZoomStage::Dynamic(_) => return Prompt::new("Specify center of view box", Accept::POINT).default("accept"),
            ZoomStage::Center => return Prompt::new("Specify center point", Accept::POINT),
            ZoomStage::CenterHeight(_) => {
                let h = s.state().map(|st| st.zoom_frame().height).unwrap_or(1.0);
                return Prompt::new("Enter magnification or height", Accept::NUMBER).default(format!("{h:.4}"));
            }
            ZoomStage::Scale => return Prompt::new("Enter a scale factor (nX or nXP)", Accept::NUMBER),
        }
        match (self.window, self.first) {
            (false, None) => Prompt::new("Specify corner of window, enter a scale factor (nX or nXP)", Accept::POINT_OR_NUMBER)
                .kw(&["All", "Center", "Dynamic", "Extents", "Previous", "Scale", "Window", "Object"])
                .default("real time"),
            (true, None) => Prompt::new("Specify first corner", Accept::POINT),
            (_, Some(a)) => Prompt::new("Specify opposite corner", Accept::POINT).corner(a),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        let bad_factor = || crate::EngineError::Other("Requires a scale factor (n, nX or nXP).".into());
        match (self.stage, &i) {
            (_, Input::Cancel) | (ZoomStage::Main, _) => {}
            (ZoomStage::Realtime, Input::Enter) => return Ok(Step::Done),
            (ZoomStage::Realtime, _) => return Ok(Step::Continue),
            (ZoomStage::Dynamic(b), Input::Enter) => {
                // Back to the view before the overview without a history entry, then zoom from it.
                let st = s.state_mut()?;
                st.set_view_quiet(b.screen);
                if st.view_history.last() == Some(&b.screen) {
                    st.view_history.pop();
                }
                st.set_zoom_frame(View { center: b.center, height: b.height });
                st.set_ref_height(b.height);
                return Ok(Step::Done);
            }
            (ZoomStage::Dynamic(mut b), Input::Point(p)) => {
                (b.center, b.height) = b.moved(*p);
                b.sizing = !b.sizing;
                self.stage = ZoomStage::Dynamic(b);
                return Ok(Step::Continue);
            }
            (ZoomStage::Dynamic(_), _) => return Err(crate::EngineError::Other("Requires a point, or Enter to accept the view.".into())),
            (ZoomStage::Center, Input::Point(c)) => {
                self.stage = ZoomStage::CenterHeight(*c);
                return Ok(Step::Continue);
            }
            (ZoomStage::Center, Input::Enter) => return Ok(Step::Done),
            (ZoomStage::Center, _) => return Err(crate::EngineError::Other("Requires a point.".into())),
            (ZoomStage::CenterHeight(c), Input::Enter) => {
                zoom(s, &json!({ "mode": "center", "center": [c.x, c.y] }))?;
                return Ok(Step::Done);
            }
            (ZoomStage::CenterHeight(c), Input::Text(t)) => {
                let tl = t.trim().to_ascii_lowercase();
                // A plain number is the new view height; nX / nXP a magnification.
                let p = if tl.ends_with('x') || tl.ends_with("xp") {
                    typed_factor(&tl).ok_or_else(bad_factor)?
                } else {
                    json!({ "height": number(&tl).filter(|h| *h > 0.0).ok_or_else(bad_factor)? })
                };
                zoom(s, &with_mode(p, "center", Some(c)))?;
                return Ok(Step::Done);
            }
            (ZoomStage::Scale, Input::Text(t)) => {
                zoom(s, &with_mode(typed_factor(t).ok_or_else(bad_factor)?, "scale", None))?;
                return Ok(Step::Done);
            }
            (ZoomStage::Scale, Input::Enter) => return Ok(Step::Done),
            (_, _) => return Err(bad_factor()),
        }
        match i {
            Input::Keyword(k) => {
                let mode = match k.as_str() {
                    "All" => "all",
                    "Extents" => "extents",
                    "Previous" => "previous",
                    "Dynamic" => return self.dynamic(s),
                    "Window" => {
                        self.window = true;
                        return Ok(Step::Continue);
                    }
                    "Center" => {
                        self.stage = ZoomStage::Center;
                        return Ok(Step::Continue);
                    }
                    "Scale" => {
                        self.stage = ZoomStage::Scale;
                        return Ok(Step::Continue);
                    }
                    "Object" => "object",
                    _ => return Err(crate::EngineError::Other("Invalid option keyword.".into())),
                };
                zoom(s, &json!({ "mode": mode }))?;
                Ok(Step::Done)
            }
            Input::Point(p) => match self.first {
                None => {
                    self.first = Some(p);
                    Ok(Step::Continue)
                }
                Some(a) => {
                    fit(s, Bounds2::new(a, p))?;
                    Ok(Step::Done)
                }
            },
            Input::Text(t) => {
                // nX: relative to the current view; nXP: to paper space; n: to the limits.
                let p = typed_factor(&t).ok_or_else(|| crate::EngineError::Other("Requires a point, scale factor or option keyword.".into()))?;
                zoom(s, &with_mode(p, "scale", None))?;
                Ok(Step::Done)
            }
            // Enter at the first prompt: Realtime (the default), as in AutoCAD.
            Input::Enter if !self.window && self.first.is_none() => self.realtime(s),
            Input::Enter => Ok(Step::Done),
            _ => Ok(Step::Continue),
        }
    }
    fn preview(&self, _s: &Session, c: Vec2) -> Vec<cadcraft_doc::EntityKind> {
        use super::helpers::{line, lwpoly, rect_vertices};
        let rect = |r: Bounds2| lwpoly(rect_vertices(r.min, r.max), true);
        if let ZoomStage::Dynamic(b) = self.stage {
            // The extents, the view before, and the box: where it is and where the cursor puts it.
            let (center, height) = b.moved(c);
            let next = DynBox::rect(center, height, b.aspect);
            let mut out = vec![
                rect(b.extents),
                rect(DynBox::rect(b.frame.center, b.frame.height, b.aspect)),
                rect(DynBox::rect(b.center, b.height, b.aspect)),
                rect(next),
            ];
            if !b.sizing {
                out.push(line(next.min, next.max));
                out.push(line(Vec2::new(next.min.x, next.max.y), Vec2::new(next.max.x, next.min.y)));
            }
            return out;
        }
        self.first.map(|a| vec![rect(Bounds2::new(a, c))]).unwrap_or_default()
    }
}

impl Session {
    /// ZOOM is in its Realtime mode: the UI turns vertical drags into [`Session::realtime_zoom`].
    pub fn realtime_zoom_active(&self) -> bool {
        self.running.as_ref().is_some_and(|r| r.id.starts_with("zoom")) && self.current_prompt().is_some_and(|p| p.message == REALTIME_PROMPT)
    }
    /// One step of a Realtime zoom drag of `dy` pixels (down is positive): dragging up half the
    /// drawing area's height zooms in 2x, down zooms out, about the centre of the view. Does
    /// nothing outside Realtime.
    pub fn realtime_zoom(&mut self, dy: f64) -> Result<()> {
        if !self.realtime_zoom_active() || !dy.is_finite() {
            return Ok(());
        }
        let half = (self.viewport_px.1 / 2.0).max(1.0);
        let f = 2f64.powf((-dy / half).clamp(-16.0, 16.0));
        let st = self.state_mut()?;
        let v = st.view();
        st.set_view_quiet(View { center: v.center, height: (v.height / f).clamp(1e-9, 1e12) });
        self.touch();
        Ok(())
    }
    /// True while 3DORBIT command is active.
    pub fn orbit_active(&self) -> bool {
        self.running.as_ref().is_some_and(|r| r.id == "3dorbit")
    }
    /// Begin a 3D orbit gesture (Shift+Middle drag or 3DORBIT):
    /// Evaluates the orbit pivot once at the center of the active viewport,
    /// shifting target strictly along the camera view ray v to intersect the active
    /// working plane (UCS origin + plane offset).
    /// If the ray is grazing (|v · n| <= 1e-4) or intersection is non-finite,
    /// the current target is kept without modification.
    /// This shift along v produces zero visual change on screen before rotation.
    pub fn begin_orbit(&mut self) {
        self.orbit_sensitivity = self.orbit_sensitivity_factor();
        if self.layout_space() != cadcraft_doc::Space::Model {
            return;
        }
        let mut cam = self.camera();
        let ucs = self.ucs();
        let offset = self.settings.drawing_plane_offset;
        let plane_origin = ucs.origin + ucs.z_axis * offset;
        let plane_frame = cadcraft_geom::UcsFrame::new(plane_origin, ucs.x_axis, ucs.y_axis, ucs.z_axis);
        let v = cam.view_dir();
        if let Some(p) = plane_frame.unproject_ray(cam.target, v) {
            if p.x.is_finite() && p.y.is_finite() && p.z.is_finite() {
                cam.target = p;
                self.set_camera(cam);
            }
        }
    }
    /// One step of 3D orbit drag of (dx, dy) pixels.
    pub fn orbit_step(&mut self, dx: f64, dy: f64) -> Result<()> {
        if self.layout_space() != cadcraft_doc::Space::Model {
            return Ok(());
        }
        let (w, h) = self.viewport_px;
        let mut cam = self.camera();
        let m = self.orbit_sensitivity;
        // Inverted axes with adaptive zoom sensitivity: dragging right turns view left, dragging down tilts view up
        cam.orbit(-dx * m, -dy * m, (w, h));
        self.set_camera(cam);
        self.touch();
        Ok(())
    }
    /// One step of ViewCube drag of (dx, dy) pixels (uninverted, nominal 1:1 sensitivity).
    pub fn viewcube_orbit_step(&mut self, dx: f64, dy: f64) -> Result<()> {
        if self.layout_space() != cadcraft_doc::Space::Model {
            return Ok(());
        }
        let (w, h) = self.viewport_px;
        let mut cam = self.camera();
        cam.orbit(dx, dy, (w, h));
        self.set_camera(cam);
        self.touch();
        Ok(())
    }
    /// One step of 2D/3D pan drag of (dx, dy) pixels.
    pub fn pan_step(&mut self, dx: f64, dy: f64, scale: f64) -> Result<()> {
        if !scale.is_finite() || scale <= 1e-12 {
            return Ok(());
        }
        let mut cam = self.camera();
        cam.pan(dx, dy, scale);
        self.set_camera(cam);
        self.touch();
        Ok(())
    }
    /// The zoom of a mouse wheel scroll by `delta` points (up zooms in): ZOOMFACTOR scales it
    /// (60, the default, zooms about 18% per wheel step).
    pub fn wheel_zoom_factor(&self, delta: f64) -> f64 {
        if !delta.is_finite() {
            return 1.0;
        }
        let k = f64::from(self.settings.zoomfactor.clamp(3, 100)) / 60.0;
        (delta / 300.0 * k).clamp(-40.0, 40.0).exp()
    }
}

#[derive(Default)]
struct PanM {
    first: Option<Vec2>,
}

impl Interactive for PanM {
    fn name(&self) -> &'static str {
        "PAN"
    }
    fn prompt(&self, _s: &Session) -> Prompt {
        match self.first {
            None => Prompt::new("Specify base point or displacement (drag with the middle mouse button for realtime pan)", Accept::POINT),
            Some(a) => Prompt::new("Specify second point", Accept::POINT).base(a),
        }
    }
    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match (self.first, i) {
            (None, Input::Point(p)) => {
                self.first = Some(p);
                Ok(Step::Continue)
            }
            (Some(a), Input::Point(b)) => {
                run_pan(s, &json!({ "from": [a.x, a.y], "to": [b.x, b.y] }))?;
                Ok(Step::Done)
            }
            (_, Input::Enter) => Ok(Step::Done),
            _ => Ok(Step::Continue),
        }
    }
}

fn run_plan(s: &mut Session, p: &Value) -> Result<Value> {
    let mut cam = s.camera();
    cam.set_preset("top");
    s.set_camera(cam);
    let opt = str_param(p, "option").unwrap_or("current").to_ascii_lowercase();
    match opt.as_str() {
        "world" | "w" => {
            s.zoom_extents()?;
            s.echo("Regenerating model.");
            Ok(json!({ "status": "ok", "view": "world_plan" }))
        }
        "ucs" | "u" => {
            let name = str_param(p, "name");
            if let Some(n) = name {
                s.zoom_extents()?;
                s.echo(format!("Restored plan view of UCS \"{n}\"."));
                Ok(json!({ "status": "ok", "view": "ucs_plan", "ucs": n }))
            } else {
                Err(bad("plan", "`name` of UCS is required"))
            }
        }
        "current" | "c" | "" => {
            s.zoom_extents()?;
            s.echo("Regenerating model.");
            Ok(json!({ "status": "ok", "view": "current_plan" }))
        }
        _ => Err(bad("plan", "option must be Current, World, or Ucs")),
    }
}

fn run_vpoint(s: &mut Session, p: &Value) -> Result<Value> {
    if let Some(arr) = p.get("direction").and_then(Value::as_array)
        && arr.len() == 3
    {
        let x = arr.first().and_then(Value::as_f64).unwrap_or(0.0);
        let y = arr.get(1).and_then(Value::as_f64).unwrap_or(0.0);
        let z = arr.get(2).and_then(Value::as_f64).unwrap_or(1.0);
        let mut cam = s.camera();
        cam.set_view_dir(cadcraft_geom::Vec3::new(x, y, z));
        s.set_camera(cam);
        s.zoom_extents()?;
        s.echo(format!("View direction set to {x:.4},{y:.4},{z:.4}."));
        return Ok(json!({ "status": "ok", "direction": [x, y, z] }));
    }
    let mut cam = s.camera();
    cam.set_view_dir(cadcraft_geom::Vec3::new(0.0, 0.0, 1.0));
    s.set_camera(cam);
    s.zoom_extents()?;
    s.echo("Regenerating model.");
    Ok(json!({ "status": "ok", "direction": [0.0, 0.0, 1.0] }))
}

pub(super) fn run_view_preset(s: &mut Session, name: &str) -> Result<Value> {
    let mut cam = s.camera();
    cam.set_preset(name);
    s.set_camera(cam);
    s.zoom_extents()?;
    s.echo(format!("View orientation set to {name}."));
    Ok(json!({ "status": "ok", "preset": name }))
}

pub(crate) fn normalize_visual_style(style: &str) -> Option<(&'static str, bool)> {
    let s = style.trim().trim_start_matches('_').to_ascii_lowercase();
    match s.as_str() {
        "2dwireframe" | "2d" | "2" | "wireframe" | "w" => Some(("2dWireframe", true)),
        "conceptual" | "c" => Some(("Conceptual", true)),
        "realistic" | "r" | "hidden" | "h" | "shaded" | "s" | "shaded with edges" | "edges" | "e"
        | "shades of gray" | "gray" | "g" | "sketchy" | "k" | "xray" | "x-ray" | "x" => {
            Some(("", false))
        }
        _ => None,
    }
}

fn run_vscurrent(s: &mut Session, p: &Value) -> Result<Value> {
    let raw = str_param(p, "style").unwrap_or("2dWireframe");
    match normalize_visual_style(raw) {
        Some((canonical, true)) => {
            s.settings.vscurrent = canonical.to_string();
            s.echo(format!("Visual style set to {canonical}."));
            s.touch();
            Ok(json!({ "status": "ok", "style": canonical }))
        }
        Some((_, false)) => {
            let name = raw.trim().trim_start_matches('_');
            Err(bad("vscurrent", format!("Visual style '{name}' is not supported yet: axialCAD renderer currently supports 2dWireframe and Conceptual.")))
        }
        None => {
            let name = raw.trim().trim_start_matches('_');
            Err(bad("vscurrent", format!("Unknown visual style '{name}'. Enter an option [2dWireframe/Wireframe/Hidden/Realistic/Conceptual/Shaded/shaded with Edges/shades of Gray/Sketchy/X-ray].")))
        }
    }
}

#[derive(Default)]
struct VscurrentM;

impl Interactive for VscurrentM {
    fn name(&self) -> &'static str {
        "VSCURRENT"
    }

    fn prompt(&self, s: &Session) -> Prompt {
        let mut p = Prompt::new("Enter an option", Accept { enter: true, text: true, ..Accept::default() });
        p.keywords = vec![
            "2dWireframe".into(),
            "Wireframe".into(),
            "Hidden".into(),
            "Realistic".into(),
            "Conceptual".into(),
            "Shaded".into(),
            "shaded with Edges".into(),
            "shades of Gray".into(),
            "Sketchy".into(),
            "X-ray".into(),
        ];
        p.default = Some(s.settings.vscurrent.clone());
        p
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Enter => {
                let current = s.settings.vscurrent.clone();
                s.echo(format!("Visual style set to {current}."));
                Ok(Step::Done)
            }
            Input::Text(t) | Input::Keyword(t) => {
                let trimmed = t.trim();
                if trimmed.is_empty() {
                    let current = s.settings.vscurrent.clone();
                    s.echo(format!("Visual style set to {current}."));
                    return Ok(Step::Done);
                }
                match normalize_visual_style(trimmed) {
                    Some((canonical, true)) => {
                        s.settings.vscurrent = canonical.to_string();
                        s.echo(format!("Visual style set to {canonical}."));
                        s.touch();
                        Ok(Step::Done)
                    }
                    Some((_, false)) => {
                        let name = trimmed.trim_start_matches('_');
                        Err(EngineError::Other(format!("Visual style '{name}' is not supported yet: axialCAD renderer currently supports 2dWireframe and Conceptual.")))
                    }
                    None => Err(EngineError::Other("Invalid option keyword.".into())),
                }
            }
            _ => Err(EngineError::Other("Invalid option keyword.".into())),
        }
    }
}

#[derive(Default)]
enum PlanStage {
    #[default]
    Option,
    UcsName,
}

#[derive(Default)]
struct PlanM {
    stage: PlanStage,
}

impl Interactive for PlanM {
    fn name(&self) -> &'static str {
        "PLAN"
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        match self.stage {
            PlanStage::Option => {
                let mut p =
                    Prompt::new("Enter an option [Current ucs/Ucs/World] <Current>:", Accept { enter: true, text: true, ..Accept::default() });
                p.keywords = vec!["Current".into(), "Ucs".into(), "World".into()];
                p.default = Some("Current".into());
                p
            }
            PlanStage::UcsName => Prompt::new("Enter name of UCS:", Accept { enter: true, text: true, ..Accept::default() }),
        }
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match self.stage {
            PlanStage::Option => match i {
                Input::Enter => {
                    let _ = run_plan(s, &json!({"option": "current"}))?;
                    Ok(Step::Done)
                }
                Input::Keyword(k) | Input::Text(k) => {
                    let kl = k.trim().to_ascii_lowercase();
                    if kl.starts_with('w') {
                        let _ = run_plan(s, &json!({"option": "world"}))?;
                        Ok(Step::Done)
                    } else if kl.starts_with('u') {
                        self.stage = PlanStage::UcsName;
                        Ok(Step::Continue)
                    } else if kl.starts_with('c') || kl.is_empty() {
                        let _ = run_plan(s, &json!({"option": "current"}))?;
                        Ok(Step::Done)
                    } else {
                        s.echo("Invalid option keyword.");
                        Ok(Step::Continue)
                    }
                }
                _ => Ok(Step::Continue),
            },
            PlanStage::UcsName => match i {
                Input::Text(name) => {
                    let _ = run_plan(s, &json!({"option": "ucs", "name": name}))?;
                    Ok(Step::Done)
                }
                Input::Enter => Ok(Step::Done),
                _ => Ok(Step::Continue),
            },
        }
    }
}

#[derive(Default)]
struct VpointM;

impl Interactive for VpointM {
    fn name(&self) -> &'static str {
        "VPOINT"
    }

    fn prompt(&self, s: &Session) -> Prompt {
        let v = s.camera().view_dir();
        Prompt::new(
            format!(
                "Current view direction: VIEWDIR={:.4},{:.4},{:.4}\nSpecify a view point or [Rotate] <display compass and tripod>:",
                v.x, v.y, v.z
            ),
            Accept { enter: true, text: true, ..Accept::default() },
        )
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Enter => {
                s.zoom_extents()?;
                s.echo("Regenerating model.");
                Ok(Step::Done)
            }
            Input::Text(t) => {
                let parts: Vec<&str> = t.split(',').collect();
                if parts.len() == 3 {
                    let x = parts.first().and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(0.0);
                    let y = parts.get(1).and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(0.0);
                    let z = parts.get(2).and_then(|s| s.trim().parse::<f64>().ok()).unwrap_or(1.0);
                    let mut cam = s.camera();
                    cam.set_view_dir(cadcraft_geom::Vec3::new(x, y, z));
                    s.set_camera(cam);
                    s.zoom_extents()?;
                    s.echo(format!("View direction set to {t}."));
                    Ok(Step::Done)
                } else if t.trim().eq_ignore_ascii_case("r") || t.trim().eq_ignore_ascii_case("rotate") {
                    s.echo("Viewpoint rotation: 0 degrees in XY plane from X axis, 90 degrees from XY plane.");
                    Ok(Step::Done)
                } else {
                    s.echo("Point or option keyword required.");
                    Ok(Step::Continue)
                }
            }
            _ => Ok(Step::Continue),
        }
    }
}

pub(crate) fn run_3dorbit(s: &mut Session, p: &Value) -> Result<Value> {
    s.begin_orbit();
    let mut cam = s.camera();
    if let Some(arr) = p.get("direction").and_then(Value::as_array) {
        if arr.len() == 3 {
            let x = arr.first().and_then(Value::as_f64).unwrap_or(0.0);
            let y = arr.get(1).and_then(Value::as_f64).unwrap_or(0.0);
            let z = arr.get(2).and_then(Value::as_f64).unwrap_or(1.0);
            cam.set_view_dir(cadcraft_geom::Vec3::new(x, y, z));
            s.set_camera(cam);
        }
    } else if let (Some(az), Some(el)) = (p.get("azimuth").and_then(Value::as_f64), p.get("elevation").and_then(Value::as_f64)) {
        cam.azimuth = az.to_radians();
        cam.elevation = el.to_radians().clamp(-cadcraft_geom::MAX_ORBIT_ELEVATION, cadcraft_geom::MAX_ORBIT_ELEVATION);
        s.set_camera(cam);
    }
    s.echo("3D Orbit active. Press ESC or ENTER to exit.");
    Ok(json!({ "status": "ok", "azimuth": cam.azimuth.to_degrees(), "elevation": cam.elevation.to_degrees() }))
}

#[derive(Default)]
pub struct OrbitM;

impl Interactive for OrbitM {
    fn name(&self) -> &'static str {
        "3DORBIT"
    }

    fn prompt(&self, _s: &Session) -> Prompt {
        Prompt::new("Press ESC or ENTER to exit, or drag with mouse to orbit view:", Accept { enter: true, text: true, ..Accept::default() })
    }

    fn input(&mut self, s: &mut Session, i: Input) -> Result<Step> {
        match i {
            Input::Enter | Input::Cancel => Ok(Step::Done),
            Input::Text(t) => {
                let tl = t.trim().to_ascii_lowercase();
                if tl == "top" || tl == "plan" {
                    let _ = run_view_preset(s, "top")?;
                    Ok(Step::Done)
                } else if tl == "front"
                    || tl == "back"
                    || tl == "left"
                    || tl == "right"
                    || tl == "swiso"
                    || tl == "seiso"
                    || tl == "neiso"
                    || tl == "nwiso"
                    || tl == "bottom"
                {
                    let _ = run_view_preset(s, &tl)?;
                    Ok(Step::Done)
                } else {
                    Ok(Step::Continue)
                }
            }
            _ => Ok(Step::Continue),
        }
    }
}
