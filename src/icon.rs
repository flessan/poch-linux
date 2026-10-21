//! Ikon aplikasi (icon.ico) dan cara menggambarnya.

use super::*;

const ICON_PX: u32 = 40;
static ICON_BYTES: &[u8] = include_bytes!("../icon.ico");

pub(crate) fn load_icon() -> Option<Pixmap> {
    let img = image::load_from_memory_with_format(ICON_BYTES, image::ImageFormat::Ico).ok()?;
    let img = img.resize_to_fill(ICON_PX, ICON_PX, image::imageops::FilterType::Lanczos3);
    pixmap_from_rgba(&img.to_rgba8().into_raw(), ICON_PX)
}

impl App {
    pub(crate) fn icon_at(&self, g: &mut Gfx, x: f32, y: f32, size: f32, r: f32, al: f32) {
        let path = rrect(x, y, size, size, r);
        if let Some(icon) = &self.icon {
            let s = size / ICON_PX as f32;
            let mut p = Paint::default();
            p.shader = Pattern::new(
                icon.as_ref(),
                SpreadMode::Pad,
                FilterQuality::Bilinear,
                al,
                Transform::from_row(s, 0.0, 0.0, s, x, y),
            );
            p.anti_alias = true;
            g.fill(&path, &p);
        } else {
            g.fill(&path, &acc(al));
        }
    }
}
