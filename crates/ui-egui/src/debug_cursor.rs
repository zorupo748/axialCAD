//! Diagnostic instrumentation for cursor transition issues.
//! Active only when `AXIALCAD_DEBUG_CURSOR=1` is set in the environment.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use egui::{Context, CursorIcon, Pos2, Rect};

static EVENT_COUNTER: AtomicUsize = AtomicUsize::new(1);

#[derive(Default)]
struct DiagState {
    last_prompt: Option<String>,
    last_inside: Option<bool>,
    last_inside_sig: Option<(bool, bool, bool, bool, bool)>,
    last_on_grip: Option<bool>,
    last_resize_from: Option<Option<(f32, usize)>>,
    last_focus: Option<(bool, bool)>,
    last_vnav: Option<bool>,
    last_local_cursor: Option<(&'static str, CursorIcon)>,
    last_cursor_visible: Option<bool>,
    last_final_cursor: Option<CursorIcon>,
}

static STATE: Mutex<Option<DiagState>> = Mutex::new(None);

#[inline]
pub fn is_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("AXIALCAD_DEBUG_CURSOR").is_some())
}

fn with_state<F: FnOnce(&mut DiagState)>(f: F) {
    if !is_enabled() {
        return;
    }
    if let Ok(mut lock) = STATE.lock() {
        if lock.is_none() {
            *lock = Some(DiagState::default());
        }
        if let Some(state) = lock.as_mut() {
            f(state);
        }
    }
}

fn header(ctx: &Context) -> String {
    let ev = EVENT_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("[cursor_diag f={} p={} #{ev}]", ctx.cumulative_frame_nr(), ctx.cumulative_pass_nr())
}

pub fn trace_prompt(ctx: &Context, prompt: Option<&str>) {
    with_state(|s| {
        let curr = prompt.map(str::to_string);
        if s.last_prompt != curr {
            let hdr = header(ctx);
            eprintln!("{hdr} PROMPT_STATE: changed from {:?} to {:?}", s.last_prompt, curr);
            s.last_prompt = curr;
        }
    });
}

pub fn trace_escape(ctx: &Context, site: &'static str, details: &str) {
    if !is_enabled() {
        return;
    }
    let hdr = header(ctx);
    eprintln!("{hdr} ESCAPE_DETECTED: site={site} details={details}");
}

pub fn trace_inside(
    ctx: &Context,
    inside: bool,
    hover_pos: Option<Pos2>,
    rect: Rect,
    any_popup: bool,
    contains: bool,
    layer_ok: bool,
    hovered: bool,
    viewcube_blocked: bool,
) {
    with_state(|s| {
        let sig = (any_popup, contains, layer_ok, hovered, viewcube_blocked);
        if s.last_inside != Some(inside) || (s.last_inside == Some(false) && s.last_inside_sig != Some(sig)) {
            let hdr = header(ctx);
            let ppp = ctx.pixels_per_point();
            let pos_pts = hover_pos.map(|p| (p.x, p.y));
            let pos_px = hover_pos.map(|p| (p.x * ppp, p.y * ppp));
            eprintln!(
                "{hdr} CANVAS_INSIDE: inside={inside} pos_pts={pos_pts:?} pos_px={pos_px:?} ppp={ppp:.2} rect_pts=[({:.1},{:.1})..({:.1},{:.1})] popup={any_popup} contains={contains} layer_ok={layer_ok} hovered={hovered} viewcube_blocked={viewcube_blocked}",
                rect.min.x, rect.min.y, rect.max.x, rect.max.y
            );
            s.last_inside = Some(inside);
            s.last_inside_sig = Some(sig);
        }
    });
}

