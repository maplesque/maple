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

/// Number of histogram cells one `fill_inverse_cmap` call resolves.
const BOX_ELEMS: usize = BOX_C0_ELEMS * BOX_C1_ELEMS * BOX_C2_ELEMS;

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

/// Bitmap of the occupied histogram cells, one `u32` per `(c0, c1)` row.
///
/// `HIST_C2_ELEMS` is exactly 32, so a whole row of the histogram fits in a
/// single word and the 128 KB histogram condenses to 8 KB that stays in L1 for
/// the entire median cut. The histogram is immutable while boxes are being
/// split, so the bitmap is built once and stays valid for every split.
struct Occupancy {
    rows: Box<[u32; HIST_C0_ELEMS * HIST_C1_ELEMS]>,
}

const _: () = assert!(HIST_C2_ELEMS == u32::BITS as usize);
const _: () = assert!(HIST_C0_ELEMS <= u32::BITS as usize);
const _: () = assert!(HIST_C1_ELEMS <= u64::BITS as usize);

impl Occupancy {
    fn from_histogram(histogram: &[u16; HIST_ELEMS]) -> Self {
        let mut rows = Box::new([0u32; HIST_C0_ELEMS * HIST_C1_ELEMS]);

        for (bits, cells) in rows.iter_mut().zip(histogram.chunks_exact(HIST_C2_ELEMS)) {
            let mut row = 0u32;
            for (c2, &count) in cells.iter().enumerate() {
                row |= ((count != 0) as u32) << c2;
            }
            *bits = row;
        }

        Occupancy { rows }
    }

    #[inline(always)]
    fn row(&self, c0: i32, c1: i32) -> u32 {
        self.rows[c0 as usize * HIST_C1_ELEMS + c1 as usize]
    }

