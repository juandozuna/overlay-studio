use super::DocumentSession;
use crate::{
    infrastructure::media_import::{Pixels, PreparedMedia},
    model::{Element, Kind, MAX_ID},
};
impl DocumentSession {
    pub fn import_media(
        &mut self,
        mut media: PreparedMedia,
        design: usize,
    ) -> Result<(u64, Pixels), String> {
        self.end_gesture()?;
        if design >= self.project.designs.len() {
            return Err("Import canvas no longer exists".into());
        }
        let asset_id = self.project.next_id();
        let id = asset_id
            .checked_add(1)
            .filter(|id| *id <= MAX_ID)
            .ok_or("Layer IDs are exhausted")?;
        let size = media.dimensions();
        let kind = if media.image.is_some() {
            Kind::Image
        } else {
            Kind::Video
        };
        let mut layer = Element::new(id, kind);
        layer.animation = crate::model::Animation::None;
        let scale = 320. / size[0].max(size[1]) as f32;
        layer.w = size[0] as f32 * scale;
        layer.h = size[1] as f32 * scale;
        layer.x = (self.project.designs[design].width as f32 - layer.w) / 2.;
        layer.y = (self.project.designs[design].height as f32 - layer.h) / 2.;
        let image_count = self.project.assets.len();
        let motion_count = self.project.motions.len();
        if let Some(mut asset) = media.image.take() {
            asset.id = asset_id;
            layer.name = format!("Image · {}", asset.name);
            layer.asset = Some(asset_id);
            self.project.assets.push(asset);
        }
        if let Some(mut motion) = media.motion.take() {
            motion.id = asset_id;
            layer.name = format!("Video · {}", motion.name);
            layer.motion = Some(asset_id);
            self.project.motions.push(motion);
        }
        let old_design = self.design;
        self.design = design;
        if let Err(error) = self.edit(|d| d.elements.push(layer)) {
            self.project.assets.truncate(image_count);
            self.project.motions.truncate(motion_count);
            self.design = old_design;
            return Err(error);
        }
        self.select(Some(id), false);
        Ok((asset_id, media.pixels))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    #[test]
    fn image_import_is_portable_one_undo_and_shared_pixels() {
        let path = std::env::temp_dir().join(format!("overlay-logo-{}.png", rand::random::<u64>()));
        image::RgbaImage::from_pixel(8, 4, image::Rgba([20, 120, 220, 100]))
            .save(&path)
            .unwrap();
        let prepared = PreparedMedia::read(&path, false).unwrap();
        let mut s = DocumentSession::default();
        let (id, pixels) = s.import_media(prepared, 0).unwrap();
        let Pixels::Image(image) = pixels else {
            panic!("Expected image");
        };
        assert!(Arc::ptr_eq(
            &image,
            &crate::scene::decode_image(&s.project.assets[0].bytes).unwrap()
        ));
        assert_eq!(s.selected_layer().unwrap().w, 320.);
        assert_eq!(s.selected_layer().unwrap().h, 160.);
        let portable = crate::infrastructure::portable_design::export(&s.project, 0).unwrap();
        assert_eq!(portable.assets.len(), 1);
        s.project.values.insert("home.score".into(), "17".into());
        s.undo().unwrap();
        assert!(!s.active().elements.iter().any(|e| e.asset == Some(id)));
        s.redo();
        assert!(s.active().elements.iter().any(|e| e.asset == Some(id)));
        assert_eq!(s.project.values["home.score"], "17");
        s.project.validate().unwrap();
        std::fs::write(&path, b"invalid image").unwrap();
        assert!(PreparedMedia::read(&path, false).is_err());
        let _ = std::fs::remove_file(path);
    }
}
