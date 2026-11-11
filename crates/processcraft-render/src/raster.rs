use crate::{DiagramScene, RenderError};
use tiny_skia::{FillRule, Paint, Pixmap, Transform};

impl DiagramScene {
    /// Explicit scale; at most 16,384 pixels per axis and 64 MiB RGBA allocation.
    /// Uses the same unhinted outlines as vector output; no second text rasterizer.
    pub fn rasterize(&self, scale: f32) -> Result<Pixmap, RenderError> {
        if !scale.is_finite() || !(0.125..=8.0).contains(&scale) {
            return Err(RenderError("Raster scale must be finite within 0.125..8"));
        }
        self.rasterize_viewport(0.0, 0.0, self.width(), self.height(), scale)
    }

    /// Rasterize a viewport in scene coordinates with the same path renderer.
    pub fn rasterize_viewport(
        &self,
        x: f32,
        y: f32,
        viewport_width: f32,
        viewport_height: f32,
        scale: f32,
    ) -> Result<Pixmap, RenderError> {
        if !x.is_finite()
            || !y.is_finite()
            || !viewport_width.is_finite()
            || !viewport_height.is_finite()
            || viewport_width <= 0.0
            || viewport_height <= 0.0
            || !scale.is_finite()
            || !(0.125..=8.0).contains(&scale)
        {
            return Err(RenderError("Invalid viewport"));
        }
        let width = (viewport_width * scale).ceil() as u32;
        let height = (viewport_height * scale).ceil() as u32;
        if width == 0
            || height == 0
            || width > 16384
            || height > 16384
            || u64::from(width) * u64::from(height) * 4 > 64 * 1024 * 1024
        {
            return Err(RenderError(
                "Raster exceeds 16384-pixel axis or 64 MiB allocation limit",
            ));
        }
        let mut pixmap =
            Pixmap::new(width, height).ok_or(RenderError("Raster allocation failed"))?;
        pixmap.fill(tiny_skia::Color::WHITE);
        let mut paint = Paint::default();
        for item in self.paths() {
            paint.set_color_rgba8(item.color[0], item.color[1], item.color[2], 255);
            let transform = Transform::from_row(
                scale,
                0.0,
                0.0,
                scale,
                (item.x - x) * scale,
                (item.y - y) * scale,
            );
            pixmap.fill_path(item.path, &paint, FillRule::Winding, transform, None);
        }
        Ok(pixmap)
    }
}
