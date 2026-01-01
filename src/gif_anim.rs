use std::{fs::File, io::BufWriter, path::Path};

use gif::{Encoder, Frame, Repeat};
use image::RgbaImage;

use crate::{
    error::{Error, Result},
    quantize::{Palette, Quantizer},
    renders::Renders,
};

pub struct GifAnim {
    renders: Renders,
    palette_frames: Vec<i32>,
    period: f64,
    hold: f64,
    first_frame: i32,
    blocks: Vec<GifBlock>,
    palette: Option<Palette>,
}

struct GifBlock {
    data: Vec<u8>,
    delay: u16,
    width: u16,
    height: u16,
}

impl GifAnim {
    pub fn new(renders: Renders) -> Self {
        GifAnim {
            renders,
            palette_frames: vec![0],
            period: 0.1,
            hold: 5.0,
            first_frame: -1,
            blocks: Vec::new(),
            palette: None,
        }
    }

    pub fn set_palette(&mut self, index: i32) {
        self.palette_frames = vec![index];
    }

    pub fn set_palette_frames(&mut self, frames: Vec<i32>) {
        self.palette_frames = frames;
    }

    pub fn set_first_frame(&mut self, index: i32) {
        self.first_frame = index;
    }

    pub fn set_timing(&mut self, period: f64, hold: f64) {
        self.period = period;
        self.hold = hold;
    }

    pub fn apply(&mut self) -> Result<()> {
        let step = (self.period * 100.0 + 0.5) as u16;
        let last_step = (self.hold * 100.0 + 0.5) as u16;

        let pals = self.palette_frames.len();
        if pals < 1 {
            return Err(Error::MissingData("No palette frames specified".to_string()));
        }

        let mut pal_image: Option<RgbaImage> = None;
        let mut pal_index: i32 = -1;

        for (i, &idx) in self.palette_frames.iter().enumerate() {
            // Get the rendered image first
            let img = {
                let render = self.renders.get_render(idx)?;
                render.get().clone()
            };
            self.renders.remove_mapping(idx);

            if pals == 1 {
                pal_image = Some(img);
                pal_index = idx;
            } else {
                let w = img.width() as usize;
                let h = img.height() as usize;
                if pal_image.is_none() {
                    pal_image = Some(img.clone());
                } else {
                    let pal_img = pal_image.as_mut().unwrap();
                    for k in (i..(w * h)).step_by(pals) {
                        let xx = (k % w) as u32;
                        let yy = (k / w) as u32;
                        let pixel = *img.get_pixel(xx, yy);
                        pal_img.put_pixel(xx, yy, pixel);
                    }
                }
            }
        }

        let pal_image = pal_image.ok_or(Error::MissingData("No palette image".to_string()))?;
        let ww = pal_image.width() as u16;
        let hh = pal_image.height() as u16;

        let mut quantizer = Quantizer::new(&pal_image);
        self.palette = Some(quantizer.palette().clone());

        let mut prev_indices = quantizer.quantize(&pal_image, true);
        let _pre_prev_indices = prev_indices.clone();

        let frames = self.renders.length() as i32;

        let mut _emit_ct = 0;
        let mut step_pending: u16 = 0;
        let mut pending_frame_number = pal_index;

        for base in 0..frames {
            let i = if self.first_frame >= 0 { (base + self.first_frame) % frames } else { base };

            step_pending = step_pending.saturating_add(step);

            let curr_indices = if i != pal_index {
                let img = {
                    let render = self.renders.get_render(i)?;
                    render.get().clone()
                };
                self.renders.remove_mapping(i);
                quantizer.quantize(&img, true)
            } else {
                prev_indices.clone()
            };

            self.renders.remove_render(i);

            let mut change = false;
            let mut _emitting_frame_number = -1;

            if base > 0 {
                for (p0, p1) in prev_indices.iter().zip(curr_indices.iter()) {
                    if p0 != p1 {
                        change = true;
                        _emitting_frame_number = pending_frame_number;
                        pending_frame_number = i;
                        break;
                    }
                }
            }

            if change {
                let delay = step_pending.saturating_sub(step);
                self.blocks.push(GifBlock {
                    data: prev_indices.clone(),
                    delay,
                    width: ww,
                    height: hh,
                });
                step_pending = step;
                _emit_ct += 1;
            }

            if change {
                let _ = std::mem::replace(&mut prev_indices, curr_indices);
            } else {
                prev_indices = curr_indices;
                pending_frame_number = i;
            }

            if i == frames - 1 {
                step_pending = step_pending.saturating_add(last_step);
            }
        }

        if frames > 0 {
            self.blocks.push(GifBlock {
                data: prev_indices,
                delay: step_pending,
                width: ww,
                height: hh,
            });
        }

        Ok(())
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        if self.blocks.is_empty() {
            return Err(Error::GifEncode("No frames to save".to_string()));
        }

        let palette = self
            .palette
            .as_ref()
            .ok_or_else(|| Error::GifEncode("No palette available".to_string()))?;

        let first_block = &self.blocks[0];
        let width = first_block.width;
        let height = first_block.height;

        let mut color_table = Vec::with_capacity(palette.colors_total * 3);
        for i in 0..palette.colors_total {
            color_table.push(palette.red[i]);
            color_table.push(palette.green[i]);
            color_table.push(palette.blue[i]);
        }
        while color_table.len() < 768 {
            color_table.push(0);
        }

        let file = File::create(path.as_ref())?;
        let writer = BufWriter::new(file);

        let mut encoder = Encoder::new(writer, width, height, &color_table)
            .map_err(|e| Error::GifEncode(e.to_string()))?;

        encoder.set_repeat(Repeat::Infinite).map_err(|e| Error::GifEncode(e.to_string()))?;

        for block in &self.blocks {
            let mut frame =
                Frame::from_indexed_pixels(block.width, block.height, block.data.clone(), None);
            frame.delay = block.delay;
            frame.dispose = gif::DisposalMethod::Keep;

            encoder.write_frame(&frame).map_err(|e| Error::GifEncode(e.to_string()))?;
        }

        Ok(())
    }
}
