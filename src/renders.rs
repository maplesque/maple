use std::collections::HashMap;

use crate::{
    error::{Error, Result},
    input::{Input, Inputs},
    render::{Render, RenderQuality},
    repository::Repository,
};

pub struct Renders {
    repo: Repository,
    inputs: Inputs,
    renders: HashMap<i32, Render>,
    target_width: i32,
    target_height: i32,
    quality: RenderQuality,
    peak_cache_count: usize,
}

impl Renders {
    pub fn new(repo: Repository, inputs: Inputs, quality: RenderQuality) -> Self {
        Renders {
            repo,
            inputs,
            renders: HashMap::new(),
            target_width: -1,
            target_height: -1,
            quality,
            peak_cache_count: 0,
        }
    }

    pub fn set_size(&mut self, w: i32, h: i32) {
        self.target_width = w;
        self.target_height = h;
    }

    fn check(&self) -> Result<()> {
        if self.inputs.is_empty() {
            return Err(Error::NoInputs);
        }
        Ok(())
    }

    pub fn get_render(&mut self, index: i32) -> Result<&Render> {
        self.check()?;

        if !self.renders.contains_key(&index) {
            let mut render = Render::new(self.quality);
            let mapping = self.repo.take_mapping(index)?;
            render.attach_mapping(mapping);
            render.apply_scaled(&self.inputs, self.target_width, self.target_height)?;

            self.renders.insert(index, render);

            if self.renders.len() > self.peak_cache_count {
                self.peak_cache_count = self.renders.len();
            }
        }

        Ok(self.renders.get(&index).unwrap())
    }

    pub fn get_render_mut(&mut self, index: i32) -> Result<&mut Render> {
        self.check()?;

        if !self.renders.contains_key(&index) {
            let mut render = Render::new(self.quality);
            let mapping = self.repo.take_mapping(index)?;
            render.attach_mapping(mapping);
            render.apply_scaled(&self.inputs, self.target_width, self.target_height)?;

            self.renders.insert(index, render);

            if self.renders.len() > self.peak_cache_count {
                self.peak_cache_count = self.renders.len();
            }
        }

        Ok(self.renders.get_mut(&index).unwrap())
    }

    pub fn remove_render(&mut self, index: i32) {
        self.renders.remove(&index);
        self.repo.remove_mapping(index);
    }

    pub fn remove_mapping(&mut self, index: i32) {
        self.repo.remove_mapping(index);
    }

    pub fn length(&self) -> u32 {
        self.repo.length()
    }

    pub fn peak(&self) -> usize {
        self.peak_cache_count
    }

    pub fn repo(&self) -> &Repository {
        &self.repo
    }

    pub fn repo_mut(&mut self) -> &mut Repository {
        &mut self.repo
    }

    pub fn inputs(&self) -> &Inputs {
        &self.inputs
    }

    pub fn inputs_mut(&mut self) -> &mut Inputs {
        &mut self.inputs
    }

    pub fn auto_zoom(&mut self) -> Result<bool> {
        self.check()?;

        let mut render = Render::new(self.quality);
        let mapping = self.repo.get_mapping(0)?.clone();
        render.attach_mapping(mapping);

        render.auto_zoom(&mut self.inputs)
    }

