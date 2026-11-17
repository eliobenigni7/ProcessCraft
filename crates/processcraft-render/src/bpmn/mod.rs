//! BPMN scene builder: turns a diagram of a `BpmnDocument` (or a culled viewport slice of

//! it) into a `DiagramScene` of shapes, edges, markers and labels, plus the hit regions the
//! canvas uses. Appearance colours, stroke widths and dash lengths are named constants here.

mod appearance;
mod edges;
mod hit;
mod labels;
mod markers;
mod overlay;
mod primitives;
mod scene;
mod semantics;
mod viewport;

pub use appearance::{Appearance, contrast_ratio, overlay_fill, overlay_ink};
pub use hit::{HitGeometry, HitRegion};
pub use scene::BpmnScene;
pub use viewport::{BpmnViewport, Viewport, ZOOM_RANGE};

/// Pool/lane header band width, shared with the modeling rules (bpmn.io and most
/// modelers place lanes 30 px inside the pool edge).
const HEADER: f64 = processcraft_core::containers::LANE_INDENTATION;
