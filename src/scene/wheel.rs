//! Wheel painter (WN-1): pure spin math, per-renderer spin clock and the egui shapes.
use crate::domain::wheel::{Pointer, Slice, WheelConfig};
use egui::{
    Color32, FontFamily, FontId, Pos2, Shape, Stroke, Vec2,
    emath::Rot2,
    epaint::{Mesh, Vertex, WHITE_UV},
};
use std::{collections::BTreeMap, f32::consts::TAU, sync::Mutex, time::Instant};

const CRAWL_TIME: f32 = 0.1;
const CRAWL_TRAVEL: f32 = 0.015;
const TAIL_SECONDS: f32 = 3.;

pub fn step(n: usize) -> f32 {
    TAU / n.max(1) as f32
}
/// Wheel angle (clockwise from 3 o'clock) that puts the middle of `winner`, plus `jitter` of a slice, under the pointer.
pub fn landing_angle(winner: usize, n: usize, jitter: f32, turns: u32) -> f32 {
    if n == 0 {
        return 0.;
    }
    (-(winner as f32 + 0.5 + jitter) * step(n)).rem_euclid(TAU) + turns as f32 * TAU
}
pub fn rest_angle(cfg: &WheelConfig) -> f32 {
    cfg.winner_index
        .filter(|w| *w < cfg.entries.len())
        .map(|w| landing_angle(w, cfg.entries.len(), cfg.spin_jitter, 0))
        .unwrap_or(0.)
}
/// Final angle of a spin that starts at `from`, lands on `rest` and adds `turns` full turns.
pub fn spin_end(from: f32, rest: f32, turns: u32) -> f32 {
    from + turns as f32 * TAU + (rest - from).rem_euclid(TAU)
}
/// Quintic ease-out; the last 10% of time covers the last 1.5% of travel as a slowing crawl.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    let main_end = 1. - CRAWL_TIME;
    if t < main_end {
        let u = t / main_end;
        (1. - CRAWL_TRAVEL) * (1. - (1. - u).powi(5))
    } else {
        let s = (t - main_end) / CRAWL_TIME;
        1. - CRAWL_TRAVEL + CRAWL_TRAVEL * (1. - (1. - s) * (1. - s))
    }
}
/// Slice under the pointer for a wheel turned by `angle`.
pub fn slice_at(angle: f32, n: usize) -> usize {
    ((-angle).rem_euclid(TAU) / step(n)) as usize % n.max(1)
}
/// Angle of the middle of slice `i` in wheel space (before the wheel turns).
pub fn label_angle(i: usize, n: usize) -> f32 {
    (i as f32 + 0.5) * step(n)
}
/// Pointer deflection 0..=1: pushed up by an approaching peg, then snapping back once it passes.
pub fn flick(angle: f32, n: usize) -> f32 {
    let x = (angle / step(n)).rem_euclid(1.);
    if x > 0.75 {
        (x - 0.75) / 0.25
    } else {
        (1. - x / 0.3).max(0.)
    }
}

#[derive(Default)]
struct ClockState {
    id: Option<u64>,
    start: Option<Instant>,
    from: f32,
    last: f32,
    total: f32,
}
/// Remembers when this renderer first saw each `wheel.spin_id`; ids seen at startup do not animate.
#[derive(Default)]
pub struct WheelClock(Mutex<ClockState>);
impl WheelClock {
    /// Seconds since the current spin began and the angle it began from, if one is running or finished recently.
    fn observe(&self, cfg: &WheelConfig) -> Option<(f32, f32)> {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        match s.id {
            None => s.id = Some(cfg.spin_id),
            Some(id) if id != cfg.spin_id => {
                s.id = Some(cfg.spin_id);
                s.start = (cfg.spin_id != 0).then(Instant::now);
                s.from = s.last;
            }
            _ => {}
        }
        s.total = cfg.spin_seconds + TAIL_SECONDS;
        s.start.map(|t| (t.elapsed().as_secs_f32(), s.from))
    }
    fn remember(&self, angle: f32) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).last = angle;
    }
    /// True while a spin, its winner glow or its confetti still needs frames.
    pub fn busy(&self) -> bool {
        let s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        s.start.is_some_and(|t| t.elapsed().as_secs_f32() < s.total)
    }
}

