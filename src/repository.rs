use std::{
    collections::HashMap,
    io::{Read, Seek},
};

#[cfg(not(target_arch = "wasm32"))]
use std::{
    fs::File,
    io::BufReader,
    path::Path,
};

#[cfg(target_arch = "wasm32")]
use std::io::Cursor;

use image::RgbaImage;
use zip::ZipArchive;

use crate::{
    error::{Error, Result},
    mapping::Mapping,
    template::Template,
};

#[cfg(not(target_arch = "wasm32"))]
pub struct Repository {
    zip: ZipArchive<BufReader<File>>,
    pub template: Template,
    mappings: HashMap<i32, Mapping>,
    peak_cache_count: usize,
}

#[cfg(target_arch = "wasm32")]
pub struct Repository {
    zip: ZipArchive<Cursor<Vec<u8>>>,
    pub template: Template,
    mappings: HashMap<i32, Mapping>,
    peak_cache_count: usize,
}

impl Repository {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        if !path.exists() {
            return Err(Error::FileNotFound(path.to_path_buf()));
        }

        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut zip = ZipArchive::new(reader)?;

        let template_json = Self::load_text_from_zip(&mut zip, "template.json")?;
        let template: Template = serde_json::from_str(&template_json)?;

        Ok(Repository { zip, template, mappings: HashMap::new(), peak_cache_count: 0 })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load_from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let reader = Cursor::new(bytes);
        let mut zip = ZipArchive::new(reader)?;

        let template_json = Self::load_text_from_zip(&mut zip, "template.json")?;
        let template: Template = serde_json::from_str(&template_json)?;

        Ok(Repository { zip, template, mappings: HashMap::new(), peak_cache_count: 0 })
    }

    fn load_text_from_zip<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str) -> Result<String> {
        let mut file = zip.by_name(name).map_err(|_| Error::MissingFile(name.to_string()))?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        Ok(contents)
    }

    fn load_image(&mut self, name: &str) -> Result<RgbaImage> {
        let mut file = self.zip.by_name(name).map_err(|_| Error::MissingFile(name.to_string()))?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        let img = image::load_from_memory(&data)?;
        Ok(img.to_rgba8())
    }

    fn load_frame(&mut self, frame: i32) -> Result<Mapping> {
        let light_name = format!("frame{}_light.png", frame);
        let dark_name = format!("frame{}_dark.png", frame);
        let map_name = format!("frame{}_map.png", frame);
        let sel_name = format!("frame{}_sel.png", frame);
        let transparent_name = format!("frame{}_transparent.png", frame);

        let light = self.load_image(&light_name)?;
        let dark = self.load_image(&dark_name)?;
        let map1 = self.load_image(&map_name)?;
        let map2 = self.load_image(&sel_name)?;

        let neutral = match self.load_image(&transparent_name) {
            Ok(img) => img,
            Err(_) => light.clone(),
        };

        Ok(Mapping {
            light,
            dark,
            map1,
            map2,
            neutral,
            scale: 1,
            light_name,
            dark_name,
            map1_name: map_name,
            map2_name: sel_name,
            neutral_name: transparent_name,
        })
    }

    pub fn get_mapping(&mut self, index: i32) -> Result<&Mapping> {
        let actual_index = if !self.template.is_animation() { 0 } else { index };

        if !self.mappings.contains_key(&actual_index) {
            let mapping = self.load_frame(actual_index)?;
            self.mappings.insert(actual_index, mapping);

            if self.mappings.len() > self.peak_cache_count {
                self.peak_cache_count = self.mappings.len();
            }
        }

        Ok(self.mappings.get(&actual_index).unwrap())
    }

    pub fn remove_mapping(&mut self, index: i32) {
        self.mappings.remove(&index);
    }

    pub fn is_animation(&self) -> bool {
        self.template.is_animation()
    }

    pub fn length(&self) -> u32 {
        self.template.frames
    }

    pub fn get_palette(&self) -> Vec<i32> {
        self.template.palette.clone()
    }

    pub fn get_period(&self) -> f64 {
        self.template.period()
    }

    pub fn get_hold(&self) -> f64 {
        self.template.hold
    }

    pub fn peak(&self) -> usize {
        self.peak_cache_count
    }
}
