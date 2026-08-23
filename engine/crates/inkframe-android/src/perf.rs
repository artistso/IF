use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(crate) const TARGET_120HZ_US: u64 = 8_333;
pub(crate) const TARGET_60HZ_US: u64 = 16_667;

pub(crate) type SharedFrameTimingStats = Arc<Mutex<FrameTimingStats>>;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameTimingSample {
    pub total_us: u64,
    pub stroke_us: u64,
    pub raster_sync_us: u64,
    pub wait_acquire_us: u64,
    pub record_us: u64,
    pub submit_us: u64,
    pub queue_present_us: u64,
    pub brush_instances: u64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameTimingStats {
    pub frames: u64,
    pub total_us_sum: u64,
    pub total_us_max: u64,
    pub stroke_us_sum: u64,
    pub raster_sync_us_sum: u64,
    pub wait_acquire_us_sum: u64,
    pub record_us_sum: u64,
    pub submit_us_sum: u64,
    pub queue_present_us_sum: u64,
    pub frames_over_120hz_budget: u64,
    pub frames_over_60hz_budget: u64,
    pub max_brush_instances: u64,
}

impl FrameTimingStats {
    pub(crate) fn record(&mut self, sample: FrameTimingSample) {
        self.frames = self.frames.saturating_add(1);
        self.total_us_sum = self.total_us_sum.saturating_add(sample.total_us);
        self.total_us_max = self.total_us_max.max(sample.total_us);
        self.stroke_us_sum = self.stroke_us_sum.saturating_add(sample.stroke_us);
        self.raster_sync_us_sum = self
            .raster_sync_us_sum
            .saturating_add(sample.raster_sync_us);
        self.wait_acquire_us_sum = self
            .wait_acquire_us_sum
            .saturating_add(sample.wait_acquire_us);
        self.record_us_sum = self.record_us_sum.saturating_add(sample.record_us);
        self.submit_us_sum = self.submit_us_sum.saturating_add(sample.submit_us);
        self.queue_present_us_sum = self
            .queue_present_us_sum
            .saturating_add(sample.queue_present_us);
        if sample.total_us > TARGET_120HZ_US {
            self.frames_over_120hz_budget = self.frames_over_120hz_budget.saturating_add(1);
        }
        if sample.total_us > TARGET_60HZ_US {
            self.frames_over_60hz_budget = self.frames_over_60hz_budget.saturating_add(1);
        }
        self.max_brush_instances = self.max_brush_instances.max(sample.brush_instances);
    }

    fn average(total: u64, frames: u64) -> u64 {
        if frames == 0 { 0 } else { total / frames }
    }

    pub(crate) fn debug_summary(&self) -> String {
        format!(
            concat!(
                "frames={} avg_total_us={} max_total_us={} ",
                "avg_stroke_us={} avg_raster_sync_us={} avg_wait_acquire_us={} ",
                "avg_record_us={} avg_submit_us={} avg_queue_present_us={} ",
                "over_120hz={} over_60hz={} max_instances={}"
            ),
            self.frames,
            Self::average(self.total_us_sum, self.frames),
            self.total_us_max,
            Self::average(self.stroke_us_sum, self.frames),
            Self::average(self.raster_sync_us_sum, self.frames),
            Self::average(self.wait_acquire_us_sum, self.frames),
            Self::average(self.record_us_sum, self.frames),
            Self::average(self.submit_us_sum, self.frames),
            Self::average(self.queue_present_us_sum, self.frames),
            self.frames_over_120hz_budget,
            self.frames_over_60hz_budget,
            self.max_brush_instances,
        )
    }
}

pub(crate) fn shared_frame_timing_stats() -> SharedFrameTimingStats {
    Arc::new(Mutex::new(FrameTimingStats::default()))
}

pub(crate) fn duration_us(duration: Duration) -> u64 {
    duration.as_micros().min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_stage_averages_maxima_and_budget_misses() {
        let mut stats = FrameTimingStats::default();
        stats.record(FrameTimingSample {
            total_us: 8_000,
            stroke_us: 1_000,
            raster_sync_us: 500,
            wait_acquire_us: 2_000,
            record_us: 1_500,
            submit_us: 500,
            queue_present_us: 2_500,
            brush_instances: 120,
        });
        stats.record(FrameTimingSample {
            total_us: 20_000,
            stroke_us: 2_000,
            raster_sync_us: 1_500,
            wait_acquire_us: 5_000,
            record_us: 4_000,
            submit_us: 1_000,
            queue_present_us: 6_500,
            brush_instances: 480,
        });

        assert_eq!(stats.frames, 2);
        assert_eq!(stats.total_us_sum, 28_000);
        assert_eq!(stats.total_us_max, 20_000);
        assert_eq!(stats.frames_over_120hz_budget, 1);
        assert_eq!(stats.frames_over_60hz_budget, 1);
        assert_eq!(stats.max_brush_instances, 480);
        assert!(stats.debug_summary().contains("avg_total_us=14000"));
        assert!(stats.debug_summary().contains("avg_record_us=2750"));
    }

    #[test]
    fn zero_frame_summary_does_not_divide_by_zero() {
        let summary = FrameTimingStats::default().debug_summary();
        assert!(summary.contains("frames=0"));
        assert!(summary.contains("avg_total_us=0"));
    }
}
