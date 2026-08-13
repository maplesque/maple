//! Median-cut color quantization with Floyd-Steinberg dithering.
use image::RgbaImage;

const HIST_C0_BITS: usize = 5; // R
const HIST_C1_BITS: usize = 6; // G
const HIST_C2_BITS: usize = 5; // B

const HIST_C0_ELEMS: usize = 1 << HIST_C0_BITS;
const HIST_C1_ELEMS: usize = 1 << HIST_C1_BITS;
const HIST_C2_ELEMS: usize = 1 << HIST_C2_BITS;
const HIST_ELEMS: usize = HIST_C0_ELEMS * HIST_C1_ELEMS * HIST_C2_ELEMS;

const C0_SHIFT: usize = 8 - HIST_C0_BITS;
const C1_SHIFT: usize = 8 - HIST_C1_BITS;
const C2_SHIFT: usize = 8 - HIST_C2_BITS;

// Distance scale factors (G > R > B perceptual weighting). /shrug
const C0_SCALE: i32 = 2;
const C1_SCALE: i32 = 3;
const C2_SCALE: i32 = 1;

// Box subdivision parameters
const BOX_C0_LOG: usize = HIST_C0_BITS - 3;
const BOX_C1_LOG: usize = HIST_C1_BITS - 3;
const BOX_C2_LOG: usize = HIST_C2_BITS - 3;

const BOX_C0_ELEMS: usize = 1 << BOX_C0_LOG;
const BOX_C1_ELEMS: usize = 1 << BOX_C1_LOG;
const BOX_C2_ELEMS: usize = 1 << BOX_C2_LOG;

const BOX_C0_SHIFT: usize = C0_SHIFT + BOX_C0_LOG;
const BOX_C1_SHIFT: usize = C1_SHIFT + BOX_C1_LOG;
const BOX_C2_SHIFT: usize = C2_SHIFT + BOX_C2_LOG;

const MAXJSAMPLE: i32 = 255;
const MAXNUMCOLORS: usize = 256;

#[derive(Clone)]
pub struct Palette {
    pub red: [u8; 256],
    pub green: [u8; 256],
    pub blue: [u8; 256],
    pub alpha: [u8; 256],
    pub colors_total: usize,
}

impl Default for Palette {
    fn default() -> Self {
        Palette {
            red: [0; 256],
            green: [0; 256],
            blue: [0; 256],
            alpha: [255; 256],
            colors_total: 0,
        }
    }
}

impl Palette {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, idx: usize) -> (u8, u8, u8, u8) {
        (self.red[idx], self.green[idx], self.blue[idx], self.alpha[idx])
    }

    pub fn set(&mut self, idx: usize, r: u8, g: u8, b: u8) {
        self.red[idx] = r;
        self.green[idx] = g;
        self.blue[idx] = b;
        self.alpha[idx] = 255;
    }
}

#[derive(Clone, Copy, Default)]
struct ColorBox {
    c0min: i32,
    c0max: i32,
    c1min: i32,
    c1max: i32,
    c2min: i32,
    c2max: i32,
    volume: i64,
    colorcount: i64,
}

pub struct Quantizer {
    histogram: Box<[u16; HIST_ELEMS]>,
    fserrors: Vec<i16>,
    error_limiter: Vec<i32>,
    on_odd_row: bool,
    palette: Palette,
}

impl Quantizer {
    pub fn new(reference: &RgbaImage) -> Self {
        let mut q = Quantizer {
            histogram: Box::new([0; HIST_ELEMS]),
            fserrors: Vec::new(),
            error_limiter: Vec::new(),
            on_odd_row: false,
            palette: Palette::new(),
        };

        q.init_error_limit();

        let width = reference.width() as usize;
        q.fserrors = vec![0i16; (width + 2) * 3];

        q.prescan_quantize(reference);
        q.select_colors(MAXNUMCOLORS);
        q.zero_histogram();

        q
    }

