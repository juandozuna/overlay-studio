//! Score effect painter (FX-1): clock, pure sampler and the egui drawing of the overlays.
use crate::domain::fx::{Dir, FxStyle, SUB_STRENGTH};
use egui::{Color32, Painter, Pos2, Rect, Stroke, Vec2};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

pub const DOTS: usize = 10;
const PEAK_T: f32 = 0.1;
const POP: f32 = 0.4;
const SHAKE_PX: f32 = 14.;

struct Seen {
    id: u64,
    start: Option<Instant>,
    seconds: f32,
}

/// Remembers when each renderer first saw a new effect id for a value-key prefix.
#[derive(Default)]
pub struct FxClock {
    seen: Mutex<HashMap<String, Seen>>,
}
impl FxClock {
    /// Seconds since the effect for `prefix` started. The first id seen starts nothing, so a fresh renderer or project load stays calm.
    pub fn observe(&self, prefix: &str, id: u64, seconds: f32, now: Instant) -> Option<f32> {
        let mut map = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        let s = map.entry(prefix.to_string()).or_insert(Seen { id, start: None, seconds });
        if s.id != id {
            s.id = id;
            s.start = (id != 0).then_some(now);
        }
        s.seconds = seconds;
        s.start.map(|t| now.saturating_duration_since(t).as_secs_f32())
    }
    pub fn busy_at(&self, now: Instant) -> bool {
        let map = self.seen.lock().unwrap_or_else(|e| e.into_inner());
        map.values().any(|s| s.start.is_some_and(|t| now.saturating_duration_since(t).as_secs_f32() < s.seconds))
    }
    pub fn busy(&self) -> bool {
        self.busy_at(Instant::now())
    }
}

/// Plain numbers describing one instant of an effect. `Default` is the neutral (resting) frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FxFrame {
    pub tint: f32,
    pub scale: f32,
    pub offset_x: f32,
    pub ring_radius: f32,
    pub ring_alpha: f32,
    pub sweep_pos: f32,
    pub sweep_alpha: f32,
    pub wipe_from: f32,
    pub wipe_to: f32,
    /// Outward distance (0..1 of the half size) and alpha per sparkle dot.
    pub dots: [(f32, f32); DOTS],
}
impl Default for FxFrame {
    fn default() -> Self {
        Self {
            tint: 0.,
            scale: 1.,
            offset_x: 0.,
            ring_radius: 0.,
            ring_alpha: 0.,
            sweep_pos: 0.,
            sweep_alpha: 0.,
            wipe_from: 1.,
            wipe_to: 1.,
            dots: [(0., 0.); DOTS],
        }
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}

pub fn sample(style: FxStyle, kind_is_text: bool, dir: Dir, t: f32) -> FxFrame {
    let t = t.clamp(0., 1.);
    let mut f = FxFrame::default();
    if style == FxStyle::None || t >= 1. {
        return f;
    }
    let strength = if dir == Dir::Sub { SUB_STRENGTH } else { 1. };
    let envelope = if t < PEAK_T { t / PEAK_T } else { (1. - (t - PEAK_T) / (1. - PEAK_T)).powi(2) };
    let tint_gain = match style {
        FxStyle::Flash | FxStyle::Shake | FxStyle::Burst => 1.,
        FxStyle::Ring => 0.4,
        FxStyle::Sweep => 0.3,
        _ => 0.,
    };
    if !kind_is_text {
        f.tint = envelope * strength * tint_gain;
    }
    if kind_is_text && matches!(style, FxStyle::Flash | FxStyle::Ring) {
        f.scale = if t < 0.2 {
            1. + POP * smooth(t / 0.2)
        } else {
            let v = (t - 0.2) / 0.8;
            1. + POP * (1. - v).powi(2) * (1.5 * std::f32::consts::PI * v).cos()
        };
    }
    match style {
        FxStyle::Shake => f.offset_x = SHAKE_PX * (t * 40.).sin() * (1. - t).powi(2),
        FxStyle::Ring => {
            f.ring_radius = 1. - (1. - t).powi(3);
            f.ring_alpha = (1. - t) * strength;
        }
        FxStyle::Sweep => {
            f.sweep_pos = t;
            f.sweep_alpha = (std::f32::consts::PI * t).sin();
        }
        FxStyle::Wipe => {
            f.wipe_to = smooth(t / 0.6);
            f.wipe_from = smooth((t - 0.4) / 0.6);
        }
        FxStyle::Burst => {
            let out = 1. - (1. - t).powi(3);
            for (i, d) in f.dots.iter_mut().enumerate() {
                let stagger = 0.75 + 0.25 * ((i % 3) as f32 / 2.);
                *d = (out * stagger, (1. - t).powf(1.5));
            }
        }
        _ => {}
    }
    f
}

pub fn blend(base: [u8; 4], to: [u8; 3], k: f32) -> [u8; 4] {
    let k = k.clamp(0., 1.);
    let m = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * k).round() as u8;
    [m(base[0], to[0]), m(base[1], to[1]), m(base[2], to[2]), base[3]]
}

fn col(c: [u8; 3], a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], (255. * a.clamp(0., 1.)).round() as u8)
}