struct Ctx<'a> {
    painter: &'a egui::Painter,
    c: Pos2,
    r: f32,
    rot: f32,
    alpha: f32,
}
impl Ctx<'_> {
    fn col(&self, c: [u8; 3], a: f32) -> Color32 {
        Color32::from_rgba_unmultiplied(c[0], c[1], c[2], (255. * a * self.alpha).round() as u8)
    }
    fn polar(&self, angle: f32, radius: f32) -> Pos2 {
        self.c + Vec2::angled(angle + self.rot) * radius
    }
    fn local(&self, p: Vec2) -> Pos2 {
        self.c + Rot2::from_angle(self.rot) * p
    }
    fn sector(&self, a0: f32, a1: f32, r0: f32, r1: f32, c0: Color32, c1: Color32) {
        let segs = ((a1 - a0) / (TAU / 120.)).ceil().max(1.) as u32;
        let mut mesh = Mesh::default();
        for j in 0..=segs {
            let a = a0 + (a1 - a0) * j as f32 / segs as f32;
            for (r, color) in [(r0, c0), (r1, c1)] {
                mesh.vertices.push(Vertex { pos: self.polar(a, r), uv: WHITE_UV, color });
            }
            if j > 0 {
                let b = (j - 1) * 2;
                mesh.indices.extend([b, b + 1, b + 2, b + 1, b + 3, b + 2]);
            }
        }
        self.painter.add(Shape::mesh(mesh));
    }
}
fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    std::array::from_fn(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t.clamp(0., 1.)).round() as u8)
}
fn is_light(c: [u8; 3]) -> bool {
    0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32 > 150.
}
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

