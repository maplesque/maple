use std::sync::atomic::{AtomicUsize, Ordering};

use image::{Rgba, RgbaImage};
#[cfg(not(target_arch = "wasm32"))]
use rayon::prelude::*;

use crate::{
    error::{Error, Result},
    input::{Input, Inputs},
    mapping::Mapping,
    pixer::{Pixer, sample_linear},
};

static RENDER_COUNT: AtomicUsize = AtomicUsize::new(0);

const RR: f64 = 2048.0; // Half of coordinate range (4096/2)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderQuality {
    None,
    Simple,
    Sampled,
}

impl Default for RenderQuality {
    fn default() -> Self {
        RenderQuality::Sampled
    }
}

#[derive(Debug, Clone)]
pub struct CloudPoint {
    pub layer: u8,
    pub x: f64,
    pub y: f64,
}

pub struct Render {
    mapping: Option<Mapping>,
    out: RgbaImage,
    out_scaled: Option<RgbaImage>,
    #[allow(dead_code)]
    quality: RenderQuality,
}

impl Render {
    pub fn new(quality: RenderQuality) -> Self {
        Render { mapping: None, out: RgbaImage::new(1, 1), out_scaled: None, quality }
    }

    pub fn with_default_quality() -> Self {
        Self::new(RenderQuality::Sampled)
    }

    pub fn attach_mapping(&mut self, mapping: Mapping) {
        self.mapping = Some(mapping);
    }

    fn check(&self) -> Result<&Mapping> {
        self.mapping.as_ref().ok_or(Error::NoMapping)
    }

    fn pre(&mut self) -> Result<()> {
        let mapping = self.check()?;
        self.out = mapping.neutral.clone();
        Ok(())
    }

