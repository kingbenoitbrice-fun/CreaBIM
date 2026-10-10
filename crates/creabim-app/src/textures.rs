//! GPU textures for shaded view images, one per view, replaced whenever
//! the view's image changes.

use creabim_geom::raster::Raster;
use eframe::egui;
use std::sync::Arc;

/// The texture showing `raster`, uploading it the first time it is seen.
pub fn texture(ctx: &egui::Context, raster: &Arc<Raster>) -> egui::TextureId {
    let key = egui::Id::new(("view raster", raster.key));
    let ptr = Arc::as_ptr(raster) as usize;
    let cached: Option<(usize, egui::TextureHandle)> = ctx.data(|d| d.get_temp(key));
    if let Some((p, handle)) = cached {
        if p == ptr {
            return handle.id();
        }
    }
    let image =
        egui::ColorImage::from_rgba_unmultiplied([raster.width, raster.height], &raster.rgba);
    let handle = ctx.load_texture(
        format!("view-{}", raster.key),
        image,
        egui::TextureOptions::LINEAR,
    );
    let id = handle.id();
    // replacing the entry drops the old handle, which frees its texture
    ctx.data_mut(|d| d.insert_temp(key, (ptr, handle)));
    id
}

/// Paint a shaded image spanning screen rect `rect`, clipped to `clip`.
pub fn paint(
    painter: &egui::Painter,
    rect: egui::Rect,
    clip: Option<egui::Rect>,
    raster: &Arc<Raster>,
) {
    let id = texture(painter.ctx(), raster);
    let painter = match clip {
        Some(c) => painter.with_clip_rect(painter.clip_rect().intersect(c)),
        None => painter.clone(),
    };
    painter.image(
        id,
        rect,
        egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}