/// Draws the style overlays over `rect`, clipped to it.
pub fn paint_overlay(painter: &Painter, rect: Rect, style: FxStyle, f: &FxFrame, color: [u8; 3], dir: Dir, opacity: f32, scale: f32) {
    let p = painter.with_clip_rect(rect);
    let strength = if dir == Dir::Sub { SUB_STRENGTH } else { 1. };
    match style {
        FxStyle::Sweep if f.sweep_alpha > 0. => {
            let w = rect.width() * 0.25;
            let x = rect.left() - w + (rect.width() + 2. * w) * f.sweep_pos;
            let c = col([255, 255, 255], 0.55 * f.sweep_alpha * opacity);
            let tilt = rect.height() * 0.2;
            p.add(egui::Shape::convex_polygon(
                vec![
                    Pos2::new(x + tilt, rect.top()),
                    Pos2::new(x + tilt + w, rect.top()),
                    Pos2::new(x + w, rect.bottom()),
                    Pos2::new(x, rect.bottom()),
                ],
                c,
                Stroke::NONE,
            ));
        }
        FxStyle::Wipe if f.wipe_to > f.wipe_from => {
            let band = Rect::from_min_max(
                Pos2::new(rect.left() + rect.width() * f.wipe_from, rect.top()),
                Pos2::new(rect.left() + rect.width() * f.wipe_to, rect.bottom()),
            );
            p.rect_filled(band, 0., col(color, 0.9 * strength * opacity));
        }
        FxStyle::Ring if f.ring_alpha > 0. => {
            let max = rect.size().length() / 2.;
            p.circle_stroke(rect.center(), max * f.ring_radius, Stroke::new(6. * scale, col(color, f.ring_alpha * opacity)));
        }
        FxStyle::Burst => {
            let half = rect.size() / 2.;
            let size = (rect.height() * 0.06).max(2. * scale);
            for (i, (dist, a)) in f.dots.iter().enumerate() {
                if *a <= 0. {
                    continue;
                }
                let ang = i as f32 * std::f32::consts::TAU / DOTS as f32;
                let dir_v = Vec2::angled(ang);
                p.circle_filled(rect.center() + Vec2::new(dir_v.x * half.x, dir_v.y * half.y) * *dist, size, col(color, a * strength * opacity));
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const STYLES: [FxStyle; 6] = [FxStyle::Flash, FxStyle::Sweep, FxStyle::Wipe, FxStyle::Shake, FxStyle::Ring, FxStyle::Burst];

    #[test]
    fn neutral_at_both_ends() {
        for s in STYLES {
            for text in [false, true] {
                let end = sample(s, text, Dir::Add, 1.);
                assert_eq!(end, FxFrame::default(), "{s:?} end");
                assert_eq!(end.tint, 0.);
                assert_eq!(end.scale, 1.);
                assert_eq!(end.offset_x, 0.);
                let start = sample(s, text, Dir::Add, 0.);
                assert_eq!((start.tint, start.scale, start.offset_x), (0., 1., 0.), "{s:?} start");
            }
        }
        assert_eq!(sample(FxStyle::None, false, Dir::Add, 0.1), FxFrame::default());
    }

    #[test]
    fn tint_peaks_near_a_tenth() {
        let peak = sample(FxStyle::Flash, false, Dir::Add, 0.1).tint;
        assert!((peak - 1.).abs() < 1e-4);
        assert!(sample(FxStyle::Flash, false, Dir::Add, 0.5).tint < peak);
        assert!(sample(FxStyle::Flash, false, Dir::Add, 0.05).tint < peak);
        let best = (0..=100).map(|i| i as f32 / 100.).max_by(|a, b| sample(FxStyle::Flash, false, Dir::Add, *a).tint.total_cmp(&sample(FxStyle::Flash, false, Dir::Add, *b).tint)).unwrap();
        assert!((best - 0.1).abs() <= 0.02);
    }

    #[test]
    fn sub_peaks_at_sub_strength_and_dir_is_a_colour_choice() {
        let add = sample(FxStyle::Flash, false, Dir::Add, 0.1).tint;
        let sub = sample(FxStyle::Flash, false, Dir::Sub, 0.1).tint;
        assert!((add - 1.).abs() < 1e-4);
        assert!((sub - SUB_STRENGTH).abs() < 1e-4);
        assert_eq!(blend([100, 100, 100, 200], [0, 0, 0], 1.), [0, 0, 0, 200]);
        assert_eq!(blend([100, 100, 100, 200], [0x22, 0xC5, 0x5E], 0.), [100, 100, 100, 200]);
    }

    #[test]
    fn text_pops_with_overshoot_and_keeps_colour() {
        let pop = sample(FxStyle::Flash, true, Dir::Add, 0.2);
        assert!(pop.scale > 1.35 && pop.scale <= 1.45);
        assert_eq!(pop.tint, 0.);
        assert_eq!(sample(FxStyle::Flash, false, Dir::Add, 0.2).scale, 1.);
    }

    #[test]
    fn shake_stays_within_bounds_and_burst_has_dots() {
        assert!((0..=100).all(|i| sample(FxStyle::Shake, false, Dir::Add, i as f32 / 100.).offset_x.abs() <= SHAKE_PX));
        let b = sample(FxStyle::Burst, false, Dir::Add, 0.4);
        assert!(b.dots.iter().all(|(d, a)| *d > 0. && *a > 0.));
    }

    #[test]
    fn clock_ignores_first_id_restarts_on_change_and_expires() {
        let c = FxClock::default();
        let t0 = Instant::now();
        assert_eq!(c.observe("team.1", 5, 1.2, t0), None);
        assert!(!c.busy_at(t0));
        let t1 = t0 + std::time::Duration::from_millis(100);
        assert_eq!(c.observe("team.1", 6, 1.2, t1), Some(0.));
        let t2 = t1 + std::time::Duration::from_millis(500);
        let el = c.observe("team.1", 6, 1.2, t2).unwrap();
        assert!((el - 0.5).abs() < 1e-3);
        assert!(c.busy_at(t2));
        assert!(!c.busy_at(t1 + std::time::Duration::from_millis(1300)));
    }
}
