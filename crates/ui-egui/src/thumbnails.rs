//! Drawing thumbnail cache and rendering for the Start page.
//!
//! Thumbnails are stored outside the repository in the user's cache directory
//! (or temporary folder) keyed by deterministic path hash, ensuring that files
//! with identical names in different directories never collide.

use egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::path::{Path, PathBuf};

/// Deterministic 64-bit FNV-1a hash of a file path.
fn path_hash(path: &str) -> u64 {
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path));
    let s = canonical.to_string_lossy();
    s.bytes().fold(0xcbf29ce484222325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x100000001b3))
}

/// Directory where thumbnail PNGs are stored.
pub fn cache_dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(std::env::temp_dir)
        .join("cadcraft")
        .join("thumbnails")
}

/// The cached thumbnail file path for `path`.
pub fn thumbnail_path(path: &str) -> PathBuf {
    let key = format!("{:016x}.png", path_hash(path));
    cache_dir().join(key)
}

/// Generate and save a 240x160 thumbnail PNG for `drawing` at `path`.
///
/// When `last_view` is provided (the 2D camera view center and height at save time),
/// the thumbnail renders from that exact view. If none is provided or the view is
/// degenerate, it falls back to model extents.
///
/// Architectural note on 3D views:
/// `cadcraft_render` operates as a 2D vector display-list renderer. The saved view
/// reflects the 2D center and scale of the model space. For 3D camera projections,
/// full 3D offscreen software rasterization would require an external 3D engine;
/// we do not present 2D projected extents as a full 3D camera render.
pub fn update_thumbnail(drawing: &cadcraft_doc::Drawing, last_view: Option<cadcraft_engine::View>, path: &str) {
    if path.is_empty() {
        return;
    }
    let ext = drawing.extents(&cadcraft_doc::Space::Model);
    let opts = cadcraft_render::Options { min_dash: 0.0, ..cadcraft_render::Options::default() };
    let dl = cadcraft_render::build(drawing, &cadcraft_doc::Space::Model, &opts);
    let view = if let Some(v) = last_view
        && v.center.is_finite()
        && v.height.is_finite()
        && v.height > 1e-6
    {
        let scale = 160.0 / v.height;
        cadcraft_render::raster::View { center: v.center, scale, width: 240, height: 160 }
    } else {
        cadcraft_render::raster::View::fit(&ext, 240, 160, 0.05)
    };
    let ropts = cadcraft_render::raster::RasterOptions { background: cadcraft_color::Rgb::from_u32(0x22262e), ..Default::default() };
    if let Some(png_bytes) = cadcraft_render::raster::render_png(&dl, &view, &ropts) {
        let target = thumbnail_path(path);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(target, png_bytes);
        prune_disk_cache(100);
    }
}

/// Prune thumbnail disk cache if it exceeds `max_files`.
pub fn prune_disk_cache(max_files: usize) {
    let dir = cache_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("png") {
            let mtime = entry.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            files.push((mtime, path));
        }
    }
    if files.len() > max_files {
        files.sort_by_key(|(mtime, _)| *mtime);
        let remove_count = files.len() - max_files;
        for (_, p) in files.iter().take(remove_count) {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Load a cached thumbnail texture for `path`, if it exists on disk.
/// Corrupt, missing or invalid images safely return None.
pub fn load_thumbnail_texture(ctx: &Context, path: &str) -> Option<TextureHandle> {
    let file = thumbnail_path(path);
    if !file.exists() {
        return None;
    }
    let bytes = std::fs::read(&file).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let pixels = img.into_raw();
    let color_image = ColorImage::from_rgba_unmultiplied(size, &pixels);
    Some(ctx.load_texture(format!("thumb_{:016x}", path_hash(path)), color_image, TextureOptions::LINEAR))
}

/// Check if a drawing file exists on disk.
pub fn file_exists(path: &str) -> bool {
    Path::new(path).exists()
}

/// Formatted last-modified timestamp of a file using chrono.
pub fn file_modified_time(path: &str) -> String {
    let Ok(meta) = std::fs::metadata(path) else { return String::new() };
    let Ok(mod_time) = meta.modified() else { return String::new() };
    let dt: chrono::DateTime<chrono::Local> = mod_time.into();
    dt.format("%Y-%m-%d %H:%M").to_string()
}
