use image::{Rgba, RgbaImage};

#[derive(Debug, Clone, Copy, Default)]
pub struct Pixer {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Pixer {
    #[inline(always)]
    pub fn new() -> Self {
        Pixer::default()
    }

    #[inline(always)]
    pub fn from_rgba(pixel: &Rgba<u8>) -> Self {
        Pixer { r: pixel[0] as f64, g: pixel[1] as f64, b: pixel[2] as f64, a: pixel[3] as f64 }
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
    pub fn postblend(&mut self, scale: f64) {
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
        self.r += pixel[0] as f64;
        self.g += pixel[1] as f64;
        self.b += pixel[2] as f64;
        self.a += pixel[3] as f64;
    }

    pub fn scale(&mut self, factor: f64) {
        self.r *= factor;
        self.g *= factor;
        self.b *= factor;
        self.a *= factor;
    }

    pub fn div(&mut self, factor: f64) {
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

impl std::ops::Mul<f64> for Pixer {
    type Output = Pixer;

    fn mul(self, factor: f64) -> Pixer {
        Pixer { r: self.r * factor, g: self.g * factor, b: self.b * factor, a: self.a * factor }
    }
}

impl std::ops::Div<f64> for Pixer {
    type Output = Pixer;

    fn div(self, factor: f64) -> Pixer {
        if factor.abs() > 0.0001 {
            Pixer { r: self.r / factor, g: self.g / factor, b: self.b / factor, a: self.a / factor }
        } else {
            Pixer::new()
        }
    }
}

#[inline]
fn clamp_u8(v: f64) -> u8 {
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

/// Bilinear interpolation with alpha-weighted averaging
#[inline(always)]
pub fn sample_linear(img: &RgbaImage, x: f64, y: f64) -> Pixer {
    let xx = x.floor() as i32;
    let yy = y.floor() as i32;
    let fx = x - xx as f64;
    let fy = y - yy as f64;

    let p00 = safe_pixel(img, xx, yy);
    let p10 = safe_pixel(img, xx + 1, yy);
    let p01 = safe_pixel(img, xx, yy + 1);
    let p11 = safe_pixel(img, xx + 1, yy + 1);

    let w00 = (1.0 - fx) * (1.0 - fy);
    let w10 = fx * (1.0 - fy);
    let w01 = (1.0 - fx) * fy;
    let w11 = fx * fy;

    let a00 = p00[3] as f64;
    let a10 = p10[3] as f64;
    let a01 = p01[3] as f64;
    let a11 = p11[3] as f64;

    let aa = w00 * a00 + w10 * a10 + w01 * a01 + w11 * a11;
    let aa_safe = if aa < 0.0001 { 0.0001 } else { aa };

    Pixer {
        r: (w00 * p00[0] as f64 * a00
            + w10 * p10[0] as f64 * a10
            + w01 * p01[0] as f64 * a01
            + w11 * p11[0] as f64 * a11)
            / aa_safe,
        g: (w00 * p00[1] as f64 * a00
            + w10 * p10[1] as f64 * a10
            + w01 * p01[1] as f64 * a01
            + w11 * p11[1] as f64 * a11)
            / aa_safe,
        b: (w00 * p00[2] as f64 * a00
            + w10 * p10[2] as f64 * a10
            + w01 * p01[2] as f64 * a01
            + w11 * p11[2] as f64 * a11)
            / aa_safe,
        a: aa,
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
}