    fn init_error_limit(&mut self) {
        self.error_limiter = vec![0i32; (MAXJSAMPLE * 2 + 1) as usize];
        let table_offset = MAXJSAMPLE as usize;

        const STEPSIZE: i32 = (MAXJSAMPLE + 1) / 16;

        let mut out: i32 = 0;

        // 1:1 up to +- MAXJSAMPLE/16
        for inp in 0..STEPSIZE {
            self.error_limiter[table_offset.wrapping_add(inp as usize)] = out;
            self.error_limiter[table_offset.wrapping_sub(inp as usize)] = -out;
            out += 1;
        }

        // 1:2 up to +- 3*MAXJSAMPLE/16
        for inp in STEPSIZE..(STEPSIZE * 3) {
            self.error_limiter[table_offset.wrapping_add(inp as usize)] = out;
            self.error_limiter[table_offset.wrapping_sub(inp as usize)] = -out;
            if inp & 1 == 0 {
                out += 1;
            }
        }

        // Clamp the rest
        for inp in (STEPSIZE * 3)..=MAXJSAMPLE {
            self.error_limiter[table_offset.wrapping_add(inp as usize)] = out;
            self.error_limiter[table_offset.wrapping_sub(inp as usize)] = -out;
        }
    }

    fn zero_histogram(&mut self) {
        self.histogram.fill(0);
    }

    #[inline(always)]
    const fn histogram_index(c0: usize, c1: usize, c2: usize) -> usize {
        (c0 * HIST_C1_ELEMS + c1) * HIST_C2_ELEMS + c2
    }

    fn prescan_quantize(&mut self, img: &RgbaImage) {
        for pixel in img.pixels() {
            let r = (pixel[0] as usize) >> C0_SHIFT;
            let g = (pixel[1] as usize) >> C1_SHIFT;
            let b = (pixel[2] as usize) >> C2_SHIFT;

            let cell = &mut self.histogram[Self::histogram_index(r, g, b)];
            if *cell < u16::MAX {
                *cell += 1;
            }
        }
    }

    fn find_biggest_color_pop(boxlist: &[ColorBox], numboxes: usize) -> Option<usize> {
        let mut maxc: i64 = 0;
        let mut which = None;

        for (i, bx) in boxlist[..numboxes].iter().enumerate() {
            if bx.colorcount > maxc && bx.volume > 0 {
                which = Some(i);
                maxc = bx.colorcount;
            }
        }

        which
    }

    fn find_biggest_volume(boxlist: &[ColorBox], numboxes: usize) -> Option<usize> {
        let mut maxv: i64 = 0;
        let mut which = None;

        for (i, bx) in boxlist[..numboxes].iter().enumerate() {
            if bx.volume > maxv {
                which = Some(i);
                maxv = bx.volume;
            }
        }

        which
    }

    fn update_box(&self, boxp: &mut ColorBox) {
        let original = *boxp;
        let mut occupied_c0min = original.c0max;
        let mut occupied_c0max = original.c0min;
        let mut occupied_c1min = original.c1max;
        let mut occupied_c1max = original.c1min;
        let mut occupied_c2min = original.c2max;
        let mut occupied_c2max = original.c2min;
        let mut colorcount = 0;

        let c2min = original.c2min as usize;
        let c2_len = (original.c2max - original.c2min + 1) as usize;

        for c0 in original.c0min..=original.c0max {
            for c1 in original.c1min..=original.c1max {
                let row_start = Self::histogram_index(c0 as usize, c1 as usize, c2min);
                let row = &self.histogram[row_start..row_start + c2_len];

                for (c2_offset, &count) in row.iter().enumerate() {
                    if count == 0 {
                        continue;
                    }

                    let c2 = original.c2min + c2_offset as i32;
                    occupied_c0min = occupied_c0min.min(c0);
                    occupied_c0max = occupied_c0max.max(c0);
                    occupied_c1min = occupied_c1min.min(c1);
                    occupied_c1max = occupied_c1max.max(c1);
                    occupied_c2min = occupied_c2min.min(c2);
                    occupied_c2max = occupied_c2max.max(c2);
                    colorcount += 1;
                }
            }
        }

        if colorcount != 0 {
            boxp.c0min = occupied_c0min;
            boxp.c0max = occupied_c0max;
            boxp.c1min = occupied_c1min;
            boxp.c1max = occupied_c1max;
            boxp.c2min = occupied_c2min;
            boxp.c2max = occupied_c2max;
        }

        let dist0 = ((boxp.c0max - boxp.c0min) << C0_SHIFT) as i64 * C0_SCALE as i64;
        let dist1 = ((boxp.c1max - boxp.c1min) << C1_SHIFT) as i64 * C1_SCALE as i64;
        let dist2 = ((boxp.c2max - boxp.c2min) << C2_SHIFT) as i64 * C2_SCALE as i64;
        boxp.volume = dist0 * dist0 + dist1 * dist1 + dist2 * dist2;
        boxp.colorcount = colorcount;
    }

