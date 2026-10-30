//! Dice painter (El Dado): quaternion tumble, per-renderer roll clock and the egui shapes of a lit 3D cube.
use super::wheel;
use crate::domain::dice::{DiceConfig, Face};
use egui::{
    Color32, FontFamily, FontId, Galley, Pos2, Shape, Stroke, Vec2,
    emath::Rot2,
    epaint::{Mesh, Vertex},
};
use std::{
    collections::BTreeMap,
    f32::consts::{FRAC_PI_2, PI, TAU},
    sync::Mutex,
    time::Instant,
};

/// Camera distance in cube edges (pinhole on the +z axis, the viewer looks down -z).
pub const CAMERA: f32 = 3.2;
/// Resting tilt of the landed face (radians): a little pitch and yaw so three faces stay visible.
const REST_PITCH: f32 = -12. * PI / 180.;
const REST_YAW: f32 = 18. * PI / 180.;
/// Direction towards the light, in view space (upper left, in front).
const LIGHT: [f32; 3] = [-0.395, -0.543, 0.741];

/// Unit quaternion (x, y, z, w). Axes follow the screen: x right, y down, z towards the viewer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}
impl Quat {
    pub const IDENTITY: Quat = Quat { x: 0., y: 0., z: 0., w: 1. };
    pub fn axis_angle(axis: [f32; 3], angle: f32) -> Quat {
        let len = dot(axis, axis).sqrt().max(1e-6);
        let s = (angle / 2.).sin() / len;
        Quat { x: axis[0] * s, y: axis[1] * s, z: axis[2] * s, w: (angle / 2.).cos() }
    }
    /// Hamilton product: applies `o` first, then `self`.
    pub fn mul(self, o: Quat) -> Quat {
        Quat {
            x: self.w * o.x + self.x * o.w + self.y * o.z - self.z * o.y,
            y: self.w * o.y - self.x * o.z + self.y * o.w + self.z * o.x,
            z: self.w * o.z + self.x * o.y - self.y * o.x + self.z * o.w,
            w: self.w * o.w - self.x * o.x - self.y * o.y - self.z * o.z,
        }
    }
    pub fn conj(self) -> Quat {
        Quat { x: -self.x, y: -self.y, z: -self.z, w: self.w }
    }
    pub fn normalized(self) -> Quat {
        let l = (self.x * self.x + self.y * self.y + self.z * self.z + self.w * self.w).sqrt().max(1e-6);
        Quat { x: self.x / l, y: self.y / l, z: self.z / l, w: self.w / l }
    }
    fn dot(self, o: Quat) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z + self.w * o.w
    }
    /// Shortest-arc spherical interpolation.
    pub fn slerp(self, to: Quat, t: f32) -> Quat {
        let mut d = self.dot(to);
        let to = if d < 0. {
            d = -d;
            Quat { x: -to.x, y: -to.y, z: -to.z, w: -to.w }
        } else {
            to
        };
        let (a, b) = if d > 0.9995 {
            (1. - t, t)
        } else {
            let th = d.clamp(-1., 1.).acos();
            let s = th.sin();
            (((1. - t) * th).sin() / s, (t * th).sin() / s)
        };
        Quat { x: self.x * a + to.x * b, y: self.y * a + to.y * b, z: self.z * a + to.z * b, w: self.w * a + to.w * b }.normalized()
    }
    pub fn rotate(self, v: [f32; 3]) -> [f32; 3] {
        let Quat { x, y, z, w } = self;
        [
            (1. - 2. * (y * y + z * z)) * v[0] + 2. * (x * y - z * w) * v[1] + 2. * (x * z + y * w) * v[2],
            2. * (x * y + z * w) * v[0] + (1. - 2. * (x * x + z * z)) * v[1] + 2. * (y * z - x * w) * v[2],
            2. * (x * z - y * w) * v[0] + 2. * (y * z + x * w) * v[1] + (1. - 2. * (x * x + y * y)) * v[2],
        ]
    }
}
impl Default for Quat {
    fn default() -> Self {
        Quat::IDENTITY
    }
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn madd(a: [f32; 3], b: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k]
}

