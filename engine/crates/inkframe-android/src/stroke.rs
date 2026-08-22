use std::collections::BTreeSet;

use inkframe_core::{Brush, StrokeSample, sample_flags};
use inkframe_engine::BrushSettings;
use inkframe_raster::{RasterDab, SparseRaster, StrokeScratch, StrokeStyle, TileCoord};

const MAX_DABS_PER_SEGMENT: usize = 4096;
const MIN_SPACING_PX: f32 = 0.75;
const MAX_SPACING_PX: f32 = 4.0;
/// Temporary safety bound for the first-present brush proof. Persistent raster
/// state is no longer bounded by this value; only the legacy Vulkan replay is.
pub(crate) const MAX_BOOTSTRAP_DABS: usize = 4096;
const MAX_PREDICTED_DABS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StrokeDab {
    pub x: f32,
    pub y: f32,
    pub diameter: f32,
    pub eraser: bool,
}

#[derive(Debug)]
enum ActiveRasterStroke {
    Normal {
        scratch: StrokeScratch,
        style: StrokeStyle,
    },
    BuildUp {
        dabs: Vec<RasterDab>,
        style: StrokeStyle,
    },
}

#[derive(Debug)]
pub(crate) struct StrokePreview {
    brush: Brush,
    settings: BrushSettings,
    pending_settings: Option<BrushSettings>,
    committed: Vec<StrokeDab>,
    predicted: Vec<StrokeDab>,
    last_actual: Option<StrokeSample>,
    active_stroke_start: Option<usize>,
    raster: SparseRaster,
    raster_tile_coords: BTreeSet<TileCoord>,
    active_raster_stroke: Option<ActiveRasterStroke>,
}

impl Default for StrokePreview {
    fn default() -> Self {
        Self::new(Brush::pencil())
    }
}

impl StrokePreview {
    pub(crate) fn new(mut brush: Brush) -> Self {
        brush.sanitize();
        let settings = BrushSettings::default().sanitized();
        Self::apply_settings_to_brush(&mut brush, settings);
        Self {
            brush,
            settings,
            pending_settings: None,
            committed: Vec::new(),
            predicted: Vec::new(),
            last_actual: None,
            active_stroke_start: None,
            raster: SparseRaster::new(),
            raster_tile_coords: BTreeSet::new(),
            active_raster_stroke: None,
        }
    }

    pub(crate) fn set_brush_settings(&mut self, settings: BrushSettings) {
        let settings = settings.sanitized();
        if self.active_stroke_start.is_some() {
            self.pending_settings = Some(settings);
            return;
        }
        self.apply_settings(settings);
    }

    pub(crate) fn brush_settings(&self) -> BrushSettings {
        self.settings
    }

    pub(crate) fn committed(&self) -> &[StrokeDab] {
        &self.committed
    }

    /// Only the actual dabs belonging to the currently cancellable stroke.
    /// Once UP seals a stroke into the persistent raster, this slice is empty.
    pub(crate) fn active(&self) -> &[StrokeDab] {
        let Some(start) = self.active_stroke_start else {
            return &[];
        };
        &self.committed[start.min(self.committed.len())..]
    }

    pub(crate) fn predicted(&self) -> &[StrokeDab] {
        &self.predicted
    }

    /// Authoritative persistent pixels accumulated from completed actual strokes.
    pub(crate) fn raster(&self) -> &SparseRaster {
        &self.raster
    }

    pub(crate) fn raster_mut(&mut self) -> &mut SparseRaster {
        &mut self.raster
    }

