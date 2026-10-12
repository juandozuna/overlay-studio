use std::time::{Duration, Instant};

/// Keep an absolute frame deadline so a coarse UI timer does not lower the
/// configured frame rate. Missed deadlines are skipped without a catch-up burst.
pub(super) struct FramePacer {
    next: Instant,
    interval: Duration,
}
impl FramePacer {
    pub fn new(fps: u32, now: Instant) -> Self {
        Self {
            next: now,
            interval: Duration::from_secs_f64(1. / fps as f64),
        }
    }
    pub fn due(&mut self, now: Instant) -> bool {
        if now < self.next {
            return false;
        }
        let intervals = now.duration_since(self.next).as_nanos() / self.interval.as_nanos() + 1;
        self.next += self.interval.mul_f64(intervals as f64);
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coarse_timer_retains_requested_average_rate() {
        for fps in [24, 25, 30, 50, 60] {
            let start = Instant::now();
            let mut pacer = FramePacer::new(fps, start);
            let frames = (0..=125)
                .filter(|tick| pacer.due(start + Duration::from_millis(tick * 8)))
                .count();
            assert!(
                (fps as usize..=fps as usize + 1).contains(&frames),
                "{fps} fps produced {frames} frames"
            );
        }
    }
    #[test]
    fn long_pause_skips_missed_frames_without_burst() {
        let start = Instant::now();
        let mut pacer = FramePacer::new(30, start);
        let later = start + Duration::from_secs(60);
        assert!(pacer.due(later));
        assert!(!pacer.due(later));
    }
}
