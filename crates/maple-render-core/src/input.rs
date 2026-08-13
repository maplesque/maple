use std::path::Path;

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{Rgba, RgbaImage};
use imageproc::drawing::draw_text_mut;

use crate::error::{Error, Result};

#[derive(Clone)]
pub struct TextOptions {
    pub font_size: f32,
    pub color: Rgba<u8>,
    pub background: Rgba<u8>,
    pub padding: u32,
}

impl Default for TextOptions {
    fn default() -> Self {
        TextOptions {
            font_size: 72.0,
            color: Rgba([0, 0, 0, 255]),
            background: Rgba([255, 255, 255, 255]),
            padding: 40,
        }
    }
}

#[derive(Clone)]
pub struct Input {
    image: RgbaImage,
    opaque: bool,
    pub layer: u8,
    pub xs: f64,
    pub ys: f64,
    pub xo: f64,
    pub yo: f64,
    pub xa: f64,
    pub ya: f64,
    pub in_scale: i32,
    pub in_x0: f64,
    pub in_y0: f64,
}

impl Default for Input {
    fn default() -> Self {
        Input {
            image: RgbaImage::new(1, 1),
            opaque: false,
            layer: 1,
            xs: 1.0,
            ys: 1.0,
            xo: 0.0,
            yo: 0.0,
            xa: 1.0,
            ya: 0.0,
            in_scale: 0,
            in_x0: 0.0,
            in_y0: 0.0,
        }
    }
}

impl Input {
    pub fn new() -> Self {
        Input::default()
    }

    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::load_with_layer(path, 1)
    }

    pub fn load_with_layer<P: AsRef<Path>>(path: P, layer: u8) -> Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(Error::FileNotFound(path.to_path_buf()));
        }

        let img = image::open(path)?;
        let opaque = !img.color().has_alpha();
        let rgba = img.to_rgba8();

        let mut input = Input { image: rgba, opaque, layer, ..Default::default() };

        input.compute_scale_params();

        Ok(input)
    }

    pub fn from_image(image: RgbaImage, layer: u8) -> Self {
        let opaque = image.pixels().all(|pixel| pixel[3] == 255);
        let mut input = Input { image, opaque, layer, ..Default::default() };
        input.compute_scale_params();
        input
    }

    pub fn from_text(
        text: &str,
        layer: u8,
        options: &TextOptions,
        font_data: &[u8],
    ) -> Result<Self> {
        let font = FontRef::try_from_slice(font_data)
            .map_err(|e| Error::TextRender(format!("Failed to load font: {}", e)))?;

        let scale = PxScale::from(options.font_size);
        let scaled_font = font.as_scaled(scale);

        let mut width = 0.0f32;
        for c in text.chars() {
            let glyph_id = font.glyph_id(c);
            width += scaled_font.h_advance(glyph_id);
        }

        let height = scaled_font.height();
        let img_width = (width.ceil() as u32).max(1) + options.padding * 2;
        let img_height = (height.ceil() as u32).max(1) + options.padding * 2;

        let mut image = RgbaImage::from_pixel(img_width, img_height, options.background);

        let x = options.padding as i32;
        let y = options.padding as i32;

        draw_text_mut(&mut image, options.color, x, y, scale, &font, text);

        Ok(Input::from_image(image, layer))
    }

    fn compute_scale_params(&mut self) {
        let w = self.image.width() as i32;
        let h = self.image.height() as i32;

        self.in_scale = w;
        self.in_x0 = 0.0;
        self.in_y0 = 0.0;

        if h > w {
            self.in_scale = h;
            self.in_x0 = -((-w + h) as f64) / 2.0;
        } else {
            self.in_y0 = -((w - h) as f64) / 2.0;
        }

        self.xa = 1.0;
        self.ya = 0.0;
    }

    pub fn get(&self) -> &RgbaImage {
        &self.image
    }

    pub fn get_mut(&mut self) -> &mut RgbaImage {
        self.opaque = false;
        &mut self.image
    }

    pub fn is_opaque(&self) -> bool {
        self.opaque
    }

    pub fn width(&self) -> u32 {
        self.image.width()
    }

    pub fn height(&self) -> u32 {
        self.image.height()
    }

    pub fn safe_pixel(&self, x: i32, y: i32) -> Rgba<u8> {
        let w = self.image.width() as i32;
        let h = self.image.height() as i32;

        if x < 0 || y < 0 || x >= w || y >= h {
            Rgba([0, 0, 0, 0])
        } else {
            *self.image.get_pixel(x as u32, y as u32)
        }
    }

    pub fn set_rotation(&mut self, theta: f64) {
        self.xa = theta.cos();
        self.ya = theta.sin();
    }
}

#[derive(Clone, Default)]
pub struct Inputs {
    data: Vec<Input>,
}

impl Inputs {
    pub fn new() -> Self {
        Inputs { data: Vec::new() }
    }

    pub fn add(&mut self) -> &mut Input {
        self.data.push(Input::new());
        self.data.last_mut().unwrap()
    }

    pub fn push(&mut self, input: Input) {
        self.data.push(input);
    }

    pub fn get(&self) -> &[Input] {
        &self.data
    }

    pub fn get_mut(&mut self) -> &mut [Input] {
        &mut self.data
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Input> {
        self.data.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Input> {
        self.data.iter_mut()
    }
}

impl std::ops::Index<usize> for Inputs {
    type Output = Input;

    fn index(&self, index: usize) -> &Self::Output {
        &self.data[index]
    }
}

impl std::ops::IndexMut<usize> for Inputs {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.data[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_is_detected_and_invalidated_by_mutable_access() {
        let image = RgbaImage::from_pixel(2, 2, Rgba([10, 20, 30, 255]));
        let mut input = Input::from_image(image, 1);
        assert!(input.is_opaque());

        input.get_mut().put_pixel(0, 0, Rgba([10, 20, 30, 255]));
        assert!(!input.is_opaque());

        let image = RgbaImage::from_pixel(2, 2, Rgba([10, 20, 30, 254]));
        assert!(!Input::from_image(image, 1).is_opaque());
    }
}