    /// Optimal positioning using Delaunay triangulation
    pub fn scan(&mut self, frame: i32) -> Result<()> {
        let frame = if frame < 0 { 0 } else { frame };

        self.check()?;

        if self.inputs.is_empty() {
            return Ok(());
        }

        let num_inputs = self.inputs.len();

        for k in 0..num_inputs {
            let mut render = Render::new(self.quality);
            let mapping = self.repo.get_mapping(frame)?.clone();
            render.attach_mapping(mapping);

            let input = &self.inputs[k];
            let pts = render.get_cloud(input)?;

            let layer = input.layer;
            let filtered_pts: Vec<_> = pts.iter().filter(|p| p.layer == layer).collect();

            if filtered_pts.is_empty() {
                continue;
            }

            let mut x0 = filtered_pts[0].x;
            let mut y0 = filtered_pts[0].y;
            let mut x1 = x0;
            let mut y1 = y0;
            let mut tx = 0.0f64;
            let mut ty = 0.0f64;

            for pt in &filtered_pts {
                if pt.x < x0 {
                    x0 = pt.x;
                }
                if pt.x > x1 {
                    x1 = pt.x;
                }
                if pt.y < y0 {
                    y0 = pt.y;
                }
                if pt.y > y1 {
                    y1 = pt.y;
                }
                tx += pt.x;
                ty += pt.y;
            }

            let ct = filtered_pts.len() as f64;
            if ct > 0.0 {
                let _ = tx / ct;
                let _ = ty / ct;
            }

            tx = (x0 + x1) / 2.0;
            ty = (y0 + y1) / 2.0;

            let dx = x0 + (x1 - x0);
            let dy = y0 + (y1 - y0);

            let pts_shifted: Vec<_> = filtered_pts
                .iter()
                .map(|p| delaunator::Point { x: p.x + dx, y: p.y + dy })
                .collect();

            if pts_shifted.len() < 3 {
                continue;
            }

            let triangulation = delaunator::triangulate(&pts_shifted);

            let input_img = input.get();
            let input_ref = &self.inputs[k];

            let result = self.tweak_scale(
                input_img,
                x0 + dx,
                y0 + dy,
                x1 + dx,
                y1 + dy,
                dx,
                dy,
                tx + dx,
                ty + dy,
                &triangulation,
                &pts_shifted,
                input_ref,
            );

            if let Some((scale, _x_adj, y_adj)) = result {
                let input_mut = &mut self.inputs.get_mut()[k];
                input_mut.xs /= scale;
                input_mut.ys /= scale;
                input_mut.in_y0 += y_adj;
            }
        }

        Ok(())
    }

    fn tweak_scale(
        &self,
        img: &image::RgbaImage,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        dx: f64,
        dy: f64,
        tx: f64,
        ty: f64,
        triangulation: &delaunator::Triangulation,
        points: &[delaunator::Point],
        input: &Input,
    ) -> Option<(f64, f64, f64)> {
        let mut actives = 0;
        let mut ix0 = 0.0f64;
        let mut iy0 = 0.0f64;
        let mut ix1 = 0.0f64;
        let mut iy1 = 0.0f64;
        let mut first = true;

        for (x, y, pixel) in img.enumerate_pixels() {
            if Self::is_active(pixel) {
                actives += 1;
                let xf = x as f64;
                let yf = y as f64;
                if first {
                    ix0 = xf;
                    ix1 = xf;
                    iy0 = yf;
                    iy1 = yf;
                    first = false;
                } else {
                    if xf > ix1 {
                        ix1 = xf;
                    }
                    if xf < ix0 {
                        ix0 = xf;
                    }
                    if yf > iy1 {
                        iy1 = yf;
                    }
                    if yf < iy0 {
                        iy0 = yf;
                    }
                }
            }
        }

        if actives >= (img.width() * img.height() * 9 / 10) as i32 {
            return None;
        }

        let ix = (ix0 + ix1) / 2.0;
        let iy = (iy0 + iy1) / 2.0;
        let xt = tx - dx;
        let yt = ty - dy;

        // Binary search for optimal scale
        let blo = 0.1f64;
        let bhi = 6.0f64;
        let mut lo = blo;
        let mut hi = bhi;
        let mut good_scale = 0.01f64;
        let mut decent_scale = 1.0f64;
        let mut decent_scale_value = -1.0f64;

        while (lo - hi).abs() > 0.01 {
            let curr = (lo + hi) / 2.0;
            let (nin, nout) = self.tweak_one(
                img,
                x0,
                y0,
                x1,
                y1,
                dx,
                dy,
                xt,
                yt,
                ix,
                iy,
                triangulation,
                points,
                curr,
            );

            let q = if nout == 0 { 1.0 } else { nin as f64 / (nin + nout) as f64 };
            let v = Self::eval(q, curr);

            if v > decent_scale_value {
                decent_scale = curr;
                decent_scale_value = v;
            }

            if q > 0.9999 {
                if curr > good_scale {
                    good_scale = curr;
                }
                lo = curr;
            } else {
                hi = curr;
            }
        }

        // Linear search for refinement
        let steps = 20;
        for i in 1..=steps {
            let curr = blo + (bhi - blo) * (i as f64 / steps as f64);
            let (nin, nout) = self.tweak_one(
                img,
                x0,
                y0,
                x1,
                y1,
                dx,
                dy,
                xt,
                yt,
                ix,
                iy,
                triangulation,
                points,
                curr,
            );

            let q = if nout == 0 { 1.0 } else { nin as f64 / (nin + nout) as f64 };
            let v = Self::eval(q, curr);

            if v > decent_scale_value {
                decent_scale = curr;
                decent_scale_value = v;
            }
        }

        good_scale = decent_scale * 0.97;

        let y_adj = Self::remap(good_scale, yt, iy, input.in_y0, input);

        Some((good_scale, 0.0, y_adj))
    }