    fn median_cut(
        &self,
        boxlist: &mut [ColorBox],
        mut numboxes: usize,
        desired_colors: usize,
    ) -> usize {
        while numboxes < desired_colors {
            // Select box to split
            let b1_idx = if numboxes * 2 <= desired_colors {
                Self::find_biggest_color_pop(boxlist, numboxes)
            } else {
                Self::find_biggest_volume(boxlist, numboxes)
            };

            let b1_idx = match b1_idx {
                Some(idx) => idx,
                None => break,
            };

            let b1 = boxlist[b1_idx];
            let b2_idx = numboxes;
            boxlist[b2_idx] = b1;

            let c0 = ((b1.c0max - b1.c0min) << C0_SHIFT) * C0_SCALE;
            let c1 = ((b1.c1max - b1.c1min) << C1_SHIFT) * C1_SCALE;
            let c2 = ((b1.c2max - b1.c2min) << C2_SHIFT) * C2_SCALE;

            let mut cmax = c1;
            let mut n = 1;
            if c2 > cmax {
                cmax = c2;
                n = 2;
            }
            if c0 > cmax {
                n = 0;
            }

            match n {
                0 => {
                    let lb = (b1.c0max + b1.c0min) / 2;
                    boxlist[b1_idx].c0max = lb;
                    boxlist[b2_idx].c0min = lb + 1;
                }
                1 => {
                    let lb = (b1.c1max + b1.c1min) / 2;
                    boxlist[b1_idx].c1max = lb;
                    boxlist[b2_idx].c1min = lb + 1;
                }
                2 => {
                    let lb = (b1.c2max + b1.c2min) / 2;
                    boxlist[b1_idx].c2max = lb;
                    boxlist[b2_idx].c2min = lb + 1;
                }
                _ => unreachable!(),
            }

            self.update_box(&mut boxlist[b1_idx]);
            self.update_box(&mut boxlist[b2_idx]);
            numboxes += 1;
        }

        numboxes
    }

    fn compute_color(&self, boxp: &ColorBox) -> (u8, u8, u8) {
        let mut total: i64 = 0;
        let mut c0total: i64 = 0;
        let mut c1total: i64 = 0;
        let mut c2total: i64 = 0;

        for c0 in boxp.c0min..=boxp.c0max {
            for c1 in boxp.c1min..=boxp.c1max {
                for c2 in boxp.c2min..=boxp.c2max {
                    let count = self.histogram
                        [Self::histogram_index(c0 as usize, c1 as usize, c2 as usize)]
                        as i64;
                    if count != 0 {
                        total += count;
                        c0total += ((c0 << C0_SHIFT) + (1 << (C0_SHIFT - 1))) as i64 * count;
                        c1total += ((c1 << C1_SHIFT) + (1 << (C1_SHIFT - 1))) as i64 * count;
                        c2total += ((c2 << C2_SHIFT) + (1 << (C2_SHIFT - 1))) as i64 * count;
                    }
                }
            }
        }

        if total > 0 {
            (
                ((c0total + (total >> 1)) / total) as u8,
                ((c1total + (total >> 1)) / total) as u8,
                ((c2total + (total >> 1)) / total) as u8,
            )
        } else {
            (255, 255, 255)
        }
    }

    fn select_colors(&mut self, desired_colors: usize) {
        let mut boxlist = vec![ColorBox::default(); desired_colors];

        // Initialize one box containing whole space
        boxlist[0] = ColorBox {
            c0min: 0,
            c0max: (MAXJSAMPLE >> C0_SHIFT) as i32,
            c1min: 0,
            c1max: (MAXJSAMPLE >> C1_SHIFT) as i32,
            c2min: 0,
            c2max: (MAXJSAMPLE >> C2_SHIFT) as i32,
            volume: 0,
            colorcount: 0,
        };

        self.update_box(&mut boxlist[0]);
        let numboxes = self.median_cut(&mut boxlist, 1, desired_colors);

        for i in 0..numboxes {
            let (r, g, b) = self.compute_color(&boxlist[i]);
            self.palette.set(i, r, g, b);
        }
        self.palette.colors_total = numboxes;
    }

