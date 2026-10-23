pub mod wheel;
use crate::model::*;
use egui::{Color32, FontFamily, FontId, Pos2, Rect, Shape, Stroke, TextureHandle, Vec2};
use image::AnimationDecoder;
use std::{
    collections::{BTreeMap, HashMap},
    hash::{Hash, Hasher},
    io::Cursor,
    sync::{Arc, Mutex, OnceLock, Weak},
};

pub struct FontBook {
    db: fontdb::Database,
    pub faces: BTreeMap<String, fontdb::ID>,
    aliases: BTreeMap<String, fontdb::ID>,
    pub definitions: egui::FontDefinitions,
    pub revision: u64,
}
impl Default for FontBook {
    fn default() -> Self {
        Self::new()
    }
}
impl FontBook {
    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();
        let aliases = db
            .faces()
            .map(|face| {
                let family = face
                    .families
                    .first()
                    .map(|f| f.0.as_str())
                    .unwrap_or("Unnamed");
                (
                    format!("{family} · {} {:?}", face.weight.0, face.style),
                    face.id,
                )
            })
            .collect();
        let faces = db
            .faces()
            .map(|face| {
                let family = face
                    .families
                    .first()
                    .map(|f| f.0.as_str())
                    .unwrap_or("Unnamed");
                let weight = match face.weight.0 {
                    0..=149 => "Thin",
                    150..=249 => "Extra Light",
                    250..=349 => "Light",
                    350..=449 => "Regular",
                    450..=549 => "Medium",
                    550..=649 => "Semibold",
                    650..=749 => "Bold",
                    750..=849 => "Extra Bold",
                    _ => "Black",
                };
                let style = match face.style {
                    fontdb::Style::Normal => "",
                    fontdb::Style::Italic => " Italic",
                    fontdb::Style::Oblique => " Oblique",
                };
                (format!("{family} — {weight}{style}"), face.id)
            })
            .collect();
        Self {
            db,
            faces,
            aliases,
            definitions: egui::FontDefinitions::default(),
            revision: 0,
        }
    }
    pub fn ensure(&mut self, name: &str) -> Result<bool, String> {
        if name == "Default"
            || self
                .definitions
                .families
                .contains_key(&FontFamily::Name(name.into()))
        {
            return Ok(false);
        }
        let id = *self
            .faces
            .get(name)
            .or_else(|| self.aliases.get(name))
            .ok_or_else(|| {
                format!("Missing font: {name}. Select a replacement in the inspector.")
            })?;
        let data = self
            .db
            .with_face_data(id, |bytes, index| {
                let mut data = egui::FontData::from_owned(bytes.to_vec());
                data.index = index;
                data
            })
            .ok_or("Cannot read font")?;
        self.definitions.font_data.insert(name.into(), data.into());
        let mut fallback = self.definitions.families[&FontFamily::Proportional].clone();
        fallback.insert(0, name.into());
        self.definitions
            .families
            .insert(FontFamily::Name(name.into()), fallback);
        self.revision += 1;
        Ok(true)
    }
}
pub struct MotionTexture {
    texture: TextureHandle,
    frames: Arc<Vec<egui::ColorImage>>,
    fps: f32,
    looped: bool,
    current: usize,
}
#[derive(Default)]
pub struct Resources {
    pub images: HashMap<u64, TextureHandle>,
    image_data: HashMap<u64, Arc<egui::ColorImage>>,
    motions: HashMap<u64, MotionTexture>,
    pub wheel_clock: wheel::WheelClock,
}
impl Resources {
    pub fn wheel_busy(&self) -> bool {
        self.wheel_clock.busy()
    }
    pub fn install_motion(
        &mut self,
        ctx: &egui::Context,
        asset: &MotionAsset,
        frames: Arc<Vec<egui::ColorImage>>,
    ) {
        self.motions.insert(
            asset.id,
            MotionTexture {
                texture: ctx.load_texture(
                    &asset.name,
                    frames[0].clone(),
                    egui::TextureOptions::LINEAR,
                ),
                frames,
                fps: asset.fps,
                looped: asset.looped,
                current: 0,
            },
        );
    }
    pub fn sync(&mut self, ctx: &egui::Context, assets: &[Asset], motions: &[MotionAsset]) {
        self.images
            .retain(|id, _| assets.iter().any(|a| a.id == *id));
        self.image_data
            .retain(|id, _| assets.iter().any(|a| a.id == *id));
        for a in assets {
            if self.images.contains_key(&a.id) {
                continue;
            }
            if let Ok(image) = decode_image(&a.bytes) {
                self.images.insert(
                    a.id,
                    ctx.load_texture(&a.name, image.clone(), egui::TextureOptions::LINEAR),
                );
                self.image_data.insert(a.id, image);
            }
        }
        self.motions
            .retain(|id, _| motions.iter().any(|motion| motion.id == *id));
        for motion in motions {
            if self.motions.contains_key(&motion.id) {
                continue;
            }
            let Ok(frames) = decode_motion(&motion.gif) else {
                continue;
            };
            if let Some(first) = frames.first() {
                self.motions.insert(
                    motion.id,
                    MotionTexture {
                        texture: ctx.load_texture(
                            &motion.name,
                            first.clone(),
                            egui::TextureOptions::LINEAR,
                        ),
                        frames,
                        fps: motion.fps,
                        looped: motion.looped,
                        current: 0,
                    },
                );
            }
        }
    }
    pub fn advance_motion(&mut self, elapsed: Option<f32>) {
        for motion in self.motions.values_mut() {
            let raw = (elapsed.unwrap_or(0.) * motion.fps).floor() as usize;
            let index = if motion.looped {
                raw % motion.frames.len()
            } else {
                raw.min(motion.frames.len() - 1)
            };
            if index != motion.current {
                motion
                    .texture
                    .set(motion.frames[index].clone(), egui::TextureOptions::LINEAR);
                motion.current = index;
            }
        }
    }
}

