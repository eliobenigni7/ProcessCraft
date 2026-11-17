//! Appearance palettes, overlay colour scales, contrast helpers and the named stroke/dash constants.

use processcraft_core::{model::NodeKind, overlay::OverlayScale};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Appearance {
    #[default]
    Light,
    Dark,
    /// The interactive editor canvas: a dotted background, white activities and tinted pool and
    /// lane headers. Never used for export, so exported files keep the Light and Dark looks.
    CanvasLight,
    CanvasDark,
}

impl Appearance {
    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark | Self::CanvasDark)
    }
    pub fn is_canvas(self) -> bool {
        matches!(self, Self::CanvasLight | Self::CanvasDark)
    }
    /// The background, also used to knock out markers drawn over lines.
    pub(super) fn paper(self) -> [u8; 3] {
        match self {
            Self::Light => [255, 255, 255],
            Self::Dark => [27, 31, 38],
            Self::CanvasLight => [250, 251, 252],
            Self::CanvasDark => [18, 20, 23],
        }
    }
    /// Default text and outline colour of the appearance.
    pub(super) fn ink(self) -> [u8; 3] {
        match self {
            Self::Light => DARK_INK,
            Self::Dark => [230, 234, 240],
            Self::CanvasLight => [42, 46, 54],
            Self::CanvasDark => [201, 206, 214],
        }
    }
    /// Default fill of shapes.
    pub(super) fn fill(self) -> [u8; 3] {
        match self {
            Self::Light | Self::CanvasLight => [255, 255, 255],
            Self::Dark => [47, 53, 63],
            Self::CanvasDark => [32, 36, 42],
        }
    }
    /// Default fill of a node of `kind`: tinted start/end events, and tinted activities in
    /// the print appearances; everything else uses [`Self::fill`].
    pub(super) fn node_fill(self, kind: Option<NodeKind>) -> [u8; 3] {
        let light = matches!(self, Self::Light | Self::CanvasLight);
        match kind {
            Some(NodeKind::StartEvent) if light => [240, 249, 242],
            Some(NodeKind::StartEvent) => [40, 68, 53],
            Some(NodeKind::EndEvent) if light => [255, 243, 240],
            Some(NodeKind::EndEvent) => [74, 48, 45],
            Some(
                NodeKind::Task
                | NodeKind::UserTask
                | NodeKind::ManualTask
                | NodeKind::ServiceTask
                | NodeKind::SendTask
                | NodeKind::ReceiveTask
                | NodeKind::ScriptTask
                | NodeKind::BusinessRuleTask
                | NodeKind::SubProcess
                | NodeKind::AdHocSubProcess
                | NodeKind::Transaction
                | NodeKind::CallActivity,
            ) if self == Self::Light => [255, 252, 240],
            Some(
                NodeKind::Task
                | NodeKind::UserTask
                | NodeKind::ManualTask
                | NodeKind::ServiceTask
                | NodeKind::SendTask
                | NodeKind::ReceiveTask
                | NodeKind::ScriptTask
                | NodeKind::BusinessRuleTask
                | NodeKind::SubProcess
                | NodeKind::AdHocSubProcess
                | NodeKind::Transaction
                | NodeKind::CallActivity,
            ) if self == Self::Dark => [54, 56, 45],
            _ => self.fill(),
        }
    }
    /// Band behind the participant names of a choreography.
    pub(super) fn choreography_band(self) -> [u8; 3] {
        if self.is_dark() {
            [60, 74, 90]
        } else {
            [237, 241, 245]
        }
    }
    /// Header strip of a pool or lane.
    pub(super) fn lane_header_band(self) -> [u8; 3] {
        if self.is_dark() {
            [35, 39, 45]
        } else {
            [242, 244, 247]
        }
    }
    /// Dots of the canvas grid.
    pub(super) fn grid_dot(self) -> [u8; 3] {
        if self.is_dark() {
            [46, 51, 58]
        } else {
            [211, 216, 223]
        }
    }
}

/// Outline and text colour of the Light appearance; also used on the light non-initiating
/// fills in every appearance.
pub(super) const DARK_INK: [u8; 3] = [26, 37, 52];

/// Fill of non-initiating participant bands and message envelopes.
pub(super) const NON_INITIATING_FILL: [u8; 3] = [225, 225, 225];