    pub fn palette(&self) -> &Palette {
        &self.palette
    }

    pub fn palette_mut(&mut self) -> &mut Palette {
        &mut self.palette
    }

    fn find_nearby_colors(
        &self,
        minc0: i32,
        minc1: i32,
        minc2: i32,
        colorlist: &mut [u8],
    ) -> usize {
        let numcolors = self.palette.colors_total;

        let maxc0 = minc0 + ((1 << BOX_C0_SHIFT) - (1 << C0_SHIFT));
        let centerc0 = (minc0 + maxc0) >> 1;
        let maxc1 = minc1 + ((1 << BOX_C1_SHIFT) - (1 << C1_SHIFT));
        let centerc1 = (minc1 + maxc1) >> 1;
        let maxc2 = minc2 + ((1 << BOX_C2_SHIFT) - (1 << C2_SHIFT));
        let centerc2 = (minc2 + maxc2) >> 1;

        let mut mindist = vec![0i64; MAXNUMCOLORS];
        let mut minmaxdist: i64 = i64::MAX;

        for i in 0..numcolors {
            let x0 = self.palette.red[i] as i32;
            let (min_dist0, max_dist0) =
                Self::compute_dist_component(x0, minc0, maxc0, centerc0, C0_SCALE);

            let x1 = self.palette.green[i] as i32;
            let (min_dist1, max_dist1) =
                Self::compute_dist_component(x1, minc1, maxc1, centerc1, C1_SCALE);

            let x2 = self.palette.blue[i] as i32;
            let (min_dist2, max_dist2) =
                Self::compute_dist_component(x2, minc2, maxc2, centerc2, C2_SCALE);

            mindist[i] = min_dist0 + min_dist1 + min_dist2;
            let max_dist = max_dist0 + max_dist1 + max_dist2;
            if max_dist < minmaxdist {
                minmaxdist = max_dist;
            }
        }

        let mut ncolors = 0;
        for i in 0..numcolors {
            if mindist[i] <= minmaxdist {
                colorlist[ncolors] = i as u8;
                ncolors += 1;
            }
        }

        ncolors
    }

    fn compute_dist_component(
        x: i32,
        minc: i32,
        maxc: i32,
        centerc: i32,
        scale: i32,
    ) -> (i64, i64) {
        if x < minc {
            let tdist = (x - minc) as i64 * scale as i64;
            let min_dist = tdist * tdist;
            let tdist = (x - maxc) as i64 * scale as i64;
            let max_dist = tdist * tdist;
            (min_dist, max_dist)
        } else if x > maxc {
            let tdist = (x - maxc) as i64 * scale as i64;
            let min_dist = tdist * tdist;
            let tdist = (x - minc) as i64 * scale as i64;
            let max_dist = tdist * tdist;
            (min_dist, max_dist)
        } else {
            let tdist = if x <= centerc {
                (x - maxc) as i64 * scale as i64
            } else {
                (x - minc) as i64 * scale as i64
            };
            (0, tdist * tdist)
        }
    }

    fn find_best_colors(
        &self,
        minc0: i32,
        minc1: i32,
        minc2: i32,
        numcolors: usize,
        colorlist: &[u8],
        bestcolor: &mut [u8],
    ) {
        let mut bestdist = vec![i64::MAX; BOX_C0_ELEMS * BOX_C1_ELEMS * BOX_C2_ELEMS];

        const STEP_C0: i64 = ((1 << C0_SHIFT) * C0_SCALE) as i64;
        const STEP_C1: i64 = ((1 << C1_SHIFT) * C1_SCALE) as i64;
        const STEP_C2: i64 = ((1 << C2_SHIFT) * C2_SCALE) as i64;

        for i in 0..numcolors {
            let icolor = colorlist[i] as usize;
            let r = self.palette.red[icolor] as i32;
            let g = self.palette.green[icolor] as i32;
            let b = self.palette.blue[icolor] as i32;

            let mut inc0 = (minc0 - r) as i64 * C0_SCALE as i64;
            let mut dist0 = inc0 * inc0;
            let mut inc1 = (minc1 - g) as i64 * C1_SCALE as i64;
            dist0 += inc1 * inc1;
            let mut inc2 = (minc2 - b) as i64 * C2_SCALE as i64;
            dist0 += inc2 * inc2;

            inc0 = inc0 * (2 * STEP_C0) + STEP_C0 * STEP_C0;
            inc1 = inc1 * (2 * STEP_C1) + STEP_C1 * STEP_C1;
            inc2 = inc2 * (2 * STEP_C2) + STEP_C2 * STEP_C2;

            let mut bptr_idx = 0;
            let mut xx0 = inc0;

            for _ic0 in 0..BOX_C0_ELEMS {
                let mut dist1 = dist0;
                let mut xx1 = inc1;

                for _ic1 in 0..BOX_C1_ELEMS {
                    let mut dist2 = dist1;
                    let mut xx2 = inc2;

                    for _ic2 in 0..BOX_C2_ELEMS {
                        if dist2 < bestdist[bptr_idx] {
                            bestdist[bptr_idx] = dist2;
                            bestcolor[bptr_idx] = icolor as u8;
                        }
                        dist2 += xx2;
                        xx2 += 2 * STEP_C2 * STEP_C2;
                        bptr_idx += 1;
                    }
                    dist1 += xx1;
                    xx1 += 2 * STEP_C1 * STEP_C1;
                }
                dist0 += xx0;
                xx0 += 2 * STEP_C0 * STEP_C0;
            }
        }
    }