pub fn paint(
    painter: &egui::Painter,
    rect: egui::Rect,
    rotation: f32,
    opacity: f32,
    values: &BTreeMap<String, String>,
    clock: &WheelClock,
) {
    let cfg = WheelConfig::read(values);
    let n = cfg.entries.len();
    let r = rect.width().min(rect.height()) / 2. * 0.9;
    if r < 2. {
        return;
    }
    let ctx = Ctx { painter, c: rect.center(), r, rot: rotation, alpha: opacity.clamp(0., 1.) };
    let ink = [20, 22, 32];
    let spin = clock.observe(&cfg);
    let rest = rest_angle(&cfg);
    let (angle, spinning, landed) = match spin {
        Some((el, from)) if n > 0 && el < cfg.spin_seconds => {
            (from + (spin_end(from, rest, cfg.spin_turns) - from) * ease(el / cfg.spin_seconds), true, None)
        }
        Some((el, _)) => (rest, false, Some(el - cfg.spin_seconds)),
        None => (rest, false, None),
    };
    clock.remember(angle);
    let stroke_w = (r * 0.012).max(1.);
    let hub = r * 0.13;
    let rim = r * 0.95;
    if n == 0 {
        painter.circle_stroke(ctx.c, r, Stroke::new(r * 0.04, ctx.col([150, 156, 175], 0.8)));
        let galley = painter.layout_no_wrap("Add entries".into(), FontId::new(r * 0.14, FontFamily::Proportional), ctx.col([200, 204, 220], 0.9));
        let pos = ctx.c - galley.size() / 2.;
        painter.add(egui::epaint::TextShape::new(pos, galley, Color32::WHITE).with_angle_and_anchor(rotation, egui::Align2::CENTER_CENTER));
        return;
    }
    let step = step(n);
    let glow = landed
        .filter(|t| *t >= 0. && *t < TAIL_SECONDS)
        .map(|t| (1. - t / TAIL_SECONDS) * (0.55 + 0.45 * (t * 7.).sin()));
    for i in 0..n {
        let mut color = cfg.slice_color(i);
        if let (Some(g), Some(w)) = (glow, cfg.winner_index) {
            if w == i {
                color = mix(color, [255, 255, 255], 0.55 * g.max(0.));
            }
        }
        let a0 = angle + i as f32 * step;
        let (inner, outer) = match cfg.slice {
            Slice::Gradient => (ctx.col(mix(color, ink, 0.55), 1.), ctx.col(color, 1.)),
            _ => (ctx.col(color, 1.), ctx.col(color, 1.)),
        };
        ctx.sector(a0, a0 + step, 0., rim, inner, outer);
    }
    let gap = Stroke::new(stroke_w * 1.5, ctx.col(ink, 0.85));
    for i in 0..n {
        let a = angle + i as f32 * step;
        painter.line_segment([ctx.polar(a, hub), ctx.polar(a, rim)], gap);
    }
    ctx.sector(0., TAU, rim, r, ctx.col([34, 38, 56], 1.), ctx.col([22, 25, 40], 1.));
    painter.circle_stroke(ctx.c, r, Stroke::new(stroke_w, ctx.col([230, 232, 245], 0.9)));
    for i in 0..n {
        let a = angle + i as f32 * step;
        painter.circle_filled(ctx.polar(a, (rim + r) / 2.), r * 0.018, ctx.col([245, 247, 255], 1.));
    }
    let max_w = r * 0.58;
    let max_h = (2. * (step / 2.).sin() * r * 0.72 * 0.78).min(r * 0.2);
    for (i, label) in cfg.entries.iter().enumerate() {
        let a = angle + label_angle(i, n);
        let color = cfg.slice_color(i);
        let text_color = ctx.col(if is_light(color) { ink } else { [250, 250, 252] }, 1.);
        let mut font = FontId::new(max_h.max(6.), FontFamily::Proportional);
        let mut galley = painter.layout_no_wrap(label.clone(), font.clone(), text_color);
        let fit = (max_w / galley.size().x.max(1.)).min(max_h / galley.size().y.max(1.)).min(1.);
        if fit < 1. {
            font.size *= fit;
            galley = painter.layout_no_wrap(label.clone(), font, text_color);
        }
        let mid = ctx.polar(a, rim * 0.93 - galley.size().x / 2.);
        painter.add(
            egui::epaint::TextShape::new(mid - galley.size() / 2., galley, text_color)
                .with_angle_and_anchor(a + rotation, egui::Align2::CENTER_CENTER),
        );
    }
    painter.circle_filled(ctx.c, hub, ctx.col([34, 38, 56], 1.));
    painter.circle_stroke(ctx.c, hub, Stroke::new(stroke_w * 2., ctx.col([245, 247, 255], 1.)));
    painter.circle_filled(ctx.c, hub * 0.35, ctx.col([245, 247, 255], 1.));
    let deflect = if spinning { flick(angle, n) } else { 0. };
    paint_pointer(&ctx, cfg.pointer, -deflect * 0.5);
    if let (true, Some(t)) = (cfg.confetti, landed) {
        paint_confetti(&ctx, &cfg, t);
    }
}

fn paint_pointer(ctx: &Ctx, pointer: Pointer, turn: f32) {
    let r = ctx.r;
    let pivot = Vec2::new(r * 1.07, 0.);
    let at = |x: f32, y: f32| {
        let v = Rot2::from_angle(turn) * (Vec2::new(x, y) * r - pivot);
        ctx.local(pivot + v)
    };
    let outline = Stroke::new((r * 0.012).max(1.), ctx.col([20, 22, 32], 1.));
    let painter = ctx.painter;
    match pointer {
        Pointer::Arrow => {
            painter.add(Shape::convex_polygon(
                vec![at(0.8, 0.), at(1.1, -0.085), at(1.1, 0.085)],
                ctx.col([250, 250, 252], 1.),
                outline,
            ));
        }
        Pointer::Pin => {
            painter.add(Shape::convex_polygon(
                vec![at(0.82, 0.), at(1.04, -0.05), at(1.04, 0.05)],
                ctx.col([232, 80, 91], 1.),
                outline,
            ));
            painter.circle_filled(at(1.07, 0.), r * 0.065, ctx.col([232, 80, 91], 1.));
            painter.circle_stroke(at(1.07, 0.), r * 0.065, outline);
        }
        Pointer::Bar => {
            painter.add(Shape::convex_polygon(
                vec![at(0.84, -0.03), at(1.1, -0.03), at(1.1, 0.03), at(0.84, 0.03)],
                ctx.col([250, 250, 252], 1.),
                outline,
            ));
        }
    }
}