// Stroke widths by role.
pub(super) const STROKE_HAIRLINE: f32 = 0.8;

pub(super) const STROKE_OUTLINE: f32 = 1.0;

pub(super) const STROKE_ICON: f32 = 1.2;

pub(super) const STROKE_ARROW: f32 = 1.5;

pub(super) const STROKE_HEAVY: f32 = 3.0;

// Dash lengths (on = off) of dashed lines.
pub(super) const MESSAGE_DASH: f32 = 7.0;

pub(super) const DOTTED_DASH: f32 = 2.0;

pub(super) const OUTLINE_DASH: f32 = 5.0;

// Overlay colour sets (M5.4). Light fills pair with dark ink and dark fills with
// light ink; `render_regressions` checks every entry against `Appearance::ink`
// for at least 4.5:1, so label text stays readable on every bucket colour.
pub(super) const OVERLAY_SEQUENTIAL_LIGHT: [[u8; 3]; 5] = [
    [222, 235, 250],
    [190, 216, 244],
    [158, 195, 236],
    [128, 174, 226],
    [106, 156, 216],
];

pub(super) const OVERLAY_SEQUENTIAL_DARK: [[u8; 3]; 5] = [
    [27, 42, 64],
    [28, 52, 88],
    [29, 62, 110],
    [30, 72, 128],
    [32, 82, 146],
];

pub(super) const OVERLAY_CATEGORICAL_LIGHT: [[u8; 3]; 8] = [
    [142, 186, 232],
    [240, 176, 110],
    [140, 200, 140],
    [238, 160, 190],
    [186, 160, 225],
    [226, 208, 110],
    [110, 200, 196],
    [200, 180, 160],
];

pub(super) const OVERLAY_CATEGORICAL_DARK: [[u8; 3]; 8] = [
    [28, 60, 110],
    [110, 60, 20],
    [28, 84, 44],
    [110, 40, 70],
    [66, 44, 110],
    [88, 80, 20],
    [20, 84, 84],
    [70, 70, 76],
];

/// Fill for an overlay bucket; out-of-range buckets clamp to the last colour.
pub fn overlay_fill(appearance: Appearance, scale: OverlayScale, bucket: u8) -> [u8; 3] {
    let i = usize::from(bucket);
    match (scale, appearance.is_dark()) {
        (OverlayScale::Sequential, false) => OVERLAY_SEQUENTIAL_LIGHT[i.min(4)],
        (OverlayScale::Sequential, true) => OVERLAY_SEQUENTIAL_DARK[i.min(4)],
        (OverlayScale::Categorical, false) => OVERLAY_CATEGORICAL_LIGHT[i.min(7)],
        (OverlayScale::Categorical, true) => OVERLAY_CATEGORICAL_DARK[i.min(7)],
    }
}

/// The label ink drawn on overlay fills.
pub fn overlay_ink(appearance: Appearance) -> [u8; 3] {
    appearance.ink()
}

/// WCAG 2.x contrast ratio of two sRGB colours.
pub fn contrast_ratio(a: [u8; 3], b: [u8; 3]) -> f64 {
    pub(super) fn luminance(c: [u8; 3]) -> f64 {
        let channel = |v: u8| {
            let x = f64::from(v) / 255.0;
            if x <= 0.04045 {
                x / 12.92
            } else {
                ((x + 0.055) / 1.055).powf(2.4)
            }
        };
        channel(c[0]) * 0.2126 + channel(c[1]) * 0.7152 + channel(c[2]) * 0.0722
    }
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

pub(super) fn color(value: Option<&str>, default: [u8; 3]) -> [u8; 3] {
    value
        .and_then(|s| {
            let s = s.strip_prefix('#')?;
            if s.len() != 6 || !s.is_ascii() {
                return None;
            }
            Some([
                u8::from_str_radix(&s[0..2], 16).ok()?,
                u8::from_str_radix(&s[2..4], 16).ok()?,
                u8::from_str_radix(&s[4..6], 16).ok()?,
            ])
        })
        .unwrap_or(default)
}

pub(super) fn low_contrast(a: [u8; 3], b: [u8; 3]) -> bool {
    contrast_ratio(a, b) < 3.0
}