    fn fill_inverse_cmap(&mut self, c0: i32, c1: i32, c2: i32) {
        let mut colorlist = vec![0u8; MAXNUMCOLORS];
        let mut bestcolor = vec![0u8; BOX_C0_ELEMS * BOX_C1_ELEMS * BOX_C2_ELEMS];

        let bc0 = c0 >> BOX_C0_LOG as i32;
        let bc1 = c1 >> BOX_C1_LOG as i32;
        let bc2 = c2 >> BOX_C2_LOG as i32;

        let minc0 = (bc0 << BOX_C0_SHIFT) + (1 << (C0_SHIFT - 1));
        let minc1 = (bc1 << BOX_C1_SHIFT) + (1 << (C1_SHIFT - 1));
        let minc2 = (bc2 << BOX_C2_SHIFT) + (1 << (C2_SHIFT - 1));

        let numcolors = self.find_nearby_colors(minc0, minc1, minc2, &mut colorlist);
        self.find_best_colors(minc0, minc1, minc2, numcolors, &colorlist, &mut bestcolor);

        let base_c0 = (bc0 << BOX_C0_LOG as i32) as usize;
        let base_c1 = (bc1 << BOX_C1_LOG as i32) as usize;
        let base_c2 = (bc2 << BOX_C2_LOG as i32) as usize;

        let mut cptr_idx = 0;
        for ic0 in 0..BOX_C0_ELEMS {
            for ic1 in 0..BOX_C1_ELEMS {
                for ic2 in 0..BOX_C2_ELEMS {
                    let histogram_index =
                        Self::histogram_index(base_c0 + ic0, base_c1 + ic1, base_c2 + ic2);
                    self.histogram[histogram_index] = bestcolor[cptr_idx] as u16 + 1;
                    cptr_idx += 1;
                }
            }
        }
    }

    pub fn quantize_no_dither(&mut self, img: &RgbaImage) -> Vec<u8> {
        let width = img.width() as usize;
        let height = img.height() as usize;
        let mut output = vec![0u8; width * height];

        for (y, row) in img.rows().enumerate() {
            for (x, pixel) in row.enumerate() {
                let r = pixel[0] as usize;
                let g = pixel[1] as usize;
                let b = pixel[2] as usize;

                let c0 = r >> C0_SHIFT;
                let c1 = g >> C1_SHIFT;
                let c2 = b >> C2_SHIFT;

                let histogram_index = Self::histogram_index(c0, c1, c2);
                let mut cached = self.histogram[histogram_index];
                if cached == 0 {
                    self.fill_inverse_cmap(c0 as i32, c1 as i32, c2 as i32);
                    cached = self.histogram[histogram_index];
                }

                output[y * width + x] = (cached - 1) as u8;
            }
        }

        output
    }