/// Shared decoded pixels: a background import can prepare them before GPU upload.
pub fn decode_image(bytes: &[u8]) -> Result<Arc<egui::ColorImage>, String> {
    type Cache = Mutex<HashMap<u64, Weak<egui::ColorImage>>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hash);
    let key = hash.finish();
    if let Some(image) = cache
        .lock()
        .map_err(|_| "Image cache unavailable")?
        .get(&key)
        .and_then(Weak::upgrade)
    {
        return Ok(image);
    }
    let dimensions = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_dimensions()
        .map_err(|e| e.to_string())?;
    if dimensions.0 == 0 || dimensions.1 == 0 || dimensions.0 > 8192 || dimensions.1 > 8192 {
        return Err("Image must be 1–8192 pixels per side".into());
    }
    let rgba = image::load_from_memory(bytes)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    let image = Arc::new(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ));
    let mut cache = cache.lock().map_err(|_| "Image cache unavailable")?;
    cache.retain(|_, image| image.strong_count() > 0);
    cache.insert(key, Arc::downgrade(&image));
    Ok(image)
}

type MotionFrames = Arc<Vec<egui::ColorImage>>;
type MotionCache = Mutex<HashMap<u64, Weak<Vec<egui::ColorImage>>>>;
pub fn decode_motion(bytes: &[u8]) -> Result<MotionFrames, String> {
    static CACHE: OnceLock<MotionCache> = OnceLock::new();
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hash);
    let key = hash.finish();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(cache) = cache.lock()
        && let Some(frames) = cache.get(&key).and_then(Weak::upgrade)
    {
        return Ok(frames);
    }
    let mut decoder =
        image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    use image::ImageDecoder;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1280);
    limits.max_image_height = Some(1280);
    limits.max_alloc = Some(64 * 1024 * 1024);
    decoder.set_limits(limits).map_err(|e| e.to_string())?;
    let mut frames = vec![];
    let mut memory = 0_usize;
    for frame in decoder.into_frames() {
        if frames.len() >= 225 {
            return Err("Video exceeds 225 frames".into());
        }
        let rgba = frame.map_err(|e| e.to_string())?.into_buffer();
        let rgba = image::DynamicImage::ImageRgba8(rgba)
            .thumbnail(640, 360)
            .to_rgba8();
        memory += rgba.as_raw().len();
        if memory > 256 * 1024 * 1024 {
            return Err("Decoded video exceeds memory limit".into());
        }
        frames.push(egui::ColorImage::from_rgba_unmultiplied(
            [rgba.width() as usize, rgba.height() as usize],
            rgba.as_raw(),
        ));
    }
    if frames.is_empty() {
        return Err("Video has no frames".into());
    }
    let frames = Arc::new(frames);
    if let Ok(mut cache) = cache.lock() {
        cache.retain(|_, entry| entry.strong_count() > 0);
        cache.insert(key, Arc::downgrade(&frames));
    }
    Ok(frames)
}
pub fn rgba(c: [u8; 4], opacity: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        c[0],
        c[1],
        c[2],
        (c[3] as f32 * opacity.clamp(0., 1.)).round() as u8,
    )
}
pub fn element_rect(e: &Element, origin: Pos2, scale: f32) -> Rect {
    Rect::from_min_size(
        origin + Vec2::new(e.x, e.y) * scale,
        Vec2::new(e.w, e.h) * scale,
    )
}
pub fn hit(e: &Element, point: Pos2) -> bool {
    let center = Pos2::new(e.x + e.w / 2., e.y + e.h / 2.);
    let p = egui::emath::Rot2::from_angle(-e.rotation.to_radians()) * (point - center);
    if e.kind == Kind::Ellipse {
        (p.x / (e.w / 2.)).powi(2) + (p.y / (e.h / 2.)).powi(2) <= 1.
    } else {
        p.x.abs() <= e.w / 2. && p.y.abs() <= e.h / 2.
    }
}
pub fn paint_scene(
    painter: &egui::Painter,
    d: &Design,
    values: &BTreeMap<String, String>,
    resources: &Resources,
    origin: Pos2,
    scale: f32,
    elapsed: Option<f32>,
) {
    for base in &d.elements {
        let animated = elapsed
            .filter(|_| !base.keyframes.is_empty())
            .map(|time| sample_keyframes(base, time));
        let e = animated.as_ref().unwrap_or(base);
        if e.hidden {
            continue;
        }
        let t = elapsed
            .map(|s| (s / e.duration).clamp(0., 1.))
            .unwrap_or(1.);
        let ease = 1. - (1. - t).powi(3);
        let alpha = if e.keyframes.is_empty()
            && matches!(
                e.animation,
                Animation::Fade | Animation::Slide | Animation::Pop
            ) {
            ease
        } else {
            1.
        };
        let mut rect = element_rect(e, origin, scale);
        if elapsed.is_some() && e.keyframes.is_empty() {
            match e.animation {
                Animation::Slide => rect = rect.translate(Vec2::new(0., (1. - ease) * 80. * scale)),
                Animation::Pop => {
                    rect = Rect::from_center_size(rect.center(), rect.size() * (0.8 + 0.2 * ease))
                }
                _ => {}
            }
        }
        let color = rgba(e.resolved_fill(values), e.opacity * alpha);
        let stroke = Stroke::new(e.stroke_width * scale, rgba(e.stroke, e.opacity * alpha));
        let rot = egui::emath::Rot2::from_angle(e.rotation.to_radians());
        let center = rect.center();
        let rotate = |p: Pos2| center + rot * (p - center);
        match e.kind {
            Kind::Text => {
                let content = e.content(values);
                let family = if e.font == "Default" {
                    FontFamily::Proportional
                } else {
                    FontFamily::Name(e.font.clone().into())
                };
                let family = if painter.ctx().fonts(|f| f.families().contains(&family)) {
                    family
                } else {
                    FontFamily::Proportional
                };
                let mut font = FontId::new(e.font_size * scale, family);
                let mut galley = painter.layout_no_wrap(content.clone(), font.clone(), color);
                let fit = (rect.width() / galley.size().x.max(1.))
                    .min(rect.height() / galley.size().y.max(1.))
                    .min(1.);
                if fit < 1. {
                    font.size *= fit;
                    galley = painter.layout_no_wrap(content, font, color);
                }
                let x = match e.text_align {
                    TextAlign::Left => rect.left(),
                    TextAlign::Center => rect.center().x - galley.size().x / 2.,
                    TextAlign::Right => rect.right() - galley.size().x,
                };
                let center = rotate(Pos2::new(x + galley.size().x / 2., rect.center().y));
                let pos = center - galley.size() / 2.;
                painter.add(
                    egui::epaint::TextShape::new(pos, galley, color).with_angle_and_anchor(
                        e.rotation.to_radians(),
                        egui::Align2::CENTER_CENTER,
                    ),
                );
            }
            Kind::Image => {
                if let Some(texture) = e.asset.and_then(|id| resources.images.get(&id)) {
                    let mut mesh = egui::Mesh::with_texture(texture.id());
                    mesh.add_rect_with_uv(
                        rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                        rgba([255; 4], e.opacity * alpha),
                    );
                    for v in &mut mesh.vertices {
                        v.pos = rotate(v.pos);
                    }
                    painter.add(Shape::mesh(mesh));
                }
            }
            Kind::Video => {
                if let Some(texture) = e
                    .motion
                    .and_then(|id| resources.motions.get(&id))
                    .map(|m| &m.texture)
                {
                    let mut mesh = egui::Mesh::with_texture(texture.id());
                    mesh.add_rect_with_uv(
                        rect,
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                        rgba([255; 4], e.opacity * alpha),
                    );
                    for v in &mut mesh.vertices {
                        v.pos = rotate(v.pos);
                    }
                    painter.add(Shape::mesh(mesh));
                }
            }
            Kind::Wheel => wheel::paint(
                painter,
                rect,
                e.rotation.to_radians(),
                e.opacity * alpha,
                values,
                &resources.wheel_clock,
            ),
            Kind::Line => {
                painter.line_segment(
                    [rotate(rect.left_center()), rotate(rect.right_center())],
                    Stroke::new(e.stroke_width.max(3.) * scale, color),
                );
            }
            _ => {
                let points = match e.kind {
                    Kind::Ellipse => (0..64)
                        .map(|i| {
                            let a = i as f32 * std::f32::consts::TAU / 64.;
                            rotate(
                                center
                                    + Vec2::new(
                                        a.cos() * rect.width() / 2.,
                                        a.sin() * rect.height() / 2.,
                                    ),
                            )
                        })
                        .collect(),
                    Kind::Triangle => vec![
                        rotate(rect.center_top()),
                        rotate(rect.right_bottom()),
                        rotate(rect.left_bottom()),
                    ],
                    Kind::Diamond => vec![
                        rotate(rect.center_top()),
                        rotate(rect.right_center()),
                        rotate(rect.center_bottom()),
                        rotate(rect.left_center()),
                    ],
                    _ => {
                        let r = (e.radius * scale)
                            .min(rect.width() / 2.)
                            .min(rect.height() / 2.);
                        let corners = [
                            (
                                rect.right_top() + Vec2::new(-r, r),
                                -std::f32::consts::FRAC_PI_2,
                            ),
                            (rect.right_bottom() + Vec2::new(-r, -r), 0.),
                            (
                                rect.left_bottom() + Vec2::new(r, -r),
                                std::f32::consts::FRAC_PI_2,
                            ),
                            (rect.left_top() + Vec2::new(r, r), std::f32::consts::PI),
                        ];
                        corners
                            .into_iter()
                            .flat_map(|(c, start)| {
                                (0..=8).map(move |i| {
                                    rotate(
                                        c + Vec2::angled(
                                            start + i as f32 * std::f32::consts::FRAC_PI_2 / 8.,
                                        ) * r,
                                    )
                                })
                            })
                            .collect()
                    }
                };
                painter.add(Shape::convex_polygon(points, color, stroke));
            }
        }
    }
}

