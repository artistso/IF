use std::collections::BTreeMap;
use std::fmt;

pub const STROKE_SAMPLE_STRIDE: usize = 32;

pub mod sample_flags {
    pub const DOWN: u32 = 1 << 0;
    pub const MOVE: u32 = 1 << 1;
    pub const UP: u32 = 1 << 2;
    pub const CANCEL: u32 = 1 << 3;
    pub const PREDICTED: u32 = 1 << 4;
    pub const ERASER: u32 = 1 << 5;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeSample {
    pub x: f32,
    pub y: f32,
    pub pressure: f32,
    pub tilt: f32,
    pub orientation: f32,
    pub time_ns: i64,
    pub flags: u32,
}

impl StrokeSample {
    pub fn is_predicted(self) -> bool {
        self.flags & sample_flags::PREDICTED != 0
    }

    pub fn is_committable(self) -> bool {
        !self.is_predicted() && self.flags & sample_flags::CANCEL == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    CountOverflow,
    BufferTooSmall { required: usize, actual: usize },
    NonFiniteSample { index: usize },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CountOverflow => write!(f, "sample count overflows packet size"),
            Self::BufferTooSmall { required, actual } => {
                write!(
                    f,
                    "sample packet requires {required} bytes but buffer has {actual}"
                )
            }
            Self::NonFiniteSample { index } => write!(f, "sample {index} contains NaN/Infinity"),
        }
    }
}

impl std::error::Error for DecodeError {}