fn paint_confetti(ctx: &Ctx, cfg: &WheelConfig, t: f32) {
    const LIFE: f32 = 2.5;
    if t < 0. || t > LIFE {
        return;
    }
    let r = ctx.r;
    let mut rng = Lcg(cfg.spin_id.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xC0FF_EE);
    let fade = 1. - (t / LIFE).powi(2);
    for _ in 0..90 {
        let dir = -std::f32::consts::FRAC_PI_2 + (rng.next() - 0.5) * 1.9;
        let speed = (0.8 + rng.next() * 1.3) * r;
        let spin = (rng.next() - 0.5) * 18.;
        let phase = rng.next() * TAU;
        let color = cfg.slice_color((rng.next() * 8.) as usize);
        let size = r * (0.025 + rng.next() * 0.025);
        let pos = ctx.c + Vec2::angled(dir) * speed * t + Vec2::new(0., 0.5 * 2.2 * r * t * t);
        let rot = Rot2::from_angle(phase + spin * t);
        let (w, h) = (size, size * 0.5 * (1. + (t * 9. + phase).sin().abs()));
        let pts = [(-w, -h), (w, -h), (w, h), (-w, h)].map(|(x, y)| pos + rot * Vec2::new(x, y));
        ctx.painter.add(Shape::convex_polygon(pts.to_vec(), ctx.col(color, fade), Stroke::NONE));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;
    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }
    #[test]
    fn landing_puts_winner_middle_under_pointer() {
        for n in [1, 3, 8, 13] {
            for w in 0..n {
                let a = landing_angle(w, n, 0., 6);
                assert_eq!(slice_at(a, n), w);
                assert!(close((a - 6. * TAU).rem_euclid(TAU), (-label_angle(w, n)).rem_euclid(TAU)));
            }
        }
    }
    #[test]
    fn landing_adds_turns_and_jitter() {
        assert!(close(landing_angle(2, 4, 0., 5) - landing_angle(2, 4, 0., 0), 5. * TAU));
        let off = landing_angle(1, 4, 0.25, 0);
        let mid = landing_angle(1, 4, 0., 0);
        assert!(close((mid - off).rem_euclid(TAU), 0.25 * PI / 2.));
        assert_eq!(slice_at(off, 4), 1);
        assert_eq!(landing_angle(0, 0, 0., 6), 0.);
    }
    #[test]
    fn spin_end_travels_forward_at_least_the_turns() {
        let rest = landing_angle(3, 8, 0.1, 0);
        for from in [0., 1.3, 5.9, rest] {
            let end = spin_end(from, rest, 6);
            assert!(end - from >= 6. * TAU - 1e-4 && end - from < 7. * TAU + 1e-4);
            assert!(close((end - rest).rem_euclid(TAU).min(TAU - (end - rest).rem_euclid(TAU)), 0.));
        }
    }
    #[test]
    fn ease_endpoints_monotonic_and_crawls() {
        assert_eq!(ease(0.), 0.);
        assert!(close(ease(1.), 1.));
        assert!(close(ease(0.9), 1. - CRAWL_TRAVEL));
        let mut prev = 0.;
        for i in 1..=100 {
            let v = ease(i as f32 / 100.);
            assert!(v >= prev);
            prev = v;
        }
        assert_eq!((ease(-1.), ease(2.)), (0., 1.));
    }
    #[test]
    fn labels_cover_slices_evenly() {
        let n = 5;
        for i in 0..n {
            assert!(close(label_angle(i, n), (i as f32 + 0.5) * TAU / 5.));
            assert_eq!(slice_at(-label_angle(i, n), n), i);
        }
        assert_eq!(slice_at(0.0001, n), n - 1);
    }
    #[test]
    fn flick_is_bounded() {
        for i in 0..200 {
            let f = flick(i as f32 * 0.07, 7);
            assert!((0. ..=1.).contains(&f));
        }
    }
}
