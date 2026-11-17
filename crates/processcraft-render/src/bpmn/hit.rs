//! Hit regions the canvas uses for picking, in absolute document space.

use processcraft_core::{
    ids::{ElementKey, ElementSelector},
    model::{Bounds, NodeKind, Point},
};

#[derive(Clone, Debug)]
pub enum HitGeometry {
    Shape(Bounds),
    Ellipse(Bounds),
    Diamond(Bounds),
    Hexagon(Bounds),
    DataObject(Bounds),
    DataStore(Bounds),
    Edge(Vec<Point>),
}

#[derive(Clone, Debug)]
pub struct HitRegion {
    pub key: ElementKey,
    pub diagram_element_id: ElementSelector,
    /// Independent BPMNLabel identity when this hit addresses an explicitly identified label.
    pub label_id: Option<ElementSelector>,
    /// True for label hits even when the BPMNLabel has no independent ID.
    pub is_label: bool,
    pub geometry: HitGeometry,
}

impl HitRegion {
    pub fn contains(&self, x: f64, y: f64, tolerance: f64) -> bool {
        match &self.geometry {
            HitGeometry::Shape(b) => {
                x >= b.x - tolerance
                    && x <= b.x + b.width + tolerance
                    && y >= b.y - tolerance
                    && y <= b.y + b.height + tolerance
            }
            HitGeometry::Ellipse(b) | HitGeometry::Diamond(b) => {
                let rx = b.width / 2.0 + tolerance;
                let ry = b.height / 2.0 + tolerance;
                if rx <= 0.0 || ry <= 0.0 {
                    return false;
                }
                let dx = (x - b.x - b.width / 2.0).abs() / rx;
                let dy = (y - b.y - b.height / 2.0).abs() / ry;
                if matches!(self.geometry, HitGeometry::Ellipse(_)) {
                    dx * dx + dy * dy <= 1.0
                } else {
                    dx + dy <= 1.0
                }
            }
            HitGeometry::DataObject(b) | HitGeometry::DataStore(b) => {
                let kind = if matches!(self.geometry, HitGeometry::DataObject(_)) {
                    NodeKind::DataObjectReference
                } else {
                    NodeKind::DataStoreReference
                };
                processcraft_core::geometry::hit_shape(kind, b, &Point { x, y }, tolerance, 1.0)
                    .unwrap_or(false)
            }
            HitGeometry::Hexagon(b) => {
                let x = (x - b.x) / b.width;
                let y = (y - b.y) / b.height;
                let t = tolerance / b.width.min(b.height).max(1.0);
                x >= -t
                    && x <= 1.0 + t
                    && y >= -t
                    && y <= 1.0 + t
                    && x >= 0.25 * (1.0 - 2.0 * y).abs() - t
                    && x <= 1.0 - 0.25 * (1.0 - 2.0 * y).abs() + t
            }
            HitGeometry::Edge(points) => points.windows(2).any(|p| {
                let dx = p[1].x - p[0].x;
                let dy = p[1].y - p[0].y;
                let l = dx * dx + dy * dy;
                let t = if l == 0.0 {
                    0.0
                } else {
                    ((x - p[0].x) * dx + (y - p[0].y) * dy) / l
                }
                .clamp(0.0, 1.0);
                (x - p[0].x - t * dx).hypot(y - p[0].y - t * dy) <= tolerance
            }),
        }
    }
}