pub fn decode_stroke_samples(
    bytes: &[u8],
    sample_count: usize,
) -> Result<Vec<StrokeSample>, DecodeError> {
    let required = sample_count
        .checked_mul(STROKE_SAMPLE_STRIDE)
        .ok_or(DecodeError::CountOverflow)?;
    if required > bytes.len() {
        return Err(DecodeError::BufferTooSmall {
            required,
            actual: bytes.len(),
        });
    }

    let mut out = Vec::with_capacity(sample_count);
    for index in 0..sample_count {
        let base = index * STROKE_SAMPLE_STRIDE;
        let f32_at = |offset: usize| {
            f32::from_le_bytes(bytes[base + offset..base + offset + 4].try_into().unwrap())
        };
        let sample = StrokeSample {
            x: f32_at(0),
            y: f32_at(4),
            pressure: f32_at(8).clamp(0.0, 1.0),
            tilt: f32_at(12),
            orientation: f32_at(16),
            time_ns: i64::from_le_bytes(bytes[base + 20..base + 28].try_into().unwrap()),
            flags: u32::from_le_bytes(bytes[base + 28..base + 32].try_into().unwrap()),
        };
        if !sample.x.is_finite()
            || !sample.y.is_finite()
            || !sample.pressure.is_finite()
            || !sample.tilt.is_finite()
            || !sample.orientation.is_finite()
        {
            return Err(DecodeError::NonFiniteSample { index });
        }
        out.push(sample);
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrushKind {
    Round,
    Pencil,
    Ink,
    Airbrush,
    Eraser,
    Marker,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Brush {
    pub id: String,
    pub name: String,
    pub kind: BrushKind,
    pub size_px: f32,
    pub min_size_px: f32,
    pub opacity: f32,
    pub flow: f32,
    pub hardness: f32,
    pub spacing: f32,
    pub pressure_to_size: bool,
    pub pressure_to_opacity: bool,
    pub smoothing: f32,
    pub build_up: bool,
}

impl Brush {
    pub fn pencil() -> Self {
        Self {
            id: "pencil".into(),
            name: "Pencil".into(),
            kind: BrushKind::Pencil,
            size_px: 6.0,
            min_size_px: 2.0,
            opacity: 1.0,
            flow: 1.0,
            hardness: 0.95,
            spacing: 0.05,
            pressure_to_size: true,
            pressure_to_opacity: false,
            smoothing: 0.35,
            build_up: false,
        }
    }

    pub fn diameter_for_pressure(&self, pressure: f32) -> f32 {
        if !self.pressure_to_size {
            return self.size_px;
        }
        let p = pressure.clamp(0.0, 1.0);
        self.min_size_px + (self.size_px - self.min_size_px) * p
    }

    pub fn flow_for_pressure(&self, pressure: f32) -> f32 {
        if self.pressure_to_opacity {
            self.flow * pressure.clamp(0.0, 1.0)
        } else {
            self.flow
        }
    }

    pub fn sanitize(&mut self) {
        self.size_px = self.size_px.max(0.1);
        self.min_size_px = self.min_size_px.clamp(0.1, self.size_px);
        self.opacity = self.opacity.clamp(0.0, 1.0);
        self.flow = self.flow.clamp(0.0, 1.0);
        self.hardness = self.hardness.clamp(0.0, 1.0);
        self.spacing = self.spacing.clamp(0.001, 4.0);
        self.smoothing = self.smoothing.clamp(0.0, 1.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point2 {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CanvasShape {
    Rectangle,
    RoundedRectangle { radius_px: f32 },
    Circle,
    Ellipse,
    Triangle,
    Polygon(Vec<Point2>),
    Star { points: u16, inner_ratio: f32 },
    Heart,
    CustomMask(u64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanvasSpec {
    pub width_px: u32,
    pub height_px: u32,
    pub fps: f32,
    pub pixel_aspect: f32,
    pub shape: CanvasShape,
}

impl Default for CanvasSpec {
    fn default() -> Self {
        Self {
            width_px: 1920,
            height_px: 1080,
            fps: 24.0,
            pixel_aspect: 1.0,
            shape: CanvasShape::Rectangle,
        }
    }
}

pub type SurfaceId = u64;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CelTransform {
    pub tx: f32,
    pub ty: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation_rad: f32,
}

impl Default for CelTransform {
    fn default() -> Self {
        Self {
            tx: 0.0,
            ty: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation_rad: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cel {
    pub surface_id: SurfaceId,
    pub transform: CelTransform,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    pub cels: BTreeMap<u32, Cel>,
}

impl Layer {
    pub fn cel_at(&self, frame: u32) -> Option<&Cel> {
        self.cels.range(..=frame).next_back().map(|(_, cel)| cel)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TileKey {
    pub surface_id: SurfaceId,
    pub x: i32,
    pub y: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encode_sample(sample: StrokeSample) -> [u8; STROKE_SAMPLE_STRIDE] {
        let mut b = [0_u8; STROKE_SAMPLE_STRIDE];
        b[0..4].copy_from_slice(&sample.x.to_le_bytes());
        b[4..8].copy_from_slice(&sample.y.to_le_bytes());
        b[8..12].copy_from_slice(&sample.pressure.to_le_bytes());
        b[12..16].copy_from_slice(&sample.tilt.to_le_bytes());
        b[16..20].copy_from_slice(&sample.orientation.to_le_bytes());
        b[20..28].copy_from_slice(&sample.time_ns.to_le_bytes());
        b[28..32].copy_from_slice(&sample.flags.to_le_bytes());
        b
    }

    #[test]
    fn packet_round_trip_and_pressure_clamp() {
        let source = StrokeSample {
            x: 12.5,
            y: -3.25,
            pressure: 1.5,
            tilt: 0.2,
            orientation: -0.7,
            time_ns: 42,
            flags: sample_flags::MOVE,
        };
        let decoded = decode_stroke_samples(&encode_sample(source), 1).unwrap();
        assert_eq!(decoded[0].x, source.x);
        assert_eq!(decoded[0].y, source.y);
        assert_eq!(decoded[0].pressure, 1.0);
        assert_eq!(decoded[0].time_ns, 42);
    }

    #[test]
    fn decoder_rejects_short_packet() {
        let error = decode_stroke_samples(&[0; 31], 1).unwrap_err();
        assert!(matches!(error, DecodeError::BufferTooSmall { .. }));
    }

    #[test]
    fn predicted_samples_are_not_committable() {
        let sample = StrokeSample {
            x: 0.0,
            y: 0.0,
            pressure: 1.0,
            tilt: 0.0,
            orientation: 0.0,
            time_ns: 0,
            flags: sample_flags::MOVE | sample_flags::PREDICTED,
        };
        assert!(!sample.is_committable());
    }

    #[test]
    fn brush_pressure_matches_v5_contract() {
        let brush = Brush::pencil();
        assert_eq!(brush.diameter_for_pressure(0.0), 2.0);
        assert_eq!(brush.diameter_for_pressure(1.0), 6.0);
        assert_eq!(brush.diameter_for_pressure(0.5), 4.0);
    }

    #[test]
    fn exposure_hold_resolves_latest_preceding_cel() {
        let mut cels = BTreeMap::new();
        cels.insert(
            2,
            Cel {
                surface_id: 10,
                transform: CelTransform::default(),
            },
        );
        cels.insert(
            8,
            Cel {
                surface_id: 20,
                transform: CelTransform::default(),
            },
        );
        let layer = Layer {
            id: 1,
            name: "Layer 1".into(),
            visible: true,
            locked: false,
            opacity: 1.0,
            cels,
        };
        assert!(layer.cel_at(1).is_none());
        assert_eq!(layer.cel_at(7).unwrap().surface_id, 10);
        assert_eq!(layer.cel_at(8).unwrap().surface_id, 20);
        assert_eq!(layer.cel_at(100).unwrap().surface_id, 20);
    }
}