    /// Coordinates are tracked separately from dirty rectangles so a newly
    /// created GPU viewport can repopulate current pixels after rotation or
    /// surface recreation even when all prior dirty regions were acknowledged.
    pub(crate) fn raster_tile_coords(&self) -> impl Iterator<Item = TileCoord> + '_ {
        self.raster_tile_coords.iter().copied()
    }

    pub(crate) fn ingest(&mut self, samples: &[StrokeSample]) {
        // Prediction is a visual tail only. Every real input packet replaces it.
        self.predicted.clear();
        let mut predicted_anchor = self.last_actual;

        for sample in samples.iter().copied() {
            if sample.flags & sample_flags::CANCEL != 0 {
                self.cancel_active_stroke();
                predicted_anchor = None;
                continue;
            }

            if sample.is_predicted() {
                if self.active_stroke_start.is_some() {
                    Self::append_segment(
                        &self.brush,
                        self.settings.eraser,
                        &mut self.predicted,
                        predicted_anchor,
                        sample,
                    );
                    Self::trim_front(&mut self.predicted, MAX_PREDICTED_DABS);
                    predicted_anchor = Some(sample);
                }
                continue;
            }

            if sample.flags & sample_flags::DOWN != 0 {
                // A second DOWN without UP/CANCEL means Android/lifecycle delivery
                // skipped a terminal event. Discard the stale unsealed stroke.
                if self.active_stroke_start.is_some() {
                    self.cancel_active_stroke();
                }
                self.active_stroke_start = Some(self.committed.len());
                self.last_actual = None;
                self.begin_raster_stroke(sample);
            } else if self.active_stroke_start.is_none() {
                // Be defensive if Android delivers a MOVE after a lifecycle transition
                // where the original DOWN was not observed by this engine instance.
                self.active_stroke_start = Some(self.committed.len());
                self.begin_raster_stroke(sample);
            }

            let previous = self.last_actual;
            let mut generated = Vec::new();
            Self::append_segment(
                &self.brush,
                self.settings.eraser,
                &mut generated,
                previous,
                sample,
            );
            self.accumulate_actual_raster(&generated);
            self.committed.extend_from_slice(&generated);
            self.enforce_committed_limit();
            self.last_actual = Some(sample);
            predicted_anchor = self.last_actual;

            if sample.flags & sample_flags::UP != 0 {
                self.commit_active_raster_stroke();
                self.last_actual = None;
                self.active_stroke_start = None;
                predicted_anchor = None;
                self.apply_pending_settings();
            }
        }
    }

    fn apply_settings_to_brush(brush: &mut Brush, settings: BrushSettings) {
        brush.size_px = settings.size_px;
        brush.min_size_px = (settings.size_px * 0.18).clamp(0.5, settings.size_px);
        brush.opacity = settings.opacity;
        brush.sanitize();
    }

    fn apply_settings(&mut self, settings: BrushSettings) {
        let settings = settings.sanitized();
        Self::apply_settings_to_brush(&mut self.brush, settings);
        self.settings = settings;
    }

    fn apply_pending_settings(&mut self) {
        if let Some(settings) = self.pending_settings.take() {
            self.apply_settings(settings);
        }
    }

    fn srgb_channel_to_linear_byte(value: u8) -> u8 {
        let srgb = value as f32 / 255.0;
        let linear = if srgb <= 0.04045 {
            srgb / 12.92
        } else {
            ((srgb + 0.055) / 1.055).powf(2.4)
        };
        (linear * 255.0).round() as u8
    }

    fn linear_ink_rgba(&self) -> [u8; 4] {
        [
            Self::srgb_channel_to_linear_byte(self.settings.color_srgb[0]),
            Self::srgb_channel_to_linear_byte(self.settings.color_srgb[1]),
            Self::srgb_channel_to_linear_byte(self.settings.color_srgb[2]),
            (self.brush.opacity.clamp(0.0, 1.0) * 255.0).round() as u8,
        ]
    }

    fn begin_raster_stroke(&mut self, sample: StrokeSample) {
        let eraser = self.settings.eraser || sample.flags & sample_flags::ERASER != 0;
        let style = if eraser {
            StrokeStyle::erase()
        } else {
            StrokeStyle::ink(self.linear_ink_rgba())
        }
        .with_build_up(self.brush.build_up);

        self.active_raster_stroke = Some(if style.build_up {
            ActiveRasterStroke::BuildUp {
                dabs: Vec::new(),
                style,
            }
        } else {
            ActiveRasterStroke::Normal {
                scratch: StrokeScratch::new(),
                style,
            }
        });
    }

    fn accumulate_actual_raster(&mut self, dabs: &[StrokeDab]) {
        let Some(active) = &mut self.active_raster_stroke else {
            return;
        };
        match active {
            ActiveRasterStroke::Normal { scratch, .. } => {
                scratch.add_dabs(
                    dabs.iter()
                        .map(|dab| RasterDab::new(dab.x, dab.y, dab.diameter)),
                );
            }
            ActiveRasterStroke::BuildUp {
                dabs: build_up_dabs,
                ..
            } => {
                build_up_dabs.extend(
                    dabs.iter()
                        .map(|dab| RasterDab::new(dab.x, dab.y, dab.diameter)),
                );
            }
        }
    }

    fn commit_active_raster_stroke(&mut self) {
        let Some(active) = self.active_raster_stroke.take() else {
            return;
        };
        match active {
            ActiveRasterStroke::Normal { scratch, style } => {
                self.raster.commit_normal_stroke(scratch, style);
            }
            ActiveRasterStroke::BuildUp { dabs, style } => {
                self.raster.apply_stroke(dabs, style);
            }
        }

        let changed_coords: Vec<_> = self
            .raster
            .dirty_tiles()
            .map(|(coord, _, _)| coord)
            .collect();
        self.raster_tile_coords.extend(changed_coords);
    }

    fn cancel_active_stroke(&mut self) {
        if let Some(start) = self.active_stroke_start.take() {
            self.committed.truncate(start);
        }
        // Persistent tiles were never touched by a normal active stroke, so
        // cancellation is an O(1) discard of its scratch coverage.
        self.active_raster_stroke = None;
        self.last_actual = None;
        self.predicted.clear();
        self.apply_pending_settings();
    }

    fn enforce_committed_limit(&mut self) {
        let overflow = self.committed.len().saturating_sub(MAX_BOOTSTRAP_DABS);
        if overflow == 0 {
            return;
        }
        self.committed.drain(..overflow);
        if let Some(start) = self.active_stroke_start {
            self.active_stroke_start = Some(start.saturating_sub(overflow));
        }
    }

    fn trim_front(target: &mut Vec<StrokeDab>, limit: usize) {
        let overflow = target.len().saturating_sub(limit);
        if overflow > 0 {
            target.drain(..overflow);
        }
    }

    fn append_segment(
        brush: &Brush,
        force_eraser: bool,
        target: &mut Vec<StrokeDab>,
        previous: Option<StrokeSample>,
        sample: StrokeSample,
    ) {
        let Some(previous) = previous else {
            target.push(Self::dab_from_sample(brush, force_eraser, sample));
            return;
        };

        let dx = sample.x - previous.x;
        let dy = sample.y - previous.y;
        let distance = dx.hypot(dy);
        let start_diameter = brush.diameter_for_pressure(previous.pressure);
        let end_diameter = brush.diameter_for_pressure(sample.pressure);
        let spacing =
            (start_diameter.min(end_diameter) * 0.35).clamp(MIN_SPACING_PX, MAX_SPACING_PX);
        let steps = ((distance / spacing).ceil() as usize)
            .max(1)
            .min(MAX_DABS_PER_SEGMENT);
        let eraser = force_eraser || sample.flags & sample_flags::ERASER != 0;

        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let pressure = previous.pressure + (sample.pressure - previous.pressure) * t;
            target.push(StrokeDab {
                x: previous.x + dx * t,
                y: previous.y + dy * t,
                diameter: brush.diameter_for_pressure(pressure),
                eraser,
            });
        }
    }

    fn dab_from_sample(brush: &Brush, force_eraser: bool, sample: StrokeSample) -> StrokeDab {
        StrokeDab {
            x: sample.x,
            y: sample.y,
            diameter: brush.diameter_for_pressure(sample.pressure),
            eraser: force_eraser || sample.flags & sample_flags::ERASER != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(x: f32, pressure: f32, flags: u32, time_ns: i64) -> StrokeSample {
        StrokeSample {
            x,
            y: 10.0,
            pressure,
            tilt: 0.0,
            orientation: 0.0,
            time_ns,
            flags,
        }
    }

    #[test]
    fn pressure_changes_bootstrap_dab_diameter() {
        let mut preview = StrokePreview::default();
        preview.ingest(&[
            sample(0.0, 0.0, sample_flags::DOWN, 1),
            sample(12.0, 1.0, sample_flags::MOVE, 2),
        ]);

        let committed = preview.committed();
        assert_eq!(committed.first().unwrap().diameter, 2.52);
        assert_eq!(committed.last().unwrap().diameter, 14.0);
    }

    #[test]
    fn brush_settings_change_size_and_forced_eraser() {
        let mut preview = StrokePreview::default();
        preview.set_brush_settings(BrushSettings {
            color_srgb: [0, 120, 255],
            size_px: 30.0,
            opacity: 0.7,
            eraser: true,
        });
        preview.ingest(&[sample(10.0, 1.0, sample_flags::DOWN, 1)]);
        let dab = preview.active().last().unwrap();
        assert_eq!(dab.diameter, 30.0);
        assert!(dab.eraser);
    }

    #[test]
    fn settings_changed_midstroke_wait_until_terminal_event() {
        let mut preview = StrokePreview::default();
        preview.ingest(&[sample(0.0, 1.0, sample_flags::DOWN, 1)]);
        preview.set_brush_settings(BrushSettings {
            size_px: 40.0,
            ..BrushSettings::default()
        });
        preview.ingest(&[sample(4.0, 1.0, sample_flags::MOVE, 2)]);
        assert_eq!(preview.active().last().unwrap().diameter, 14.0);
        preview.ingest(&[sample(4.0, 1.0, sample_flags::UP, 3)]);
        assert_eq!(preview.brush_settings().size_px, 40.0);
    }

    #[test]
    fn selected_srgb_color_is_converted_to_linear_raster_bytes() {
        let mut preview = StrokePreview::default();
        preview.set_brush_settings(BrushSettings {
            color_srgb: [200, 0, 70],
            size_px: 14.0,
            opacity: 1.0,
            eraser: false,
        });
        preview.ingest(&[
            sample(30.0, 1.0, sample_flags::DOWN, 1),
            sample(30.0, 1.0, sample_flags::UP, 2),
        ]);
        let pixel = preview.raster().pixel_rgba(30, 10);
        assert!(pixel[0] >= 145 && pixel[0] <= 149);
        assert_eq!(pixel[1], 0);
        assert!(pixel[2] >= 15 && pixel[2] <= 18);
        assert!(pixel[3] > 0);
    }

    #[test]
    fn active_dabs_are_empty_after_up() {
        let mut preview = StrokePreview::default();
        preview.ingest(&[
            sample(0.0, 0.5, sample_flags::DOWN, 1),
            sample(8.0, 0.5, sample_flags::MOVE, 2),
        ]);
        assert!(!preview.active().is_empty());

        preview.ingest(&[sample(8.0, 0.5, sample_flags::UP, 3)]);
        assert!(preview.active().is_empty());
    }

    #[test]
    fn predicted_dabs_are_replaced_not_committed() {
        let mut preview = StrokePreview::default();
        preview.ingest(&[sample(0.0, 0.5, sample_flags::DOWN, 1)]);
        let committed_before_prediction = preview.committed().len();

        preview.ingest(&[sample(
            8.0,
            0.5,
            sample_flags::MOVE | sample_flags::PREDICTED,
            2,
        )]);
        assert_eq!(preview.committed().len(), committed_before_prediction);
        assert!(!preview.predicted().is_empty());

        preview.ingest(&[sample(8.0, 0.5, sample_flags::MOVE, 3)]);
        assert!(preview.predicted().is_empty());
        assert!(preview.committed().len() > committed_before_prediction);
    }

    #[test]
    fn cancel_removes_only_the_active_stroke() {
        let mut preview = StrokePreview::default();
        preview.ingest(&[
            sample(0.0, 0.5, sample_flags::DOWN, 1),
            sample(4.0, 0.5, sample_flags::UP, 2),
        ]);
        let first_stroke_len = preview.committed().len();

        preview.ingest(&[
            sample(20.0, 0.5, sample_flags::DOWN, 3),
            sample(30.0, 0.5, sample_flags::MOVE, 4),
        ]);
        assert!(preview.committed().len() > first_stroke_len);

        preview.ingest(&[sample(30.0, 0.5, sample_flags::CANCEL, 5)]);
        assert_eq!(preview.committed().len(), first_stroke_len);
        assert!(preview.predicted().is_empty());
    }

    #[test]
    fn actual_pixels_remain_scratch_only_until_up() {
        let mut preview = StrokePreview::default();
        preview.ingest(&[
            sample(0.0, 0.5, sample_flags::DOWN, 1),
            sample(8.0, 0.5, sample_flags::MOVE, 2),
        ]);

        assert_eq!(preview.raster().tile_count(), 0);
        assert_eq!(preview.raster().pixel_rgba(4, 10), [0; 4]);

        preview.ingest(&[sample(8.0, 0.5, sample_flags::UP, 3)]);
        assert!(preview.raster().tile_count() > 0);
        assert!(preview.raster().pixel_rgba(4, 10)[3] > 0);
        assert_eq!(
            preview.raster_tile_coords().count(),
            preview.raster().tile_count()
        );
    }

    #[test]
    fn cancel_discards_sparse_scratch_without_touching_document() {
        let mut preview = StrokePreview::default();
        preview.ingest(&[
            sample(40.0, 0.5, sample_flags::DOWN, 1),
            sample(48.0, 0.5, sample_flags::MOVE, 2),
        ]);
        assert_eq!(preview.raster().tile_count(), 0);

        preview.ingest(&[sample(48.0, 0.5, sample_flags::CANCEL, 3)]);
        assert_eq!(preview.raster().tile_count(), 0);
        assert_eq!(preview.raster().pixel_rgba(44, 10), [0; 4]);
    }

    #[test]
    fn predicted_tail_never_enters_persistent_raster() {
        let mut preview = StrokePreview::default();
        preview.ingest(&[sample(0.0, 0.5, sample_flags::DOWN, 1)]);
        preview.ingest(&[sample(
            80.0,
            0.5,
            sample_flags::MOVE | sample_flags::PREDICTED,
            2,
        )]);
        assert_eq!(preview.raster().pixel_rgba(80, 10), [0; 4]);

        preview.ingest(&[sample(0.0, 0.5, sample_flags::UP, 3)]);
        assert!(preview.raster().pixel_rgba(0, 10)[3] > 0);
        assert_eq!(preview.raster().pixel_rgba(80, 10), [0; 4]);
    }

    #[test]
    fn second_down_discards_stale_unsealed_stroke() {
        let mut preview = StrokePreview::default();
        preview.ingest(&[
            sample(10.0, 0.5, sample_flags::DOWN, 1),
            sample(20.0, 0.5, sample_flags::MOVE, 2),
            sample(100.0, 0.5, sample_flags::DOWN, 3),
            sample(104.0, 0.5, sample_flags::UP, 4),
        ]);

        assert_eq!(preview.raster().pixel_rgba(15, 10), [0; 4]);
        assert!(preview.raster().pixel_rgba(102, 10)[3] > 0);
    }

    #[test]
    fn separate_strokes_do_not_interpolate_across_the_gap() {
        let mut preview = StrokePreview::default();
        preview.ingest(&[
            sample(0.0, 0.5, sample_flags::DOWN, 1),
            sample(0.0, 0.5, sample_flags::UP, 2),
            sample(100.0, 0.5, sample_flags::DOWN, 3),
        ]);

        assert!(
            preview
                .committed()
                .iter()
                .all(|dab| dab.x <= 1.0 || dab.x >= 99.0)
        );
    }

    #[test]
    fn committed_bootstrap_replay_is_hard_bounded_but_raster_is_not_dab_bounded() {
        let mut preview = StrokePreview::default();
        preview.ingest(&[
            sample(0.0, 0.5, sample_flags::DOWN, 1),
            sample(50_000.0, 0.5, sample_flags::MOVE, 2),
        ]);

        assert_eq!(preview.committed().len(), MAX_BOOTSTRAP_DABS);
        assert_eq!(preview.raster().tile_count(), 0);

        preview.ingest(&[sample(50_000.0, 0.5, sample_flags::UP, 3)]);
        assert!(preview.raster().tile_count() > 1);
    }
}