    pub fn quantize_fs_dither(&mut self, img: &RgbaImage) -> Vec<u8> {
        let width = img.width() as usize;
        let height = img.height() as usize;
        let mut output = vec![0u8; width * height];

        self.fserrors = vec![0i16; (width + 2) * 3];
        self.on_odd_row = false;

        let table_offset = MAXJSAMPLE as usize;

        for row in 0..height {
            let (dir, start_col, end_col, errorptr_start) = if self.on_odd_row {
                (-1i32, width as i32 - 1, -1i32, (width + 1) * 3)
            } else {
                (1i32, 0i32, width as i32, 0usize)
            };

            let mut cur0: i32 = 0;
            let mut cur1: i32 = 0;
            let mut cur2: i32 = 0;
            let mut belowerr0: i32 = 0;
            let mut belowerr1: i32 = 0;
            let mut belowerr2: i32 = 0;
            let mut bpreverr0: i32 = 0;
            let mut bpreverr1: i32 = 0;
            let mut bpreverr2: i32 = 0;

            let mut col = start_col;
            let mut errorptr = errorptr_start as i32;
            let dir3 = dir * 3;

            while col != end_col {
                let x = col as usize;
                let pixel = img.get_pixel(x as u32, row as u32);

                // Add error from previous and below
                let ep_idx = (errorptr + dir3) as usize;
                cur0 = (cur0 + self.fserrors[ep_idx] as i32 + 8) >> 4;
                cur1 = (cur1 + self.fserrors[ep_idx + 1] as i32 + 8) >> 4;
                cur2 = (cur2 + self.fserrors[ep_idx + 2] as i32 + 8) >> 4;

                cur0 = self.error_limiter[table_offset.wrapping_add(cur0 as usize)];
                cur1 = self.error_limiter[table_offset.wrapping_add(cur1 as usize)];
                cur2 = self.error_limiter[table_offset.wrapping_add(cur2 as usize)];

                cur0 += pixel[0] as i32;
                cur1 += pixel[1] as i32;
                cur2 += pixel[2] as i32;
                cur0 = cur0.clamp(0, 255);
                cur1 = cur1.clamp(0, 255);
                cur2 = cur2.clamp(0, 255);

                let c0 = (cur0 as usize) >> C0_SHIFT;
                let c1 = (cur1 as usize) >> C1_SHIFT;
                let c2 = (cur2 as usize) >> C2_SHIFT;

                let histogram_index = Self::histogram_index(c0, c1, c2);
                let mut cached = self.histogram[histogram_index];
                if cached == 0 {
                    self.fill_inverse_cmap(c0 as i32, c1 as i32, c2 as i32);
                    cached = self.histogram[histogram_index];
                }

                let pixcode = (cached - 1) as usize;
                output[row * width + x] = pixcode as u8;

                cur0 -= self.palette.red[pixcode] as i32;
                cur1 -= self.palette.green[pixcode] as i32;
                cur2 -= self.palette.blue[pixcode] as i32;

                let mut bnexterr = cur0;
                let mut delta = cur0 * 2;
                cur0 += delta; // 3x
                self.fserrors[errorptr as usize] = (bpreverr0 + cur0) as i16;
                cur0 += delta; // 5x
                bpreverr0 = belowerr0 + cur0;
                belowerr0 = bnexterr;
                cur0 += delta; // 7x

                bnexterr = cur1;
                delta = cur1 * 2;
                cur1 += delta;
                self.fserrors[errorptr as usize + 1] = (bpreverr1 + cur1) as i16;
                cur1 += delta;
                bpreverr1 = belowerr1 + cur1;
                belowerr1 = bnexterr;
                cur1 += delta;

                bnexterr = cur2;
                delta = cur2 * 2;
                cur2 += delta;
                self.fserrors[errorptr as usize + 2] = (bpreverr2 + cur2) as i16;
                cur2 += delta;
                bpreverr2 = belowerr2 + cur2;
                belowerr2 = bnexterr;
                cur2 += delta;

                col += dir;
                errorptr += dir3;
            }

            self.fserrors[errorptr as usize + 1] = bpreverr1 as i16;
            self.fserrors[errorptr as usize + 2] = bpreverr2 as i16;

            self.on_odd_row = !self.on_odd_row;
        }

        output
    }

    pub fn quantize(&mut self, img: &RgbaImage, dither: bool) -> Vec<u8> {
        if dither { self.quantize_fs_dither(img) } else { self.quantize_no_dither(img) }
    }

    pub fn sync_palette_from(&mut self, other: &Palette) {
        self.palette = other.clone();
    }
}

pub fn sync_palette(from: &Palette, to: &mut Palette) {
    *to = from.clone();
}