/// Rotation that carries the cube's face `i` (in cube space it points along +z) onto its own outward normal.
fn face_quat(i: usize) -> Quat {
    const X: [f32; 3] = [1., 0., 0.];
    const Y: [f32; 3] = [0., 1., 0.];
    match i {
        0 => Quat::IDENTITY,
        1 => Quat::axis_angle(Y, FRAC_PI_2),
        2 => Quat::axis_angle(Y, PI),
        3 => Quat::axis_angle(Y, -FRAC_PI_2),
        4 => Quat::axis_angle(X, FRAC_PI_2),
        _ => Quat::axis_angle(X, -FRAC_PI_2),
    }
}
/// Outward normal and in-face right / down axes of face `i`, in cube space.
fn face_axes(i: usize) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let f = face_quat(i);
    (f.rotate([0., 0., 1.]), f.rotate([1., 0., 0.]), f.rotate([0., 1., 0.]))
}
/// Still three-quarter view shown before the first roll.
pub fn idle_orientation() -> Quat {
    Quat::axis_angle([1., 0., 0.], -0.5).mul(Quat::axis_angle([0., 1., 0.], 0.75))
}
/// Orientation that turns face `face` towards the viewer, with the fixed resting tilt.
pub fn rest_orientation(face: usize) -> Quat {
    let tilt = Quat::axis_angle([1., 0., 0.], REST_PITCH).mul(Quat::axis_angle([0., 1., 0.], REST_YAW));
    tilt.mul(face_quat(face.min(5)).conj()).normalized()
}
/// Face whose outward normal points most at the viewer when the cube is turned by `q`.
pub fn facing_face(q: Quat) -> usize {
    (0..6)
        .map(|i| (i, q.rotate(face_axes(i).0)[2]))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Quintic ease-out with a slowing crawl at the end, the same curve as the wheel.
pub fn ease(t: f32) -> f32 {
    wheel::ease(t)
}
struct SplitMix(u64);
impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
}
/// Axis and total angle of the extra turns of roll `roll_id`; every renderer derives the same pair.
pub fn spin_for(roll_id: u64) -> ([f32; 3], f32) {
    let mut rng = SplitMix(roll_id.wrapping_mul(0xD1B5_4A32_D192_ED03) ^ 0xD1CE);
    let mut axis = [rng.unit() * 2. - 1., rng.unit() * 2. - 1., rng.unit() * 2. - 1.];
    if dot(axis, axis) < 0.09 {
        axis = [1., 0.4, 0.2];
    }
    (axis, (2 + (rng.next() % 2)) as f32 * TAU)
}
/// Orientation `t` (0..=1 of the roll) into roll `roll_id`, starting from `from` and landing exactly on `face`.
pub fn orientation_at(from: Quat, roll_id: u64, face: usize, t: f32) -> Quat {
    let rest = rest_orientation(face);
    if t >= 1. {
        return rest;
    }
    let e = ease(t);
    let (axis, angle) = spin_for(roll_id);
    Quat::axis_angle(axis, angle * (1. - e)).mul(from.slerp(rest, e)).normalized()
}

/// Pinhole projection of a view-space point; `scale` is pixels per cube edge on the z = 0 plane.
pub fn project(p: [f32; 3], scale: f32, centre: Pos2) -> Pos2 {
    let k = scale * CAMERA / (CAMERA - p[2]).max(0.05);
    Pos2::new(centre.x + p[0] * k, centre.y + p[1] * k)
}