    /// 9-tap antialiased sampling with standard UV map (parallelized by row chunks)
    fn add(&mut self, input: &Input) -> Result<()> {
        let mapping = self.mapping.as_ref().ok_or(Error::NoMapping)?;

        let active_scale = input.in_scale as f64 / 2.0;
        let w = mapping.light.width() as i32;
        let h = mapping.light.height() as i32;

        if (mapping.map1.width() as i32) < w {
            return Ok(());
        }

        let off = ((mapping.map2.height() as i32 - h) / 2) as i32;

        if mapping.map1.width() != mapping.neutral.width() {
            return Ok(());
        }
        if mapping.map2.width() != mapping.neutral.width() {
            return Ok(());
        }

        let out_w = self.out.width() as usize;
        let _out_h = self.out.height() as usize;
        let input_img = input.get();

        let light_raw = mapping.light.as_raw();
        let dark_raw = mapping.dark.as_raw();
        let map1_raw = mapping.map1.as_raw();
        let map2_raw = mapping.map2.as_raw();

        let row_stride = out_w * 4;
        let map_stride = mapping.map1.width() as usize * 4;
        let out_raw = self.out.as_mut();

        #[cfg(not(target_arch = "wasm32"))]
        let iter = out_raw.par_chunks_mut(row_stride).enumerate();
        #[cfg(target_arch = "wasm32")]
        let iter = out_raw.chunks_mut(row_stride).enumerate();

        iter.for_each(|(y, row)| {
            let map_y_base = (y as i32 + off) as u32;
            if map_y_base >= mapping.map1.height() || map_y_base >= mapping.map2.height() {
                return;
            }

            for x in 0..out_w {
                let idx = x * 4;
                let light_idx = y * row_stride + idx;
                let map_idx = map_y_base as usize * map_stride + idx;

                let light_pixel = &light_raw[light_idx..light_idx + 4];
                let dark_pixel = &dark_raw[light_idx..light_idx + 4];
                let map_pixel = &map1_raw[map_idx..map_idx + 4];
                let sel_pixel = &map2_raw[map_idx..map_idx + 4];

                if sel_pixel[0] != input.layer {
                    continue;
                }

                let act = map_pixel[3] as i32;
                if act <= 25 {
                    continue;
                }

                let b_val = map_pixel[2] as i32;
                let ymod = b_val / 16;
                let xmod = b_val % 16;
                let x1 = map_pixel[0] as f64 + 256.0 * xmod as f64 - RR;
                let y1 = map_pixel[1] as f64 + 256.0 * ymod as f64 - RR;

                let mut x12 = x1;
                let mut y12 = y1;
                let mut x13 = x1;
                let mut y13 = y1;

                if (x as i32) < w - 1 && (y as i32) < h - 1 {
                    let mdx_idx = map_idx + 4;
                    let mdy_idx = map_idx + map_stride;
                    let mdx = &map1_raw[mdx_idx..mdx_idx + 4];
                    let mdy = &map1_raw[mdy_idx..mdy_idx + 4];

                    if mdx[3] > 127 && mdy[3] > 127 {
                        let idx2 = &map2_raw[mdx_idx..mdx_idx + 4];
                        let idx3 = &map2_raw[mdy_idx..mdy_idx + 4];

                        if idx2[0] == input.layer && idx3[0] == input.layer {
                            let mod2 = mdx[2] as i32;
                            let ymod2 = mod2 / 16;
                            let xmod2 = mod2 % 16;
                            x12 = mdx[0] as f64 + 256.0 * xmod2 as f64 - RR;
                            y12 = mdx[1] as f64 + 256.0 * ymod2 as f64 - RR;

                            let mod3 = mdy[2] as i32;
                            let ymod3 = mod3 / 16;
                            let xmod3 = mod3 % 16;
                            x13 = mdy[0] as f64 + 256.0 * xmod3 as f64 - RR;
                            y13 = mdy[1] as f64 + 256.0 * ymod3 as f64 - RR;
                        }

                        // Compare squared distance against the 400.0 threshold to
                        // avoid two `sqrt` calls per pixel in this warp branch.
                        // (da < 400  <=>  da^2 < 160000).
                        let dax = x1 - x12;
                        let day = y1 - y12;
                        let dbx = x1 - x13;
                        let dby = y1 - y13;
                        if dax * dax + day * day > 160_000.0 || dbx * dbx + dby * dby > 160_000.0 {
                            x12 = x1;
                            y12 = y1;
                            x13 = x1;
                            y13 = y1;
                        }
                    }
                }

                let x1s = x1 * input.xs;
                let y1s = y1 * input.ys;
                let xx_rot = input.xa * x1s + input.ya * y1s;
                let yy_rot = -input.ya * x1s + input.xa * y1s;
                let xx = input.in_x0 + active_scale * (xx_rot + RR + input.xo) / RR;
                let yy = input.in_y0 + active_scale * (yy_rot + RR + input.yo) / RR;

                let x12s = x12 * input.xs;
                let y12s = y12 * input.ys;
                let xxa_rot = input.xa * x12s + input.ya * y12s;
                let yya_rot = -input.ya * x12s + input.xa * y12s;
                let xxa = input.in_x0 + active_scale * (xxa_rot + RR + input.xo) / RR - xx;
                let yya = input.in_y0 + active_scale * (yya_rot + RR + input.yo) / RR - yy;

                let x13s = x13 * input.xs;
                let y13s = y13 * input.ys;
                let xxb_rot = input.xa * x13s + input.ya * y13s;
                let yyb_rot = -input.ya * x13s + input.xa * y13s;
                let xxb = input.in_x0 + active_scale * (xxb_rot + RR + input.xo) / RR - xx;
                let yyb = input.in_y0 + active_scale * (yyb_rot + RR + input.yo) / RR - yy;

                let mut mo = sample_linear(input_img, xx, yy);
                let m2 = sample_linear(input_img, xx + xxa / 2.0, yy + yya / 2.0);
                let m3 = sample_linear(input_img, xx - xxa / 2.0, yy - yya / 2.0);
                let m4 = sample_linear(input_img, xx + xxb / 2.0, yy + yyb / 2.0);
                let m5 = sample_linear(input_img, xx - xxb / 2.0, yy - yyb / 2.0);
                let m2b = sample_linear(input_img, xx + (xxa + xxb) / 2.0, yy + (yya + yyb) / 2.0);
                let m3b = sample_linear(input_img, xx + (xxa - xxb) / 2.0, yy + (yya - yyb) / 2.0);
                let m4b = sample_linear(input_img, xx - (xxa + xxb) / 2.0, yy - (yya + yyb) / 2.0);
                let m5b = sample_linear(input_img, xx - (xxa - xxb) / 2.0, yy - (yya - yyb) / 2.0);

                let mut mo_p = mo;
                mo_p.preblend();
                let mut m2_p = m2;
                m2_p.preblend();
                let mut m3_p = m3;
                m3_p.preblend();
                let mut m4_p = m4;
                m4_p.preblend();
                let mut m5_p = m5;
                m5_p.preblend();
                let mut m2b_p = m2b;
                m2b_p.preblend();
                let mut m3b_p = m3b;
                m3b_p.preblend();
                let mut m4b_p = m4b;
                m4b_p.preblend();
                let mut m5b_p = m5b;
                m5b_p.preblend();

                let sc = (mo.a * 4.0
                    + (m2.a + m3.a + m4.a + m5.a) * 2.0
                    + (m2b.a + m3b.a + m4b.a + m5b.a))
                    / 16.0;

                mo = (mo_p * 4.0
                    + (m2_p + m3_p + m4_p + m5_p) * 2.0
                    + (m2b_p + m3b_p + m4b_p + m5b_p))
                    / 16.0;

                if sc > 0.0001 {
                    mo.postblend(sc);
                } else {
                    mo.r = 0.0;
                    mo.g = 0.0;
                    mo.b = 0.0;
                    mo.a = 0.0;
                }

                let m_r = mo.r as i32;
                let m_g = mo.g as i32;
                let m_b = mo.b as i32;
                let m_a = mo.a as i32;

                let result_r = dark_pixel[0] as i32
                    + ((light_pixel[0] as i32 - dark_pixel[0] as i32) * m_r) / 255;
                let result_g = dark_pixel[1] as i32
                    + ((light_pixel[1] as i32 - dark_pixel[1] as i32) * m_g) / 255;
                let result_b = dark_pixel[2] as i32
                    + ((light_pixel[2] as i32 - dark_pixel[2] as i32) * m_b) / 255;
                let mut result_a = m_a;

                if (dark_pixel[3] as i32) < result_a {
                    result_a = dark_pixel[3] as i32;
                }

                if result_a > 0 {
                    let idx = x * 4;
                    if result_a > 250 {
                        row[idx] = result_r.clamp(0, 255) as u8;
                        row[idx + 1] = result_g.clamp(0, 255) as u8;
                        row[idx + 2] = result_b.clamp(0, 255) as u8;
                    } else {
                        row[idx] = (row[idx] as i32
                            + ((result_r - row[idx] as i32) * result_a) / 255)
                            .clamp(0, 255) as u8;
                        row[idx + 1] = (row[idx + 1] as i32
                            + ((result_g - row[idx + 1] as i32) * result_a) / 255)
                            .clamp(0, 255) as u8;
                        row[idx + 2] = (row[idx + 2] as i32
                            + ((result_b - row[idx + 2] as i32) * result_a) / 255)
                            .clamp(0, 255) as u8;
                    }
                }
            }
        });

        Ok(())
    }

