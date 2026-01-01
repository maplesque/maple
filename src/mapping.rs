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
}