#[derive(Default)]
struct ClockState {
    id: Option<u64>,
    start: Option<Instant>,
    from: Option<Quat>,
    last: Option<Quat>,
    total: f32,
}
/// Remembers when this renderer first saw each `dice.roll_id`; ids seen at startup do not animate.
#[derive(Default)]
pub struct DiceClock(Mutex<ClockState>);
impl DiceClock {
    /// Seconds since the current roll began and the orientation it began from, if one is running or finished recently.
    fn observe(&self, cfg: &DiceConfig) -> Option<(f32, Quat)> {
        let mut s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        match s.id {
            None => s.id = Some(cfg.roll_id),
            Some(id) if id != cfg.roll_id => {
                s.id = Some(cfg.roll_id);
                s.start = (cfg.roll_id != 0).then(Instant::now);
                s.from = s.last;
            }
            _ => {}
        }
        s.total = cfg.roll_seconds;
        let from = s.from.unwrap_or_else(idle_orientation);
        s.start.map(|t| (t.elapsed().as_secs_f32(), from))
    }
    fn remember(&self, q: Quat) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).last = Some(q);
    }
    /// True while a roll still needs frames.
    pub fn busy(&self) -> bool {
        let s = self.0.lock().unwrap_or_else(|e| e.into_inner());
        s.start.is_some_and(|t| t.elapsed().as_secs_f32() < s.total)
    }
}

/// Where the cube sits on screen: projection centre, scale and the layer rotation around the box centre.
struct View {
    pivot: Pos2,
    c: Pos2,
    scale: f32,
    rot: Rot2,
    alpha: f32,
}
impl View {
    fn spin(&self, p: Pos2) -> Pos2 {
        self.pivot + self.rot * (p - self.pivot)
    }
    fn point(&self, p: [f32; 3]) -> Pos2 {
        self.spin(project(p, self.scale, self.c))
    }
    fn col(&self, c: [u8; 3], a: f32) -> Color32 {
        Color32::from_rgba_unmultiplied(c[0], c[1], c[2], (255. * a * self.alpha).round() as u8)
    }
}
#[derive(Clone, Copy)]
struct Quad {
    face: usize,
    depth: f32,
    lum: f32,
    centre: [f32; 3],
    u: [f32; 3],
    v: [f32; 3],
    pts: [Pos2; 4],
}
fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    std::array::from_fn(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t.clamp(0., 1.)).round() as u8)
}
fn is_light(c: [u8; 3]) -> bool {
    0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32 > 150.
}
const CORNERS: [(f32, f32); 4] = [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];

/// Faces turned towards the camera when the cube is rotated by `q`, farthest first. `at` projects a view-space point.
fn visible_faces(q: Quat, at: &impl Fn([f32; 3]) -> Pos2) -> ([Quad; 6], usize) {
    let blank = Quad { face: 0, depth: 0., lum: 0., centre: [0.; 3], u: [0.; 3], v: [0.; 3], pts: [Pos2::ZERO; 4] };
    let mut out = [blank; 6];
    let mut n_vis = 0;
    for i in 0..6 {
        let (n, u, v) = face_axes(i);
        let (n, u, v) = (q.rotate(n), q.rotate(u), q.rotate(v));
        // the face is seen from the front when the camera is on its outer side
        if CAMERA * n[2] - 0.5 <= 0. {
            continue;
        }
        let centre = [n[0] * 0.5, n[1] * 0.5, n[2] * 0.5];
        let pts = CORNERS.map(|(a, b)| at(madd(madd(centre, u, a), v, b)));
        out[n_vis] = Quad { face: i, depth: centre[2], lum: dot(n, LIGHT).max(0.), centre, u, v, pts };
        n_vis += 1;
    }
    out[..n_vis].sort_by(|a, b| a.depth.total_cmp(&b.depth));
    (out, n_vis)
}
/// Projected area of face `face` (0 when it is turned away), used by tests and checks.
pub fn face_area(q: Quat, face: usize, scale: f32, centre: Pos2) -> f32 {
    let (quads, n) = visible_faces(q, &|p| project(p, scale, centre));
    quads[..n].iter().find(|f| f.face == face).map_or(0., |f| polygon_area(&f.pts))
}
fn polygon_area(p: &[Pos2; 4]) -> f32 {
    (0..4).map(|i| p[i].x * p[(i + 1) % 4].y - p[(i + 1) % 4].x * p[i].y).sum::<f32>().abs() / 2.
}