    #[allow(dead_code)]
    fn add_simple(&mut self, input: &Input) -> Result<()> {
        let mapping = self.mapping.as_ref().ok_or(Error::NoMapping)?;

        let active_scale = input.in_scale as f64 / 2.0;
        let off = ((mapping.map2.height() as i32 - mapping.light.height() as i32) / 2) as i32;

        if (mapping.map1.width() as i32) < (mapping.light.width() as i32) {
            return Ok(());
        }

        let out_w = self.out.width();
        let out_h = self.out.height();

        for y in 0..out_h {
            for x in 0..out_w {
                let light_pixel = mapping.light.get_pixel(x, y);
                let dark_pixel = mapping.dark.get_pixel(x, y);

                let map_y = (y as i32 + off) as u32;
                if map_y >= mapping.map1.height() || map_y >= mapping.map2.height() {
                    continue;
                }

                let map_pixel = mapping.map1.get_pixel(x, map_y);
                let sel_pixel = mapping.map2.get_pixel(x, map_y);

                let b_val = map_pixel[2] as i32;
                let ymod = b_val / 16;
                let xmod = b_val % 16;
                let act = map_pixel[3] as i32;
                let x1 = (map_pixel[0] as f64 + 256.0 * xmod as f64 - RR) * input.xs;
                let y1 = (map_pixel[1] as f64 + 256.0 * ymod as f64 - RR) * input.ys;

                let xx_rot = input.xa * x1 + input.ya * y1;
                let yy_rot = -input.ya * x1 + input.xa * y1;
                let xx = input.in_x0 + active_scale * (xx_rot + RR + input.xo) / RR;
                let yy = input.in_y0 + active_scale * (yy_rot + RR + input.yo) / RR;

                let m = input.safe_pixel(xx as i32, yy as i32);

                if sel_pixel[0] == input.layer && act > 25 {
                    let result_r = dark_pixel[0] as i32
                        + ((light_pixel[0] as i32 - dark_pixel[0] as i32) * m[0] as i32) / 255;
                    let result_g = dark_pixel[1] as i32
                        + ((light_pixel[1] as i32 - dark_pixel[1] as i32) * m[1] as i32) / 255;
                    let result_b = dark_pixel[2] as i32
                        + ((light_pixel[2] as i32 - dark_pixel[2] as i32) * m[2] as i32) / 255;
                    let mut result_a = m[3] as i32;

                    if (dark_pixel[3] as i32) < result_a {
                        result_a = dark_pixel[3] as i32;
                    }

                    if result_a > 0 {
                        let out_pixel = self.out.get_pixel_mut(x, y);
                        if result_a > 250 {
                            out_pixel[0] = result_r.clamp(0, 255) as u8;
                            out_pixel[1] = result_g.clamp(0, 255) as u8;
                            out_pixel[2] = result_b.clamp(0, 255) as u8;
                        } else {
                            out_pixel[0] = (out_pixel[0] as i32
                                + ((result_r - out_pixel[0] as i32) * result_a) / 255)
                                .clamp(0, 255) as u8;
                            out_pixel[1] = (out_pixel[1] as i32
                                + ((result_g - out_pixel[1] as i32) * result_a) / 255)
                                .clamp(0, 255) as u8;
                            out_pixel[2] = (out_pixel[2] as i32
                                + ((result_b - out_pixel[2] as i32) * result_a) / 255)
                                .clamp(0, 255) as u8;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Edge smoothing post-process
    fn post(&mut self) -> Result<()> {
        RENDER_COUNT.fetch_add(1, Ordering::SeqCst);

        let mapping = self.mapping.as_ref().ok_or(Error::NoMapping)?;
        let pre = self.out.clone();
        let w = self.out.width() as i32;
        let h = self.out.height() as i32;

        if (mapping.map1.width() as i32) < (mapping.light.width() as i32) {
            return Ok(());
        }

        if mapping.map1.width() != mapping.neutral.width() {
            return Ok(());
        }
        if mapping.map2.width() != mapping.neutral.width() {
            return Ok(());
        }

        for y in 0..h {
            for x in 0..w {
                let xu = x as u32;
                let yu = y as u32;
                let sel_pixel = mapping.map2.get_pixel(xu, yu);

                if sel_pixel[2] > 0 {
                    if x > 0 && y > 0 && x < w - 1 && y < h - 1 {
                        let back1 = pre.get_pixel((x - 1) as u32, yu);
                        let idx1 = mapping.map2.get_pixel((x - 1) as u32, yu);

                        let back2 = pre.get_pixel((x + 1) as u32, yu);
                        let idx2 = mapping.map2.get_pixel((x + 1) as u32, yu);

                        let back3 = pre.get_pixel(xu, (y - 1) as u32);
                        let idx3 = mapping.map2.get_pixel(xu, (y - 1) as u32);

                        let back4 = pre.get_pixel(xu, (y + 1) as u32);
                        let idx4 = mapping.map2.get_pixel(xu, (y + 1) as u32);

                        let mut total = Pixer::new();
                        let mut ct = 0.0;

                        if idx1[2] < 127 {
                            total.add_rgba(back1);
                            ct += 1.0;
                        }
                        if idx2[2] < 127 {
                            total.add_rgba(back2);
                            ct += 1.0;
                        }
                        if idx3[2] < 127 {
                            total.add_rgba(back3);
                            ct += 1.0;
                        }
                        if idx4[2] < 127 {
                            total.add_rgba(back4);
                            ct += 1.0;
                        }

                        if ct > 0.5 {
                            total.div(ct);
                            let out_pixel = self.out.get_pixel_mut(xu, yu);
                            out_pixel[0] = total.r.clamp(0.0, 255.0) as u8;
                            out_pixel[1] = total.g.clamp(0.0, 255.0) as u8;
                            out_pixel[2] = total.b.clamp(0.0, 255.0) as u8;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn apply(&mut self, inputs: &Inputs) -> Result<()> {
        self.apply_scaled(inputs, -1, -1)
    }

    pub fn apply_scaled(&mut self, inputs: &Inputs, w: i32, h: i32) -> Result<()> {
        self.pre()?;

        for input in inputs.iter() {
            self.add(input)?;
        }

        self.post()?;

        if w > 0 && h > 0 && (w != self.out.width() as i32 || h != self.out.height() as i32) {
            let wi = self.out.width() as i32;
            let hi = self.out.height() as i32;

            let fi = wi as f64 / hi as f64;
            let f = w as f64 / h as f64;

            let (xo, yo, wo, ho) = if fi > f + 0.001 {
                let wo = w;
                let ho = (wo as f64 / fi) as i32;
                let yo = (h - ho) / 2;
                (0, yo, wo, ho)
            } else if fi < f - 0.001 {
                let ho = h;
                let wo = (h as f64 * fi) as i32;
                let xo = (w - wo) / 2;
                (xo, 0, wo, ho)
            } else {
                (0, 0, w, h)
            };

            let mut scaled = RgbaImage::from_pixel(w as u32, h as u32, Rgba([255, 255, 255, 0]));

            for dy in 0..ho {
                for dx in 0..wo {
                    let sx = (dx as f64 * wi as f64 / wo as f64) as u32;
                    let sy = (dy as f64 * hi as f64 / ho as f64) as u32;
                    let sx = sx.min(self.out.width() - 1);
                    let sy = sy.min(self.out.height() - 1);
                    let pixel = *self.out.get_pixel(sx, sy);
                    scaled.put_pixel((xo + dx) as u32, (yo + dy) as u32, pixel);
                }
            }

            for pixel in scaled.pixels_mut() {
                pixel[3] = 255;
            }

            self.out_scaled = Some(scaled);
            self.out = RgbaImage::new(1, 1);
        }

        Ok(())
    }

    pub fn get(&self) -> &RgbaImage {
        self.out_scaled.as_ref().unwrap_or(&self.out)
    }

    pub fn get_mut(&mut self) -> &mut RgbaImage {
        if self.out_scaled.is_some() { self.out_scaled.as_mut().unwrap() } else { &mut self.out }
    }

    pub fn save<P: AsRef<std::path::Path>>(&self, path: P) -> Result<()> {
        self.get().save(path)?;
        Ok(())
    }

    pub fn render_count() -> usize {
        RENDER_COUNT.load(Ordering::SeqCst)
    }

    pub fn get_cloud(&self, input: &Input) -> Result<Vec<CloudPoint>> {
        let mapping = self.mapping.as_ref().ok_or(Error::NoMapping)?;

        let active_scale = input.in_scale as f64 / 2.0;
        let off = ((mapping.map2.height() as i32 - mapping.light.height() as i32) / 2) as i32;

        if (mapping.map1.width() as i32) < (mapping.light.width() as i32) {
            return Ok(Vec::new());
        }

        let mut cloud = Vec::new();
        let w = mapping.light.width();
        let h = mapping.light.height();

        for y in 0..h {
            for x in 0..w {
                let light_pixel = mapping.light.get_pixel(x, y);
                let dark_pixel = mapping.dark.get_pixel(x, y);

                let map_y = (y as i32 + off) as u32;
                if map_y >= mapping.map1.height() || map_y >= mapping.map2.height() {
                    continue;
                }

                let map_pixel = mapping.map1.get_pixel(x, map_y);
                let sel_pixel = mapping.map2.get_pixel(x, map_y);

                let b_val = map_pixel[2] as i32;
                let ymod = b_val / 16;
                let xmod = b_val % 16;
                let act = map_pixel[3] as i32;
                let x1 = (map_pixel[0] as f64 + 256.0 * xmod as f64 - RR) * input.xs;
                let y1 = (map_pixel[1] as f64 + 256.0 * ymod as f64 - RR) * input.ys;

                let xx_rot = input.xa * x1 + input.ya * y1;
                let yy_rot = -input.ya * x1 + input.xa * y1;
                let xx = input.in_x0 + active_scale * (xx_rot + RR + input.xo) / RR;
                let yy = input.in_y0 + active_scale * (yy_rot + RR + input.yo) / RR;

                if sel_pixel[0] != 0 && act > 25 {
                    let del = 5i32;
                    let r_diff = (light_pixel[0] as i32 - dark_pixel[0] as i32).abs();
                    let g_diff = (light_pixel[1] as i32 - dark_pixel[1] as i32).abs();
                    let b_diff = (light_pixel[2] as i32 - dark_pixel[2] as i32).abs();

                    if (r_diff > del || g_diff > del || b_diff > del)
                        && dark_pixel[3] > 100
                        && light_pixel[3] > 100
                    {
                        cloud.push(CloudPoint { layer: sel_pixel[0], x: xx, y: yy });
                    }
                }
            }
        }

        Ok(cloud)
    }

    pub fn auto_zoom_input(&self, input: &mut Input) -> Result<bool> {
        let mapping = self.mapping.as_ref().ok_or(Error::NoMapping)?;

        let active_scale = input.in_scale as f64 / 2.0;
        let off = ((mapping.map2.height() as i32 - mapping.light.height() as i32) / 2) as i32;

        if (mapping.map1.width() as i32) < (mapping.light.width() as i32) {
            return Ok(false);
        }

        let mut x_min = input.width() as f64;
        let mut x_max = 0.0f64;
        let mut y_min = input.height() as f64;
        let mut y_max = 0.0f64;

        let w = mapping.light.width();
        let h = mapping.light.height();

        for y in 0..h {
            for x in 0..w {
                let map_y = (y as i32 + off) as u32;
                if map_y >= mapping.map1.height() || map_y >= mapping.map2.height() {
                    continue;
                }

                let map_pixel = mapping.map1.get_pixel(x, map_y);
                let sel_pixel = mapping.map2.get_pixel(x, map_y);

                let b_val = map_pixel[2] as i32;
                let ymod = b_val / 16;
                let xmod = b_val % 16;
                let act = map_pixel[3] as i32;
                let x1 = (map_pixel[0] as f64 + 256.0 * xmod as f64 - RR) * input.xs;
                let y1 = (map_pixel[1] as f64 + 256.0 * ymod as f64 - RR) * input.ys;

                let xx_rot = input.xa * x1 + input.ya * y1;
                let yy_rot = -input.ya * x1 + input.xa * y1;
                let xx = input.in_x0 + active_scale * (xx_rot + RR + input.xo) / RR;
                let yy = input.in_y0 + active_scale * (yy_rot + RR + input.yo) / RR;

                // Only check layer 1 for auto-zoom
                // Only check layer 1 for auto-zoom
                if sel_pixel[0] == 1 && act > 25 {
                    if xx < x_min {
                        x_min = xx;
                    }
                    if xx > x_max {
                        x_max = xx;
                    }
                    if yy < y_min {
                        y_min = yy;
                    }
                    if yy > y_max {
                        y_max = yy;
                    }
                }
            }
        }

        let hh = input.height() as f64;
        if y_max - y_min < hh * 0.75 {
            input.xs *= 2.0;
            input.ys *= 2.0;
            return Ok(true);
        }

        Ok(false)
    }

    pub fn auto_zoom(&self, inputs: &mut Inputs) -> Result<bool> {
        let mut changed = false;
        for input in inputs.iter_mut() {
            if self.auto_zoom_input(input)? {
                changed = true;
            }
        }
        Ok(changed)
    }
}
