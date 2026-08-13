use image::{Rgba, RgbaImage};

#[derive(Debug, Clone, Copy, Default)]
pub struct Pixer {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Pixer {
    #[inline(always)]
    pub fn new() -> Self {
        Pixer::default()
    }

    #[inline(always)]
    pub fn from_rgba(pixel: &Rgba<u8>) -> Self {
        Pixer { r: pixel[0] as f32, g: pixel[1] as f32, b: pixel[2] as f32, a: pixel[3] as f32 }
    }

    #[inline(always)]
    pub fn to_rgba(&self) -> Rgba<u8> {
        Rgba([clamp_u8(self.r), clamp_u8(self.g), clamp_u8(self.b), clamp_u8(self.a)])
    }

    #[inline(always)]
    pub fn preblend(&mut self) {
        self.r *= self.a;
        self.g *= self.a;
        self.b *= self.a;
    }

    #[inline(always)]
    pub fn postblend(&mut self, scale: f32) {
        if scale > 0.0001 {
            self.r /= scale;
            self.g /= scale;
            self.b /= scale;
        } else {
            self.r = 0.0;
            self.g = 0.0;
            self.b = 0.0;
        }
    }

    pub fn add(&mut self, other: &Pixer) {
        self.r += other.r;
        self.g += other.g;
        self.b += other.b;
        self.a += other.a;
    }

    pub fn add_rgba(&mut self, pixel: &Rgba<u8>) {
        self.r += pixel[0] as f32;
        self.g += pixel[1] as f32;
        self.b += pixel[2] as f32;
        self.a += pixel[3] as f32;
    }

    pub fn scale(&mut self, factor: f32) {
        self.r *= factor;
        self.g *= factor;
        self.b *= factor;
        self.a *= factor;
    }

    pub fn div(&mut self, factor: f32) {
        if factor.abs() > 0.0001 {
            self.r /= factor;
            self.g /= factor;
            self.b /= factor;
            self.a /= factor;
        }
    }
}

impl std::ops::Add for Pixer {
    type Output = Pixer;

    fn add(self, other: Pixer) -> Pixer {
        Pixer { r: self.r + other.r, g: self.g + other.g, b: self.b + other.b, a: self.a + other.a }
    }
}

impl std::ops::AddAssign for Pixer {
    fn add_assign(&mut self, other: Pixer) {
        self.r += other.r;
        self.g += other.g;
        self.b += other.b;
        self.a += other.a;
    }
}

impl std::ops::Mul<f32> for Pixer {
    type Output = Pixer;

    fn mul(self, factor: f32) -> Pixer {
        Pixer { r: self.r * factor, g: self.g * factor, b: self.b * factor, a: self.a * factor }
    }
}

impl std::ops::Div<f32> for Pixer {
    type Output = Pixer;