pub fn sample_keyframes(element: &Element, elapsed: f32) -> Element {
    let mut result = element.clone();
    let mut frames: Vec<_> = element.keyframes.iter().collect();
    frames.sort_by(|a, b| a.time.total_cmp(&b.time));
    let Some(first) = frames.first() else {
        return result;
    };
    let end = frames.last().unwrap().time;
    let time = if element.loop_animation && end > 0. {
        elapsed.max(0.) % end
    } else {
        elapsed.max(0.)
    };
    let (a, b) = if time <= first.time {
        (*first, *first)
    } else {
        frames
            .windows(2)
            .find(|pair| time <= pair[1].time)
            .map(|pair| (pair[0], pair[1]))
            .unwrap_or_else(|| {
                let last = *frames.last().unwrap();
                (last, last)
            })
    };
    let t = if b.time > a.time {
        ((time - a.time) / (b.time - a.time)).clamp(0., 1.)
    } else {
        1.
    };
    let t = match a.easing {
        Easing::Linear => t,
        Easing::EaseOut => 1. - (1. - t).powi(3),
        Easing::EaseInOut => t * t * (3. - 2. * t),
        Easing::EaseOutBack if t > 0. && t < 1. => 1. + 2.70158 * (t - 1.).powi(3) + 1.70158 * (t - 1.).powi(2),
        Easing::Spring if t > 0. && t < 1. => {
            let tail = (-6f32).exp() * 10f32.cos();
            1. - (-6. * t).exp() * (10. * t).cos() + t * tail
        }
        Easing::EaseOutBack | Easing::Spring => t,
        Easing::Hold => {
            if t >= 1. {
                1.
            } else {
                0.
            }
        }
    };
    let mix = |a: f32, b: f32| a + (b - a) * t;
    result.x = mix(a.x, b.x);
    result.y = mix(a.y, b.y);
    result.w = mix(a.w, b.w);
    result.h = mix(a.h, b.h);
    result.rotation = mix(a.rotation, b.rotation);
    result.opacity = mix(a.opacity, b.opacity);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyframes_interpolate_loop_and_hold_without_changing_source() {
        let mut element = Element::new(1, Kind::Rectangle);
        let mut first = Keyframe::capture(&element, 0.);
        first.x = 0.;
        first.opacity = 0.;
        first.easing = Easing::Linear;
        let mut last = Keyframe::capture(&element, 2.);
        last.x = 200.;
        last.opacity = 1.;
        element.keyframes = vec![last, first];
        let midpoint = sample_keyframes(&element, 1.);
        assert_eq!(midpoint.x, 100.);
        assert_eq!(midpoint.opacity, 0.5);
        assert_eq!(element.x, 300.);
        element.loop_animation = true;
        assert_eq!(sample_keyframes(&element, 3.).x, 100.);
        element.keyframes[1].easing = Easing::Hold;
        assert_eq!(sample_keyframes(&element, 1.9).x, 0.);
        element.loop_animation = false;
        assert_eq!(sample_keyframes(&element, 2.).x, 200.);
    }
    #[test]
    fn back_and_spring_easing_curves() {
        let mut e = Element::new(1, Kind::Rectangle);
        e.keyframes = vec![Keyframe::capture(&e, 0.), {
            let mut k = Keyframe::capture(&e, 1.);
            k.x = 100.;
            k
        }];
        for easing in [Easing::EaseOutBack, Easing::Spring] {
            e.keyframes[0].easing = easing;
            assert_eq!(sample_keyframes(&e, 0.).x, 0.);
            assert_eq!(sample_keyframes(&e, 1.).x, 100.);
            let peak = (1..100)
                .map(|i| sample_keyframes(&e, i as f32 / 100.).x)
                .fold(0., f32::max);
            assert!(peak > 100., "{easing:?} should overshoot");
        }
        e.keyframes[0].easing = Easing::EaseOutBack;
        let peak = (1..100)
            .map(|i| sample_keyframes(&e, i as f32 / 100.).x)
            .fold(0., f32::max);
        assert!((peak - 110.).abs() < 1.5);
    }
    #[test]
    fn rotation_hit_test() {
        let mut e = Element::new(1, Kind::Rectangle);
        e.x = 0.;
        e.y = 0.;
        e.w = 100.;
        e.h = 20.;
        e.rotation = 90.;
        assert!(hit(&e, Pos2::new(50., 50.)));
        assert!(!hit(&e, Pos2::new(5., 10.)));
    }
    #[test]
    fn alpha_is_premultiplied() {
        let c = rgba([255, 0, 0, 128], 1.);
        assert_eq!(c.a(), 128);
        assert!(c.r() < 255);
    }
    #[test]
    fn installed_font_face_loads_and_lays_out_text() {
        let mut fonts = FontBook::new();
        let legacy = fonts.aliases.keys().next().unwrap().clone();
        assert!(fonts.ensure(&legacy).unwrap());
        let name = fonts
            .faces
            .keys()
            .find(|name| name.starts_with("Arial ·"))
            .or_else(|| fonts.faces.keys().next())
            .expect("This test requires at least one installed font")
            .clone();
        assert!(fonts.ensure(&name).unwrap());
        assert!(!fonts.ensure(&name).unwrap());
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts.definitions);
        let mut e = Element::new(1, Kind::Text);
        e.font = name;
        e.text = "Score 12".into();
        e.x = 10.;
        e.y = 10.;
        let d = Design {
            id: 0,
            name: "Font check".into(),
            width: 640,
            height: 360,
            elements: vec![e],
        };
        let mut full = ctx.run_ui(egui::RawInput::default(), |ui| {
            paint_scene(
                ui.painter(),
                &d,
                &BTreeMap::new(),
                &Resources::default(),
                Pos2::ZERO,
                1.,
                None,
            )
        });
        assert!(!full.shapes.is_empty());
        full.textures_delta.clear();
    }
    #[test]
    fn motion_clip_decodes_and_advances_frames() {
        use image::codecs::gif::GifEncoder;
        let mut gif = vec![];
        {
            let mut encoder = GifEncoder::new(&mut gif);
            encoder
                .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                    2,
                    2,
                    image::Rgba([255, 0, 0, 255]),
                )))
                .unwrap();
            encoder
                .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                    2,
                    2,
                    image::Rgba([0, 0, 255, 255]),
                )))
                .unwrap();
        }
        let clip = MotionAsset {
            id: 42,
            name: "Two frames".into(),
            gif: gif.into(),
            fps: 10.,
            looped: false,
        };
        let ctx = egui::Context::default();
        let mut resources = Resources::default();
        resources.sync(&ctx, &[], &[clip]);
        assert_eq!(resources.motions[&42].frames.len(), 2);
        resources.advance_motion(Some(0.11));
        assert_eq!(resources.motions[&42].current, 1);
    }
}