pub fn trace_cmdline_grip(
    ctx: &Context,
    bar: Rect,
    lines: Option<Rect>,
    grip: Rect,
    ptr: Option<Pos2>,
    on_grip: bool,
) {
    with_state(|s| {
        if s.last_on_grip != Some(on_grip) {
            let hdr = header(ctx);
            let ppp = ctx.pixels_per_point();
            let ptr_pts = ptr.map(|p| (p.x, p.y));
            let ptr_px = ptr.map(|p| (p.x * ppp, p.y * ppp));
            let lines_h_pts = lines.map(|r| r.height());
            eprintln!(
                "{hdr} CMDLINE_GRIP: on_grip={on_grip} ptr_pts={ptr_pts:?} ptr_px={ptr_px:?} ppp={ppp:.2} bar_pts=[({:.1},{:.1})..({:.1},{:.1})] grip_pts=[({:.1},{:.1})..({:.1},{:.1})] lines_h_pts={lines_h_pts:?}",
                bar.min.x, bar.min.y, bar.max.x, bar.max.y, grip.min.x, grip.min.y, grip.max.x, grip.max.y
            );
            s.last_on_grip = Some(on_grip);
        }
    });
}

pub fn trace_resize_from(ctx: &Context, resize_from: Option<(f32, usize)>) {
    with_state(|s| {
        if s.last_resize_from != Some(resize_from) {
            let hdr = header(ctx);
            eprintln!("{hdr} RESIZE_FROM: changed from {:?} to {:?}", s.last_resize_from.flatten(), resize_from);
            s.last_resize_from = Some(resize_from);
        }
    });
}

pub fn trace_focus(ctx: &Context, wants_kb: bool, focused: bool) {
    with_state(|s| {
        if s.last_focus != Some((wants_kb, focused)) {
            let hdr = header(ctx);
            eprintln!("{hdr} FOCUS_STATE: wants_keyboard={wants_kb} focused={focused}");
            s.last_focus = Some((wants_kb, focused));
        }
    });
}

pub fn trace_vnav_start(ctx: &Context, is_orbit: bool, anchor: Pos2, virt: Pos2) {
    with_state(|s| {
        let hdr = header(ctx);
        let ppp = ctx.pixels_per_point();
        eprintln!(
            "{hdr} VNAV_START: is_orbit={is_orbit} anchor_pts=({:.1},{:.1}) anchor_px=({:.1},{:.1}) virt_pts=({:.1},{:.1}) ppp={ppp:.2}",
            anchor.x, anchor.y, anchor.x * ppp, anchor.y * ppp, virt.x, virt.y
        );
        s.last_vnav = Some(true);
    });
}

pub fn trace_vnav_release(ctx: &Context, reason: &'static str) {
    with_state(|s| {
        if s.last_vnav != Some(false) {
            let hdr = header(ctx);
            eprintln!("{hdr} VNAV_RELEASE: reason={reason}");
            s.last_vnav = Some(false);
        }
    });
}

pub fn trace_local_cursor_req(ctx: &Context, site: &'static str, icon: CursorIcon) {
    with_state(|s| {
        if s.last_local_cursor != Some((site, icon)) {
            let hdr = header(ctx);
            eprintln!("{hdr} LOCAL_CURSOR_REQ: site={site} icon={icon:?}");
            s.last_local_cursor = Some((site, icon));
        }
    });
}

pub fn trace_cursor_visible_req(ctx: &Context, site: &'static str, visible: bool) {
    with_state(|s| {
        if s.last_cursor_visible != Some(visible) {
            let hdr = header(ctx);
            eprintln!("{hdr} CURSOR_VISIBLE_REQ: site={site} visible={visible}");
            s.last_cursor_visible = Some(visible);
        }
    });
}

pub fn trace_frame_end(ctx: &Context) {
    with_state(|s| {
        let final_icon = ctx.output(|o| o.cursor_icon);
        if s.last_final_cursor != Some(final_icon) {
            let hdr = header(ctx);
            eprintln!("{hdr} UI_FINAL_CURSOR: changed from {:?} to {:?}", s.last_final_cursor, final_icon);
            s.last_final_cursor = Some(final_icon);
        }
    });
}
