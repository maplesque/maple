use std::sync::OnceLock;

use image::RgbaImage;

#[derive(Clone)]
pub struct Mapping {
    pub light: RgbaImage,
    pub dark: RgbaImage,
    pub map1: RgbaImage,
    pub map2: RgbaImage,
    pub neutral: RgbaImage,
    pub scale: i32,
    pub light_name: String,
    pub dark_name: String,
    pub map1_name: String,
    pub map2_name: String,
    pub neutral_name: String,
    /// Cached result of [`Self::has_nonzero_smoothing`]. `None` until first query.
    pub(crate) smooth_cache: OnceLock<bool>,
}

impl Default for Mapping {
    fn default() -> Self {
        Mapping {
            light: RgbaImage::new(1, 1),
            dark: RgbaImage::new(1, 1),
            map1: RgbaImage::new(1, 1),
            map2: RgbaImage::new(1, 1),
            neutral: RgbaImage::new(1, 1),
            scale: 1,
            light_name: String::new(),
            dark_name: String::new(),
            map1_name: String::new(),
            map2_name: String::new(),
            neutral_name: String::new(),
            smooth_cache: OnceLock::new(),
        }
    }
}

impl Mapping {
    pub fn new() -> Self {
        Mapping::default()
    }

    pub fn width(&self) -> u32 {
        self.light.width()
    }

    pub fn height(&self) -> u32 {
        self.light.height()
    }

    /// Whether the `map2` ("sel") image's channel 2 (the edge-smoothing flag)
    /// contains any nonzero pixel. When false (the common case for shipped
    /// templates), [`crate::render::Render`]'s smoothing pass is a no-op and can
    /// be skipped entirely, avoiding a full output-image clone + scan.
    ///
    /// The result is computed once and cached for the lifetime of this mapping.
    pub fn has_nonzero_smoothing(&self) -> bool {
        *self.smooth_cache.get_or_init(|| {
            // RGBA: channel 2 is at byte offset +2 of each 4-byte pixel.
            let raw = self.map2.as_raw();
            raw.chunks_exact(4).any(|px| px[2] != 0)
        })
    }
}
