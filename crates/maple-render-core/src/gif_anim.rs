use std::{
    fs::File,
    io::{BufWriter, Cursor},
    path::Path,
};

use gif::{Encoder, Frame, Repeat};
use image::RgbaImage;

use crate::{
    error::{Error, Result},
    quantize::{Palette, Quantizer},
    renders::Renders,
};

pub struct GifAnim {
    renders: Renders,
    /// Explicit palette frames set by the caller. When `None`, the template's
    /// own palette (from `template.json`) is used.
    palette_frames: Option<Vec<i32>>,
    /// Explicit timing set by the caller. When `None`, the template's own
    /// delay/hold (from `template.json`) is used.
    period: Option<f64>,
    hold: Option<f64>,
    first_frame: i32,
    blocks: Vec<GifBlock>,
    palette: Option<Palette>,
    dither: bool,
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
            palette_frames: None,
            period: None,
            hold: None,
            first_frame: -1,
            blocks: Vec::new(),
            palette: None,
            dither: false,
        }
    }

    pub fn set_dither(&mut self, dither: bool) {
        self.dither = dither;
    }

    pub fn set_palette(&mut self, index: i32) {
        self.palette_frames = Some(vec![index]);
    }

    pub fn set_palette_frames(&mut self, frames: Vec<i32>) {
        self.palette_frames = Some(frames);
    }

    pub fn set_first_frame(&mut self, index: i32) {
        self.first_frame = index;
    }

    pub fn set_timing(&mut self, period: f64, hold: f64) {
        self.period = Some(period);
        self.hold = Some(hold);
    }

    pub fn apply(&mut self) -> Result<()> {
        let palette_frames =
            self.palette_frames.clone().unwrap_or_else(|| self.renders.repo().get_palette());
        let period = self.period.unwrap_or_else(|| self.renders.repo().get_period());
        let hold = self.hold.unwrap_or_else(|| self.renders.repo().get_hold());

        let step = (period * 100.0 + 0.5) as u16;
        let last_step = (hold * 100.0 + 0.5) as u16;

        let pals = palette_frames.len();
        if pals < 1 {
            return Err(Error::MissingData("No palette frames specified".to_string()));
        }

        let mut pal_image: Option<RgbaImage> = None;
        let mut pal_index: i32 = -1;

        for (i, &idx) in palette_frames.iter().enumerate() {
            if pals == 1 {
                let render = self.renders.get_render(idx)?;
                pal_image = Some(render.get().clone());
                pal_index = idx;
                self.renders.remove_mapping(idx);
                continue;
            }

            {
                let render = self.renders.get_render(idx)?;
                let img = render.get();
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

            self.renders.remove_mapping(idx);
        }

        let pal_image = pal_image.ok_or(Error::MissingData("No palette image".to_string()))?;
        let ww = pal_image.width() as u16;
        let hh = pal_image.height() as u16;

        let mut quantizer = Quantizer::new(&pal_image);
        self.palette = Some(quantizer.palette().clone());

        let mut prev_indices = quantizer.quantize(&pal_image, self.dither);

        let frames = self.renders.length() as i32;

        let mut _emit_ct = 0;
        let mut step_pending: u16 = 0;
        let mut pending_frame_number = pal_index;

        for base in 0..frames {
            let i = if self.first_frame >= 0 { (base + self.first_frame) % frames } else { base };

            step_pending = step_pending.saturating_add(step);

            let curr_indices = if i != pal_index {
                let quantized = {
                    let render = self.renders.get_render(i)?;
                    quantizer.quantize(render.get(), self.dither)
                };
                self.renders.remove_mapping(i);
                quantized
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
                let emitted_indices = std::mem::replace(&mut prev_indices, curr_indices);
                self.blocks.push(GifBlock { data: emitted_indices, delay, width: ww, height: hh });
                step_pending = step;
                _emit_ct += 1;
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
        let (width, height, color_table) = self.build_palette_table()?;
        let file = File::create(path.as_ref())?;
        let writer = BufWriter::new(file);
        self.write_blocks(writer, width, height, &color_table)?;

        Ok(())
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        let (width, height, color_table) = self.build_palette_table()?;
        let mut out = Vec::new();
        let writer = BufWriter::new(Cursor::new(&mut out));
        self.write_blocks(writer, width, height, &color_table)?;
        Ok(out)
    }

    fn build_palette_table(&self) -> Result<(u16, u16, Vec<u8>)> {
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

        Ok((width, height, color_table))
    }

    fn write_blocks<W: std::io::Write>(
        &self,
        writer: W,
        width: u16,
        height: u16,
        color_table: &[u8],
    ) -> Result<()> {
        let mut encoder = Encoder::new(writer, width, height, color_table)
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