/// Lays `galley` flat on a face: its centre goes to face coordinates `at` (cube edges) and its glyph
/// vertices follow the face through `map`, so the text is foreshortened with the face.
fn add_text(mesh: &mut Mesh, galley: &Galley, uv_norm: Vec2, unit: f32, at: (f32, f32), map: &impl Fn(f32, f32) -> Pos2) {
    let half = galley.size() / 2.;
    for row in &galley.rows {
        let base = mesh.vertices.len() as u32;
        for v in &row.visuals.mesh.vertices {
            let p = row.pos.to_vec2() + v.pos.to_vec2() - half;
            mesh.vertices.push(Vertex { pos: map(at.0 + p.x / unit, at.1 + p.y / unit), uv: (v.uv.to_vec2() * uv_norm).to_pos2(), color: v.color });
        }
        mesh.indices.extend(row.visuals.mesh.indices.iter().map(|i| i + base));
    }
}

pub fn paint(
    painter: &egui::Painter,
    rect: egui::Rect,
    rotation: f32,
    opacity: f32,
    values: &BTreeMap<String, String>,
    clock: &DiceClock,
) {
    let cfg = DiceConfig::read(values);
    let side = rect.width().min(rect.height());
    if side < 8. {
        return;
    }
    let spin = clock.observe(&cfg);
    let q = match (spin, cfg.face) {
        (Some((el, from)), Some(face)) if el < cfg.roll_seconds => orientation_at(from, cfg.roll_id, face.index(), el / cfg.roll_seconds),
        (_, Some(face)) => rest_orientation(face.index()),
        _ => idle_orientation(),
    };
    clock.remember(q);
    let scale = side * 0.36;
    let view = View {
        pivot: rect.center(),
        c: rect.center() - Vec2::new(0., side * 0.06),
        scale,
        rot: Rot2::from_angle(rotation),
        alpha: opacity.clamp(0., 1.),
    };
    // soft contact shadow: three stacked ellipses, the smallest darkest
    for (k, a) in [(1., 0.10), (0.78, 0.12), (0.56, 0.14)] {
        let (rx, ry) = (scale * 0.7 * k, scale * 0.14 * k);
        let mid = view.c + Vec2::new(0., scale * 0.92);
        let ring: Vec<Pos2> = (0..28).map(|i| view.spin(mid + Vec2::angled(i as f32 * TAU / 28.) * Vec2::new(rx, ry))).collect();
        painter.add(Shape::convex_polygon(ring, view.col([0, 0, 0], a), Stroke::NONE));
    }
    let (quads, n) = visible_faces(q, &|p| view.point(p));
    let ink = [20, 22, 32];
    let edge = (scale * 0.02).max(1.);
    for f in &quads[..n] {
        let base = cfg.colors[f.face];
        let shade = 0.62 + 0.38 * f.lum;
        let body = mix([0; 3], base, shade);
        painter.add(Shape::convex_polygon(f.pts.to_vec(), view.col(body, 1.), Stroke::new(edge, view.col(mix(body, [255; 3], 0.55), 0.95))));
        let mid = (f.pts.iter().fold(Vec2::ZERO, |a, p| a + p.to_vec2()) / 4.).to_pos2();
        let inset: Vec<Pos2> = f.pts.iter().map(|p| *p + (mid - *p) * 0.09).collect();
        painter.add(Shape::closed_line(inset, Stroke::new((edge * 0.5).max(1.), view.col(mix(body, [255; 3], 0.35), 0.55))));
    }
    let code_px = scale * 0.30;
    let name_px = scale * 0.095;
    if name_px < 4. {
        return;
    }
    let uv_norm = {
        let [w, h] = painter.ctx().fonts(|f| f.font_image_size());
        Vec2::new(1. / w.max(1) as f32, 1. / h.max(1) as f32)
    };
    let mut text = Mesh::default();
    for f in &quads[..n] {
        let face = Face::ALL[f.face];
        let color = if is_light(cfg.colors[f.face]) { ink } else { [250, 250, 252] };
        let map = |a: f32, b: f32| view.point(madd(madd(f.centre, f.u, a), f.v, b));
        let code = painter.layout_no_wrap(face.code().into(), FontId::new(code_px, FontFamily::Proportional), view.col(color, 1.));
        add_text(&mut text, &code, uv_norm, scale, (0., -0.04), &map);
        let name = painter.layout_no_wrap(face.name().into(), FontId::new(name_px, FontFamily::Proportional), view.col(color, 0.8));
        add_text(&mut text, &name, uv_norm, scale, (0., 0.2), &map);
    }
    if !text.is_empty() {
        painter.add(Shape::mesh(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }
    fn same(a: Quat, b: Quat) -> bool {
        a.dot(b).abs() > 1. - 1e-5
    }
    #[test]
    fn rest_puts_chosen_face_towards_the_viewer() {
        for i in 0..6 {
            let q = rest_orientation(i);
            assert_eq!(facing_face(q), i);
            let n = q.rotate(face_axes(i).0);
            assert!(n[2] > 0.9, "face {i} normal z {}", n[2]);
        }
    }
    #[test]
    fn chosen_face_has_the_largest_projected_area_at_rest() {
        let centre = Pos2::new(100., 100.);
        for i in 0..6 {
            let q = rest_orientation(i);
            let areas: Vec<f32> = (0..6).map(|f| face_area(q, f, 100., centre)).collect();
            let best = areas.iter().cloned().fold(0., f32::max);
            assert!(areas[i] > 0. && close(areas[i], best), "face {i}: {areas:?}");
            assert!(areas.iter().filter(|a| **a > 0.).count() >= 3, "face {i}: {areas:?}");
        }
    }
    #[test]
    fn project_centres_and_shrinks_with_distance() {
        let c = Pos2::new(50., 40.);
        assert_eq!(project([0.; 3], 100., c), c);
        let near = project([0.5, 0., 1.], 100., c).x - c.x;
        let far = project([0.5, 0., -1.], 100., c).x - c.x;
        assert!(near > 50. && far < 50. && far > 0.);
    }
    #[test]
    fn ease_endpoints_and_monotonic() {
        assert_eq!(ease(0.), 0.);
        assert!(close(ease(1.), 1.));
        let mut prev = 0.;
        for i in 1..=200 {
            let v = ease(i as f32 / 200.);
            assert!(v >= prev);
            prev = v;
        }
    }
    #[test]
    fn tumble_lands_exactly_and_stays_unit_length() {
        let from = idle_orientation();
        for face in 0..6 {
            assert!(same(orientation_at(from, 7, face, 1.), rest_orientation(face)));
            assert!(same(orientation_at(from, 7, face, 2.), rest_orientation(face)));
            for k in 0..=20 {
                let q = orientation_at(from, 7, face, k as f32 / 20.);
                assert!(close(q.dot(q), 1.));
            }
        }
        assert!(same(orientation_at(from, 7, 2, 0.), from));
    }
    #[test]
    fn tumble_is_deterministic_per_roll_id_and_differs_between_rolls() {
        let from = idle_orientation();
        for k in 0..=10 {
            let t = k as f32 / 10.;
            assert_eq!(orientation_at(from, 41, 3, t), orientation_at(from, 41, 3, t));
        }
        assert_eq!(spin_for(41), spin_for(41));
        assert_ne!(spin_for(41), spin_for(42));
        let (axis, angle) = spin_for(5);
        assert!(dot(axis, axis) >= 0.09 && (close(angle, 2. * TAU) || close(angle, 3. * TAU)));
    }
    #[test]
    fn two_clocks_see_the_same_roll() {
        let mut cfg = DiceConfig::default();
        let (a, b) = (DiceClock::default(), DiceClock::default());
        for clock in [&a, &b] {
            assert!(clock.observe(&cfg).is_none());
            clock.remember(rest_orientation(1));
        }
        cfg.roll_id = 3;
        cfg.face = Face::from_index(4);
        let ((_, fa), (_, fb)) = (a.observe(&cfg).unwrap(), b.observe(&cfg).unwrap());
        assert_eq!(fa, fb);
        assert_eq!(fa, rest_orientation(1));
        assert!(a.busy() && b.busy());
        let t = 0.3;
        assert_eq!(orientation_at(fa, cfg.roll_id, 4, t), orientation_at(fb, cfg.roll_id, 4, t));
    }
}