    fn eval(q: f64, curr: f64) -> f64 {
        q * curr.sqrt() + if q >= 0.9999 { 1.0 } else { 0.0 }
    }

    fn remap(scale: f64, v: f64, i: f64, delta: f64, input: &Input) -> f64 {
        let scale = 1.0 / scale;
        i - (scale * v - input.in_scale as f64 / 2.0 * (scale - 1.0) - scale * delta) - delta
    }

    fn tweak_one(
        &self,
        img: &image::RgbaImage,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        dx: f64,
        dy: f64,
        xt: f64,
        yt: f64,
        ix: f64,
        iy: f64,
        triangulation: &delaunator::Triangulation,
        points: &[delaunator::Point],
        scale: f64,
    ) -> (i32, i32) {
        let mut nin = 0;
        let mut nout = 0;

        for (x, y, pixel) in img.enumerate_pixels() {
            if Self::is_active(pixel) {
                let xx = dx + (x as f64 - ix) * scale + xt;
                let yy = dy + (y as f64 - iy) * scale + yt;

                let dud = if xx < x0 || xx > x1 || yy < y0 || yy > y1 {
                    true
                } else {
                    !Self::point_in_triangulation(xx, yy, triangulation, points, x0, y0, x1, y1)
                };

                if dud {
                    nout += 1;
                } else {
                    nin += 1;
                }
            }
        }

        (nin, nout)
    }

    fn is_active(pixel: &image::Rgba<u8>) -> bool {
        pixel[3] > 25 && (pixel[0] < 250 || pixel[1] < 250 || pixel[2] < 250)
    }

    fn point_in_triangulation(
        x: f64,
        y: f64,
        triangulation: &delaunator::Triangulation,
        points: &[delaunator::Point],
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    ) -> bool {
        let num_triangles = triangulation.triangles.len() / 3;

        for t in 0..num_triangles {
            let i0 = triangulation.triangles[t * 3];
            let i1 = triangulation.triangles[t * 3 + 1];
            let i2 = triangulation.triangles[t * 3 + 2];

            let p0 = &points[i0];
            let p1 = &points[i1];
            let p2 = &points[i2];

            // Skip degenerate triangles
            let d01 = ((p0.x - p1.x).powi(2) + (p0.y - p1.y).powi(2)).sqrt();
            let d12 = ((p1.x - p2.x).powi(2) + (p1.y - p2.y).powi(2)).sqrt();
            let d20 = ((p2.x - p0.x).powi(2) + (p2.y - p0.y).powi(2)).sqrt();

            if d01 > 100.0 || d12 > 100.0 || d20 > 100.0 {
                continue;
            }

            if p0.x < x0
                || p0.x > x1
                || p0.y < y0
                || p0.y > y1
                || p1.x < x0
                || p1.x > x1
                || p1.y < y0
                || p1.y > y1
                || p2.x < x0
                || p2.x > x1
                || p2.y < y0
                || p2.y > y1
            {
                continue;
            }

            if Self::point_in_triangle(x, y, p0.x, p0.y, p1.x, p1.y, p2.x, p2.y) {
                return true;
            }
        }

        false
    }

    fn point_in_triangle(
        x: f64,
        y: f64,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    ) -> bool {
        let area = 0.5 * (-y1 * x2 + y0 * (-x1 + x2) + x0 * (y1 - y2) + x1 * y2);
        let s = (y0 * x2 - x0 * y2 + (y2 - y0) * x + (x0 - x2) * y) / (2.0 * area);
        let t = (x0 * y1 - y0 * x1 + (y0 - y1) * x + (x1 - x0) * y) / (2.0 * area);

        s >= 0.0 && t >= 0.0 && (1.0 - s - t) >= 0.0
    }
}
