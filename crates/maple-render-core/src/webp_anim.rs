//! Animated WebP output.
//!
//! Unlike GIF, animated WebP supports full 24-bit color with alpha per frame,
//! so it does **not** need color quantization and exhibits no gradient banding.
//!
//! Encoding is delegated to [`libwebp`](https://developers.google.com/speed/webp)
//! via the [`webp-animation`] crate (statically linked when the `static` feature
//! is enabled, which is the default on native targets).

#[cfg(not(target_arch = "wasm32"))]
use std::{fs::File, io::Write, path::Path};

#[cfg(not(target_arch = "wasm32"))]
use webp_animation::Encoder as WebPEncoder;

#[cfg(not(target_arch = "wasm32"))]
use crate::{
    error::{Error, Result},
    renders::Renders,
};

/// Quality (0..=100) used for lossy WebP encoding.
pub const DEFAULT_WEBP_QUALITY: f32 = 95.0;

/// Lossy method (0=fastest … 6=slowest-best). libwebp default is 4.
pub const DEFAULT_WEBP_METHOD: usize = 4;

#[cfg(not(target_arch = "wasm32"))]
pub struct WebpOptions {
    /// 0..=100. 100 = best quality (largest).
    pub quality: f32,
    /// Lossless encoding when true (quality acts as compression effort 0..=100).
    pub lossless: bool,
    /// Method / speed: 0 = fastest (larger/slightly lower quality), 6 = slowest best.
    pub method: usize,
}

impl Default for WebpOptions {
    fn default() -> Self {
        WebpOptions { quality: DEFAULT_WEBP_QUALITY, lossless: false, method: DEFAULT_WEBP_METHOD }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub struct WebpAnim {
    renders: Renders,
    period: f64,
    hold: f64,
    first_frame: i32,
    options: WebpOptions,
}

#[cfg(not(target_arch = "wasm32"))]
impl WebpAnim {
    pub fn new(renders: Renders) -> Self {
        WebpAnim {
            renders,
            period: 0.1,
            hold: 5.0,
            first_frame: -1,
            options: WebpOptions::default(),
        }
    }

    pub fn set_first_frame(&mut self, index: i32) {
        self.first_frame = index;
    }

    pub fn set_timing(&mut self, period: f64, hold: f64) {
        self.period = period;
        self.hold = hold;
    }

    pub fn set_options(&mut self, options: WebpOptions) {
        self.options = WebpOptions {
            quality: options.quality.clamp(0.0, 100.0),
            lossless: options.lossless,
            method: options.method.min(6),
        };
    }

    /// Encode a single RGBA frame to a WebP byte buffer (still image, not animation).
    ///
    /// This is a convenience for `--webp_single`. It is only available on native
    /// targets (libwebp is not built for wasm).
    pub fn encode_single(img: &image::RgbaImage, options: &WebpOptions) -> Result<Vec<u8>> {
        let (width, height) = (img.width(), img.height());
        let rgba = img.as_raw().to_vec();

        let enc_options = if options.lossless {
            webp_animation::EncoderOptions {
                encoding_config: Some(webp_animation::EncodingConfig {
                    encoding_type: webp_animation::EncodingType::Lossless,
                    quality: options.quality,
                    method: options.method,
                    ..Default::default()
                }),
                color_mode: webp_animation::ColorMode::Rgba,
                ..Default::default()
            }
        } else {
            let mut cfg = webp_animation::EncodingConfig::new_lossy(options.quality);
            cfg.method = options.method;
            webp_animation::EncoderOptions {
                encoding_config: Some(cfg),
                color_mode: webp_animation::ColorMode::Rgba,
                ..Default::default()
            }
        };

        let mut enc = WebPEncoder::new_with_options((width, height), enc_options)
            .map_err(|e| Error::VideoEncode(format!("WebP encoder init: {}", e)))?;
        enc.add_frame(&rgba, 0)
            .map_err(|e| Error::VideoEncode(format!("WebP add_frame: {}", e)))?;
        let data =
            enc.finalize(1).map_err(|e| Error::VideoEncode(format!("WebP finalize: {}", e)))?;
        Ok(data.as_ref().to_vec())
    }

    /// Encode all frames to a WebP byte buffer.
    pub fn encode(&mut self) -> Result<Vec<u8>> {
        let frames = self.renders.length() as i32;
        if frames == 0 {
            return Err(Error::VideoEncode("No frames to encode".to_string()));
        }

        // Dimensions come from the first render.
        let first = self.renders.get_render(0)?;
        let (width, height) = (first.get().width(), first.get().height());

        let encoding_config = if self.options.lossless {
            webp_animation::EncodingConfig {
                encoding_type: webp_animation::EncodingType::Lossless,
                quality: self.options.quality,
                method: self.options.method,
                ..Default::default()
            }
        } else {
            let mut cfg = webp_animation::EncodingConfig::new_lossy(self.options.quality);
            cfg.method = self.options.method;
            cfg
        };

        let enc_options = webp_animation::EncoderOptions {
            encoding_config: Some(encoding_config),
            color_mode: webp_animation::ColorMode::Rgba,
            kmin: 3,
            kmax: 5,
            ..Default::default()
        };

        let mut enc = WebPEncoder::new_with_options((width, height), enc_options)
            .map_err(|e| Error::VideoEncode(format!("WebP encoder init: {}", e)))?;

        let mut curr_ms: f64 = 0.0;
        let mut prev_ts: i32 = -1;

        for base in 0..frames {
            let i = if self.first_frame >= 0 { (base + self.first_frame) % frames } else { base };

            let step = if i == frames - 1 { self.period + self.hold } else { self.period };

            let render = self.renders.get_render(i)?;
            let img = render.get();
            // libwebp expects raw RGBA bytes (one plane, no stride padding).
            let rgba: Vec<u8> = img.as_raw().to_vec();

            let mut ts = (curr_ms).round() as i32;
            if ts <= prev_ts {
                ts = prev_ts + 1;
            }
            enc.add_frame(&rgba, ts)
                .map_err(|e| Error::VideoEncode(format!("WebP add_frame: {}", e)))?;
            prev_ts = ts;

            curr_ms += step * 1000.0;
            self.renders.remove_render(i);
        }

        // Finalize: timestamp marks when the final frame's display ends.
        let mut final_ts = curr_ms.round() as i32;
        if final_ts <= prev_ts {
            final_ts = prev_ts + 1;
        }

        let webp_data = enc
            .finalize(final_ts)
            .map_err(|e| Error::VideoEncode(format!("WebP finalize: {}", e)))?;

        Ok(webp_data.as_ref().to_vec())
    }

    /// Encode all frames and write the result to `path`.
    pub fn save<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let data = self.encode()?;
        let file = File::create(path.as_ref()).map_err(|e| Error::Io(e))?;
        let mut writer = std::io::BufWriter::new(file);
        writer.write_all(&data).map_err(|e| Error::Io(e))?;
        Ok(())
    }
}