    /// Mask of the `c2` bits a box spans.
    #[inline(always)]
    fn c2_mask(c2min: i32, c2max: i32) -> u32 {
        let width = (c2max - c2min + 1) as u32;
        if width >= u32::BITS { u32::MAX } else { ((1u32 << width) - 1) << c2min }
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

    #[inline(always)]
    fn histogram_index_of(pixel: &[u8; 4]) -> usize {
        Self::histogram_index(
            (pixel[0] as usize) >> C0_SHIFT,
            (pixel[1] as usize) >> C1_SHIFT,
            (pixel[2] as usize) >> C2_SHIFT,
        )
    }

    fn prescan_quantize(&mut self, img: &RgbaImage) {
        // The arithmetic per pixel is trivial; what costs is the scatter into
        // the histogram. Neighbouring pixels of a photo usually land in the
        // same cell, so a single table turns the scan into one long chain of
        // store-to-load forwarded increments. Scattering even and odd pixels
        // into two tables halves that chain; the tables are folded back
        // together afterwards in one linear pass.
        //
        // Two lanes is the sweet spot: four makes the working set larger than
        // the level of cache that keeps up, and loses more than the shorter
        // chain wins.
        let (pairs, tail) = img.as_raw().as_chunks::<8>();
        let mut odd_counts = vec![0u16; HIST_ELEMS];

        for pair in pairs {
            let (pixels, _) = pair.as_chunks::<4>();
            let even = Self::histogram_index_of(&pixels[0]);
            let odd = Self::histogram_index_of(&pixels[1]);

            let cell = &mut self.histogram[even];
            if *cell < u16::MAX {
                *cell += 1;
            }

            let cell = &mut odd_counts[odd];
            if *cell < u16::MAX {
                *cell += 1;
            }
        }

        for pixel in tail.as_chunks::<4>().0 {
            let cell = &mut self.histogram[Self::histogram_index_of(pixel)];
            if *cell < u16::MAX {
                *cell += 1;
            }
        }

        // Folding the two halves back together is a linear, vectorizable pass.
        for (cell, odd) in self.histogram.iter_mut().zip(odd_counts.iter()) {
            *cell = cell.saturating_add(*odd);
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

    fn update_box(&self, occupancy: &Occupancy, boxp: &mut ColorBox) {
        let original = *boxp;
        let mask = Occupancy::c2_mask(original.c2min, original.c2max);

        // The scan is entirely branch-free: every row of the box is one masked
        // load, a popcount and two ORs. Rather than widening six bounds as it
        // goes, it collects which c0/c1/c2 coordinates are occupied as bitmaps
        // and reads the bounds off them once at the end.
        let mut colorcount: i64 = 0;
        let mut c0_used: u32 = 0;
        let mut c1_used: u64 = 0;
        let mut c2_used: u32 = 0;

        for c0 in original.c0min..=original.c0max {
            let mut plane: u32 = 0;

            for c1 in original.c1min..=original.c1max {
                let row = occupancy.row(c0, c1) & mask;
                colorcount += row.count_ones() as i64;
                plane |= row;
                c1_used |= ((row != 0) as u64) << c1;
            }

            c2_used |= plane;
            c0_used |= ((plane != 0) as u32) << c0;
        }

        if colorcount != 0 {
            boxp.c0min = c0_used.trailing_zeros() as i32;
            boxp.c0max = (u32::BITS - 1 - c0_used.leading_zeros()) as i32;
            boxp.c1min = c1_used.trailing_zeros() as i32;
            boxp.c1max = (u64::BITS - 1 - c1_used.leading_zeros()) as i32;
            boxp.c2min = c2_used.trailing_zeros() as i32;
            boxp.c2max = (u32::BITS - 1 - c2_used.leading_zeros()) as i32;
        }

        let dist0 = ((boxp.c0max - boxp.c0min) << C0_SHIFT) as i64 * C0_SCALE as i64;
        let dist1 = ((boxp.c1max - boxp.c1min) << C1_SHIFT) as i64 * C1_SCALE as i64;
        let dist2 = ((boxp.c2max - boxp.c2min) << C2_SHIFT) as i64 * C2_SCALE as i64;
        boxp.volume = dist0 * dist0 + dist1 * dist1 + dist2 * dist2;
        boxp.colorcount = colorcount;
    }

    fn median_cut(
        &self,
        occupancy: &Occupancy,
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

            self.update_box(occupancy, &mut boxlist[b1_idx]);
            self.update_box(occupancy, &mut boxlist[b2_idx]);
            numboxes += 1;
        }

        numboxes
    }

    fn compute_color(&self, occupancy: &Occupancy, boxp: &ColorBox) -> (u8, u8, u8) {
        let mut total: i64 = 0;
        let mut c0total: i64 = 0;
        let mut c1total: i64 = 0;
        let mut c2total: i64 = 0;

        let mask = Occupancy::c2_mask(boxp.c2min, boxp.c2max);

        for c0 in boxp.c0min..=boxp.c0max {
            for c1 in boxp.c1min..=boxp.c1max {
                // Walking the set bits visits only the occupied cells, so the
                // empty ones never reach the histogram at all.
                let mut row = occupancy.row(c0, c1) & mask;
                while row != 0 {
                    let c2 = row.trailing_zeros() as i32;
                    row &= row - 1;

                    let count = self.histogram
                        [Self::histogram_index(c0 as usize, c1 as usize, c2 as usize)]
                        as i64;
                    total += count;
                    c0total += ((c0 << C0_SHIFT) + (1 << (C0_SHIFT - 1))) as i64 * count;
                    c1total += ((c1 << C1_SHIFT) + (1 << (C1_SHIFT - 1))) as i64 * count;
                    c2total += ((c2 << C2_SHIFT) + (1 << (C2_SHIFT - 1))) as i64 * count;
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

        let occupancy = Occupancy::from_histogram(&self.histogram);

        self.update_box(&occupancy, &mut boxlist[0]);
        let numboxes = self.median_cut(&occupancy, &mut boxlist, 1, desired_colors);

        for i in 0..numboxes {
            let (r, g, b) = self.compute_color(&occupancy, &boxlist[i]);
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
        colorlist: &mut [u8; MAXNUMCOLORS],
    ) -> usize {
        let numcolors = self.palette.colors_total;

        let maxc0 = minc0 + ((1 << BOX_C0_SHIFT) - (1 << C0_SHIFT));
        let centerc0 = (minc0 + maxc0) >> 1;
        let maxc1 = minc1 + ((1 << BOX_C1_SHIFT) - (1 << C1_SHIFT));
        let centerc1 = (minc1 + maxc1) >> 1;
        let maxc2 = minc2 + ((1 << BOX_C2_SHIFT) - (1 << C2_SHIFT));
        let centerc2 = (minc2 + maxc2) >> 1;

        // A weighted squared distance never exceeds (255 * 3)^2 * 3, so the
        // whole computation fits in an i32 - half the traffic of the i64 it
        // used to run in, and a min-reduce the compiler can vectorize.
        let mut mindist = [0i32; MAXNUMCOLORS];
        let mut minmaxdist: i32 = i32::MAX;

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
    ) -> (i32, i32) {
        if x < minc {
            let tdist = (x - minc) * scale;
            let min_dist = tdist * tdist;
            let tdist = (x - maxc) * scale;
            let max_dist = tdist * tdist;
            (min_dist, max_dist)
        } else if x > maxc {
            let tdist = (x - maxc) * scale;
            let min_dist = tdist * tdist;
            let tdist = (x - minc) * scale;
            let max_dist = tdist * tdist;
            (min_dist, max_dist)
        } else {
            let tdist = if x <= centerc { (x - maxc) * scale } else { (x - minc) * scale };
            (0, tdist * tdist)
        }
    }

    fn find_best_colors(
        &self,
        minc0: i32,
        minc1: i32,
        minc2: i32,
        numcolors: usize,
        colorlist: &[u8; MAXNUMCOLORS],
        bestcolor: &mut [u8; BOX_ELEMS],
    ) {
        // Same i32 range argument as `find_nearby_colors`. Keeping the winning
        // color as an i32 next to the distance lets the compare-and-select run
        // on two same-width lanes instead of mixing an i64 compare with a byte
        // store; it is narrowed back to u8 once, at the end.
        let mut bestdist = [i32::MAX; BOX_ELEMS];
        let mut bestindex = [0i32; BOX_ELEMS];

        const STEP_C0: i32 = ((1 << C0_SHIFT) * C0_SCALE) as i32;
        const STEP_C1: i32 = ((1 << C1_SHIFT) * C1_SCALE) as i32;
        const STEP_C2: i32 = ((1 << C2_SHIFT) * C2_SCALE) as i32;

        for i in 0..numcolors {
            let icolor = colorlist[i] as i32;
            let r = self.palette.red[icolor as usize] as i32;
            let g = self.palette.green[icolor as usize] as i32;
            let b = self.palette.blue[icolor as usize] as i32;

            let mut inc0 = (minc0 - r) * C0_SCALE;
            let mut dist0 = inc0 * inc0;
            let mut inc1 = (minc1 - g) * C1_SCALE;
            dist0 += inc1 * inc1;
            let mut inc2 = (minc2 - b) * C2_SCALE;
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
                        let closer = dist2 < bestdist[bptr_idx];
                        if closer {
                            bestdist[bptr_idx] = dist2;
                            bestindex[bptr_idx] = icolor;
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

        for (out, &best) in bestcolor.iter_mut().zip(bestindex.iter()) {
            *out = best as u8;
        }
    }

    fn fill_inverse_cmap(&mut self, c0: i32, c1: i32, c2: i32) {
        // These are small, fixed-size and dead by the end of the call, so they
        // live on the stack. Heap-allocating and zeroing them on every call
        // was pure overhead on the cold-cache path.
        let mut colorlist = [0u8; MAXNUMCOLORS];
        let mut bestcolor = [0u8; BOX_ELEMS];

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