    fn div(self, factor: f32) -> Pixer {
        if factor.abs() > 0.0001 {
            Pixer { r: self.r / factor, g: self.g / factor, b: self.b / factor, a: self.a / factor }
        } else {
            Pixer::new()
        }
    }
}

#[inline]
fn clamp_u8(v: f32) -> u8 {
    if v <= 0.0 {
        0
    } else if v >= 255.0 {
        255
    } else {
        v as u8
    }
}

#[inline(always)]
pub fn safe_pixel(img: &RgbaImage, x: i32, y: i32) -> Rgba<u8> {
    let w = img.width() as i32;
    let h = img.height() as i32;

    if x < 0 || y < 0 || x >= w || y >= h {
        Rgba([0, 0, 0, 0])
    } else {
        *img.get_pixel(x as u32, y as u32)
    }
}

/// Bilinear interpolation with alpha-weighted averaging.
///
/// Coordinates remain `f64` for sub-pixel accuracy (the heavy lifting is the
/// per-pixel color blend, which is performed in `f32` for throughput). The
/// `f64` geometry inputs are down-cast to `f32` when constructing the returned
/// `Pixer`.
#[inline(always)]
pub fn sample_linear(img: &RgbaImage, x: f64, y: f64) -> Pixer {
    let xx = x.floor() as i32;
    let yy = y.floor() as i32;
    let fx = x - xx as f64;
    let fy = y - yy as f64;

    let w00 = ((1.0 - fx) * (1.0 - fy)) as f32;
    let w10 = (fx * (1.0 - fy)) as f32;
    let w01 = ((1.0 - fx) * fy) as f32;
    let w11 = (fx * fy) as f32;

    let w = img.width() as i32;
    let h = img.height() as i32;

    if xx >= 0 && yy >= 0 && xx + 1 < w && yy + 1 < h {
        let raw = img.as_raw();
        let w_us = w as usize;
        let row_stride = w_us * 4;

        let row0 = yy as usize * row_stride;
        let row1 = row0 + row_stride;
        let col = xx as usize * 4;

        let i00 = row0 + col;
        let i10 = i00 + 4;
        let i01 = row1 + col;
        let i11 = i01 + 4;

        let a00 = raw[i00 + 3] as f32;
        let a10 = raw[i10 + 3] as f32;
        let a01 = raw[i01 + 3] as f32;
        let a11 = raw[i11 + 3] as f32;

        let aa = w00 * a00 + w10 * a10 + w01 * a01 + w11 * a11;
        let aa_safe = if aa < 0.0001 { 0.0001 } else { aa };

        return Pixer {
            r: (w00 * raw[i00] as f32 * a00
                + w10 * raw[i10] as f32 * a10
                + w01 * raw[i01] as f32 * a01
                + w11 * raw[i11] as f32 * a11)
                / aa_safe,
            g: (w00 * raw[i00 + 1] as f32 * a00
                + w10 * raw[i10 + 1] as f32 * a10
                + w01 * raw[i01 + 1] as f32 * a01
                + w11 * raw[i11 + 1] as f32 * a11)
                / aa_safe,
            b: (w00 * raw[i00 + 2] as f32 * a00
                + w10 * raw[i10 + 2] as f32 * a10
                + w01 * raw[i01 + 2] as f32 * a01
                + w11 * raw[i11 + 2] as f32 * a11)
                / aa_safe,
            a: aa,
        };
    }

    let p00 = safe_pixel(img, xx, yy);
    let p10 = safe_pixel(img, xx + 1, yy);
    let p01 = safe_pixel(img, xx, yy + 1);
    let p11 = safe_pixel(img, xx + 1, yy + 1);

    let a00 = p00[3] as f32;
    let a10 = p10[3] as f32;
    let a01 = p01[3] as f32;
    let a11 = p11[3] as f32;

    let aa = w00 * a00 + w10 * a10 + w01 * a01 + w11 * a11;
    let aa_safe = if aa < 0.0001 { 0.0001 } else { aa };

    Pixer {
        r: (w00 * p00[0] as f32 * a00
            + w10 * p10[0] as f32 * a10
            + w01 * p01[0] as f32 * a01
            + w11 * p11[0] as f32 * a11)
            / aa_safe,
        g: (w00 * p00[1] as f32 * a00
            + w10 * p10[1] as f32 * a10
            + w01 * p01[1] as f32 * a01
            + w11 * p11[1] as f32 * a11)
            / aa_safe,
        b: (w00 * p00[2] as f32 * a00
            + w10 * p10[2] as f32 * a10
            + w01 * p01[2] as f32 * a01
            + w11 * p11[2] as f32 * a11)
            / aa_safe,
        a: aa,
    }
}

/// Bilinear interpolation in premultiplied-alpha space.
///
/// The sampled compositor combines several neighboring samples before
/// converting back to straight alpha. Keeping RGB premultiplied here avoids a
/// divide followed immediately by a multiply for every tap.
#[inline(always)]
pub(crate) fn sample_linear_premultiplied(img: &RgbaImage, x: f64, y: f64) -> Pixer {
    let xx = x.floor() as i32;
    let yy = y.floor() as i32;
    let fx = x - xx as f64;
    let fy = y - yy as f64;

    let w00 = ((1.0 - fx) * (1.0 - fy)) as f32;
    let w10 = (fx * (1.0 - fy)) as f32;
    let w01 = ((1.0 - fx) * fy) as f32;
    let w11 = (fx * fy) as f32;

    let w = img.width() as i32;
    let h = img.height() as i32;

    if xx >= 0 && yy >= 0 && xx + 1 < w && yy + 1 < h {
        let raw = img.as_raw();
        let row_stride = w as usize * 4;

        let i00 = yy as usize * row_stride + xx as usize * 4;
        let i10 = i00 + 4;
        let i01 = i00 + row_stride;
        let i11 = i01 + 4;

        let a00 = raw[i00 + 3] as f32;
        let a10 = raw[i10 + 3] as f32;
        let a01 = raw[i01 + 3] as f32;
        let a11 = raw[i11 + 3] as f32;

        return Pixer {
            r: w00 * raw[i00] as f32 * a00
                + w10 * raw[i10] as f32 * a10
                + w01 * raw[i01] as f32 * a01
                + w11 * raw[i11] as f32 * a11,
            g: w00 * raw[i00 + 1] as f32 * a00
                + w10 * raw[i10 + 1] as f32 * a10
                + w01 * raw[i01 + 1] as f32 * a01
                + w11 * raw[i11 + 1] as f32 * a11,
            b: w00 * raw[i00 + 2] as f32 * a00
                + w10 * raw[i10 + 2] as f32 * a10
                + w01 * raw[i01 + 2] as f32 * a01
                + w11 * raw[i11 + 2] as f32 * a11,
            a: w00 * a00 + w10 * a10 + w01 * a01 + w11 * a11,
        };
    }

    let p00 = safe_pixel(img, xx, yy);
    let p10 = safe_pixel(img, xx + 1, yy);
    let p01 = safe_pixel(img, xx, yy + 1);
    let p11 = safe_pixel(img, xx + 1, yy + 1);

    let a00 = p00[3] as f32;
    let a10 = p10[3] as f32;
    let a01 = p01[3] as f32;
    let a11 = p11[3] as f32;

    Pixer {
        r: w00 * p00[0] as f32 * a00
            + w10 * p10[0] as f32 * a10
            + w01 * p01[0] as f32 * a01
            + w11 * p11[0] as f32 * a11,
        g: w00 * p00[1] as f32 * a00
            + w10 * p10[1] as f32 * a10
            + w01 * p01[1] as f32 * a01
            + w11 * p11[1] as f32 * a11,
        b: w00 * p00[2] as f32 * a00
            + w10 * p10[2] as f32 * a10
            + w01 * p01[2] as f32 * a01
            + w11 * p11[2] as f32 * a11,
        a: w00 * a00 + w10 * a10 + w01 * a01 + w11 * a11,
    }
}

/// Bilinear interpolation for an image known to contain only opaque pixels.
///
/// This preserves the general sampler's floating-point operation order while
/// replacing four alpha loads with the known value `255`.
#[inline(always)]
pub(crate) fn sample_linear_opaque(img: &RgbaImage, x: f64, y: f64) -> Pixer {
    let xx = x.floor() as i32;
    let yy = y.floor() as i32;
    let fx = x - xx as f64;
    let fy = y - yy as f64;

    let w00 = ((1.0 - fx) * (1.0 - fy)) as f32;
    let w10 = (fx * (1.0 - fy)) as f32;
    let w01 = ((1.0 - fx) * fy) as f32;
    let w11 = (fx * fy) as f32;

    let w = img.width() as i32;
    let h = img.height() as i32;

    if xx >= 0 && yy >= 0 && xx + 1 < w && yy + 1 < h {
        let raw = img.as_raw();
        let row_stride = w as usize * 4;

        let i00 = yy as usize * row_stride + xx as usize * 4;
        let i10 = i00 + 4;
        let i01 = i00 + row_stride;
        let i11 = i01 + 4;

        let alpha = 255.0;

        return Pixer {
            r: w00 * raw[i00] as f32 * alpha
                + w10 * raw[i10] as f32 * alpha
                + w01 * raw[i01] as f32 * alpha
                + w11 * raw[i11] as f32 * alpha,
            g: w00 * raw[i00 + 1] as f32 * alpha
                + w10 * raw[i10 + 1] as f32 * alpha
                + w01 * raw[i01 + 1] as f32 * alpha
                + w11 * raw[i11 + 1] as f32 * alpha,
            b: w00 * raw[i00 + 2] as f32 * alpha
                + w10 * raw[i10 + 2] as f32 * alpha
                + w01 * raw[i01 + 2] as f32 * alpha
                + w11 * raw[i11 + 2] as f32 * alpha,
            a: w00 * alpha + w10 * alpha + w01 * alpha + w11 * alpha,
        };
    }

    let p00 = safe_pixel(img, xx, yy);
    let p10 = safe_pixel(img, xx + 1, yy);
    let p01 = safe_pixel(img, xx, yy + 1);
    let p11 = safe_pixel(img, xx + 1, yy + 1);
    let a00 = p00[3] as f32;
    let a10 = p10[3] as f32;
    let a01 = p01[3] as f32;
    let a11 = p11[3] as f32;

    Pixer {
        r: w00 * p00[0] as f32 * a00
            + w10 * p10[0] as f32 * a10
            + w01 * p01[0] as f32 * a01
            + w11 * p11[0] as f32 * a11,
        g: w00 * p00[1] as f32 * a00
            + w10 * p10[1] as f32 * a10
            + w01 * p01[1] as f32 * a01
            + w11 * p11[1] as f32 * a11,
        b: w00 * p00[2] as f32 * a00
            + w10 * p10[2] as f32 * a10
            + w01 * p01[2] as f32 * a01
            + w11 * p11[2] as f32 * a11,
        a: w00 * a00 + w10 * a10 + w01 * a01 + w11 * a11,
    }
}

pub fn sample_weakly(img: &RgbaImage, x: f64, y: f64) -> Pixer {
    Pixer::from_rgba(&safe_pixel(img, x as i32, y as i32))
}

#[inline(always)]
pub fn distance(x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    ((x1 - x2).powi(2) + (y1 - y2).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixer_operations() {
        let mut p = Pixer { r: 100.0, g: 150.0, b: 200.0, a: 255.0 };
        p.scale(0.5);
        assert_eq!(p.r, 50.0);
        assert_eq!(p.g, 75.0);
        assert_eq!(p.b, 100.0);
    }

    #[test]
    fn test_clamp_u8() {
        assert_eq!(clamp_u8(-10.0), 0);
        assert_eq!(clamp_u8(128.0), 128);
        assert_eq!(clamp_u8(300.0), 255);
    }

    #[test]
    fn opaque_sampler_matches_general_premultiplied_sampler() {
        let mut image = RgbaImage::new(3, 2);
        for (i, pixel) in image.pixels_mut().enumerate() {
            let value = u8::try_from(i * 31).expect("test value fits in u8");
            *pixel = Rgba([value, value.wrapping_add(17), value.wrapping_add(83), 255]);
        }

        for (x, y) in [
            (-1.25, -0.5),
            (-0.25, 0.25),
            (0.0, 0.0),
            (0.3, 0.7),
            (1.5, 0.25),
            (2.0, 1.0),
            (2.75, 1.75),
            (4.0, 4.0),
        ] {
            let general = sample_linear_premultiplied(&image, x, y);
            let opaque = sample_linear_opaque(&image, x, y);
            assert_eq!(general.r.to_bits(), opaque.r.to_bits());
            assert_eq!(general.g.to_bits(), opaque.g.to_bits());
            assert_eq!(general.b.to_bits(), opaque.b.to_bits());
            assert_eq!(general.a.to_bits(), opaque.a.to_bits());
        }
    }
}
