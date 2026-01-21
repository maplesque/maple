use std::io::Cursor;

use image::Rgba;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

use crate::{
    error::Error,
    gif_anim::GifAnim,
    input::{Input, Inputs, TextOptions},
    render::RenderQuality,
    renders::Renders,
    repository::Repository,
};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum InputSpec {
    Image { layer: u8, bytes: Vec<u8> },
    Text {
        layer: u8,
        text: String,
        font_size: Option<f32>,
        color: Option<String>,
        background: Option<String>,
        padding: Option<u32>,
    },
}

#[derive(Deserialize, Default)]
struct RenderOptions {
    frame: Option<i32>,
    auto_zoom: Option<bool>,
    width: Option<i32>,
    height: Option<i32>,
}

#[derive(Serialize)]
struct TemplateInfo {
    width: u32,
    height: u32,
    frames: u32,
    delay: f64,
    hold: f64,
    palette: Vec<i32>,
}

#[wasm_bindgen]
pub fn template_info(template_zip: &[u8]) -> Result<JsValue, JsValue> {
    let repo = Repository::load_from_bytes(template_zip.to_vec()).map_err(to_js_error)?;
    let template = repo.template;
    let info = TemplateInfo {
        width: template.width,
        height: template.height,
        frames: template.frames,
        delay: template.delay,
        hold: template.hold,
        palette: template.palette,
    };
    serde_wasm_bindgen::to_value(&info).map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub fn render_png(template_zip: &[u8], inputs: JsValue, options: JsValue) -> Result<Vec<u8>, JsValue> {
    let input_specs: Vec<InputSpec> =
        serde_wasm_bindgen::from_value(inputs).map_err(|e| JsValue::from_str(&e.to_string()))?;
    if input_specs.is_empty() {
        return Err(JsValue::from_str("At least one input is required."));
    }

    let options: RenderOptions = if options.is_null() || options.is_undefined() {
        RenderOptions::default()
    } else {
        serde_wasm_bindgen::from_value(options).map_err(|e| JsValue::from_str(&e.to_string()))?
    };

    let mut inputs = Inputs::new();
    for spec in input_specs {
        match spec {
            InputSpec::Image { layer, bytes } => {
                if !is_png(&bytes) {
                    return Err(JsValue::from_str("Only PNG inputs are supported."));
                }
                let img = image::load_from_memory(&bytes).map_err(|e| to_js_error(Error::from(e)))?;
                inputs.push(Input::from_image(img.to_rgba8(), layer));
            }
            InputSpec::Text { layer, text, font_size, color, background, padding } => {
                let mut options = TextOptions::default();
                if let Some(size) = font_size {
                    options.font_size = size;
                }
                if let Some(padding) = padding {
                    options.padding = padding;
                }
                if let Some(color) = color {
                    options.color = parse_hex_color(&color)?;
                }
                if let Some(background) = background {
                    options.background = parse_hex_color(&background)?;
                }
                let input = Input::from_text(&text, layer, &options).map_err(to_js_error)?;
                inputs.push(input);
            }
        }
    }

    let repo = Repository::load_from_bytes(template_zip.to_vec()).map_err(to_js_error)?;
    let mut renders = Renders::new(repo, inputs, RenderQuality::Sampled);

    if let (Some(w), Some(h)) = (options.width, options.height) {
        if w > 0 && h > 0 {
            renders.set_size(w, h);
        }
    }

    if options.auto_zoom.unwrap_or(false) {
        renders.auto_zoom().map_err(to_js_error)?;
    }

    let frame = options.frame.unwrap_or(0);
    let render = renders.get_render(frame).map_err(to_js_error)?;
    encode_png(render.get()).map_err(to_js_error)
}

#[wasm_bindgen]
pub fn render_gif(template_zip: &[u8], inputs: JsValue, options: JsValue) -> Result<Vec<u8>, JsValue> {
    let input_specs: Vec<InputSpec> =
        serde_wasm_bindgen::from_value(inputs).map_err(|e| JsValue::from_str(&e.to_string()))?;
    if input_specs.is_empty() {
        return Err(JsValue::from_str("At least one input is required."));
    }

    let options: RenderOptions = if options.is_null() || options.is_undefined() {
        RenderOptions::default()
    } else {
        serde_wasm_bindgen::from_value(options).map_err(|e| JsValue::from_str(&e.to_string()))?
    };

    let mut inputs = Inputs::new();
    for spec in input_specs {
        match spec {
            InputSpec::Image { layer, bytes } => {
                if !is_png(&bytes) {
                    return Err(JsValue::from_str("Only PNG inputs are supported."));
                }
                let img =
                    image::load_from_memory(&bytes).map_err(|e| to_js_error(Error::from(e)))?;
                inputs.push(Input::from_image(img.to_rgba8(), layer));
            }
            InputSpec::Text { layer, text, font_size, color, background, padding } => {
                let mut options = TextOptions::default();
                if let Some(size) = font_size {
                    options.font_size = size;
                }
                if let Some(padding) = padding {
                    options.padding = padding;
                }
                if let Some(color) = color {
                    options.color = parse_hex_color(&color)?;
                }
                if let Some(background) = background {
                    options.background = parse_hex_color(&background)?;
                }
                let input = Input::from_text(&text, layer, &options).map_err(to_js_error)?;
                inputs.push(input);
            }
        }
    }

    let repo = Repository::load_from_bytes(template_zip.to_vec()).map_err(to_js_error)?;
    let period = repo.get_period();
    let hold = repo.get_hold();
    let palette_frames = repo.get_palette();
    let mut renders = Renders::new(repo, inputs, RenderQuality::Sampled);

    if let (Some(w), Some(h)) = (options.width, options.height) {
        if w > 0 && h > 0 {
            renders.set_size(w, h);
        }
    }

    if options.auto_zoom.unwrap_or(false) {
        renders.auto_zoom().map_err(to_js_error)?;
    }

    let mut gif = GifAnim::new(renders);
    gif.set_timing(period, hold);
    if !palette_frames.is_empty() {
        gif.set_palette_frames(palette_frames);
    }
    gif.apply().map_err(to_js_error)?;
    gif.encode().map_err(to_js_error)
}

fn is_png(bytes: &[u8]) -> bool {
    const PNG_SIG: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    bytes.len() >= PNG_SIG.len() && bytes[..PNG_SIG.len()] == PNG_SIG
}

fn parse_hex_color(value: &str) -> Result<Rgba<u8>, JsValue> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 && hex.len() != 8 {
        return Err(JsValue::from_str("Color must be 6 or 8 hex digits."));
    }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| JsValue::from_str("Invalid hex color."))?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| JsValue::from_str("Invalid hex color."))?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| JsValue::from_str("Invalid hex color."))?;
    let a = if hex.len() == 8 {
        u8::from_str_radix(&hex[6..8], 16).map_err(|_| JsValue::from_str("Invalid hex color."))?
    } else {
        255
    };
    Ok(Rgba([r, g, b, a]))
}

fn encode_png(img: &image::RgbaImage) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    let mut cursor = Cursor::new(&mut out);
    let dyn_img = image::DynamicImage::ImageRgba8(img.clone());
    dyn_img.write_to(&mut cursor, image::ImageFormat::Png)?;
    Ok(out)
}

fn to_js_error(err: Error) -> JsValue {
    JsValue::from_str(&err.to_string())
}
