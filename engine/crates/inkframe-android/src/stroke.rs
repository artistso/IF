use inkframe_core::{Brush, StrokeSample, sample_flags};

const MAX_DABS_PER_SEGMENT: usize = 4096;
const MIN_SPACING_PX: f32 = 0.75;
const MAX_SPACING_PX: f32 = 4.0;
/// Temporary safety bound for the first-present brush proof. Until completed
/// strokes are flattened into sparse raster tiles, the oldest bootstrap dabs
/// roll off instead of allowing frame-recording cost and memory to grow forever.
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
pub(crate) struct StrokePreview {
    brush: Brush,
    committed: Vec<StrokeDab>,
    predicted: Vec<StrokeDab>,
    last_actual: Option<StrokeSample>,
    active_stroke_start: Option<usize>,
}

impl Default for StrokePreview {
    fn default() -> Self {
        Self::new(Brush::pencil())
    }
}

impl StrokePreview {
    pub(crate) fn new(mut brush: Brush) -> Self {
        brush.sanitize();
        Self {
            brush,
            committed: Vec::new(),
            predicted: Vec::new(),
            last_actual: None,
            active_stroke_start: None,
        }
    }

    pub(crate) fn committed(&self) -> &[StrokeDab] {
        &self.committed
    }

    pub(crate) fn predicted(&self) -> &[StrokeDab] {
        &self.predicted
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
                self.active_stroke_start = Some(self.committed.len());
                self.last_actual = None;
            } else if self.active_stroke_start.is_none() {
                // Be defensive if Android delivers a MOVE after a lifecycle transition
                // where the original DOWN was not observed by this engine instance.
                self.active_stroke_start = Some(self.committed.len());
            }

            let previous = self.last_actual;
            Self::append_segment(&self.brush, &mut self.committed, previous, sample);
            self.enforce_committed_limit();
            self.last_actual = Some(sample);
            predicted_anchor = self.last_actual;

            if sample.flags & sample_flags::UP != 0 {
                self.last_actual = None;
                self.active_stroke_start = None;
                predicted_anchor = None;
            }
        }
    }

    fn cancel_active_stroke(&mut self) {
        if let Some(start) = self.active_stroke_start.take() {
            self.committed.truncate(start);
        }
        self.last_actual = None;
        self.predicted.clear();
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
        target: &mut Vec<StrokeDab>,
        previous: Option<StrokeSample>,
        sample: StrokeSample,
    ) {
        let Some(previous) = previous else {
            target.push(Self::dab_from_sample(brush, sample));
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
        let eraser = sample.flags & sample_flags::ERASER != 0;

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

    fn dab_from_sample(brush: &Brush, sample: StrokeSample) -> StrokeDab {
        StrokeDab {
            x: sample.x,
            y: sample.y,
            diameter: brush.diameter_for_pressure(sample.pressure),
            eraser: sample.flags & sample_flags::ERASER != 0,
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
        assert_eq!(committed.first().unwrap().diameter, 2.0);
        assert_eq!(committed.last().unwrap().diameter, 6.0);
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
    fn committed_bootstrap_replay_is_hard_bounded() {
        let mut preview = StrokePreview::default();
        preview.ingest(&[
            sample(0.0, 0.5, sample_flags::DOWN, 1),
            sample(50_000.0, 0.5, sample_flags::MOVE, 2),
        ]);

        assert_eq!(preview.committed().len(), MAX_BOOTSTRAP_DABS);
    }
}
