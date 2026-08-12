//! Benchmarks for animated WebP output, the `--webp` and `--webp_single` paths.
//!
//! WebP keeps full 24-bit color with alpha, so unlike GIF it skips color
//! quantization entirely: the cost is compositing plus libwebp. `webp_anim` is
//! native-only and therefore not re-exported by `maple`, so these benchmarks
//! reach it through `maple_render_core` directly.

mod common;

use divan::Bencher;
use maple::render::RenderQuality;
use maple_render_core::webp_anim::{WebpAnim, WebpOptions};

fn main() {
    common::init();
    divan::main();
}

/// Default lossy settings: quality 95, method 4.
fn lossy() -> WebpOptions {
    WebpOptions::default()
}

fn lossless() -> WebpOptions {
    WebpOptions { lossless: true, ..WebpOptions::default() }
}

fn anim(template: &str, image: &str, options: WebpOptions) -> WebpAnim {
    let mut anim = WebpAnim::new(common::renders(template, image, RenderQuality::Sampled));
    let repo = common::repository(template);
    anim.set_timing(repo.get_period(), repo.get_hold());
    anim.set_options(options);
    anim
}

/// The same animation with every frame composited up front, so that only the
/// libwebp encoding is measured.
fn warm_anim(template: &str, image: &str, options: WebpOptions) -> WebpAnim {
    let mut anim = WebpAnim::new(common::warm_renders(template, image, RenderQuality::Sampled));
    let repo = common::repository(template);
    anim.set_timing(repo.get_period(), repo.get_hold());
    anim.set_options(options);
    anim
}

/// Full animated WebP: every frame is composited and handed to libwebp. This is
/// what `maple --zip ... --webp out.webp` runs.
#[divan::bench(sample_count = 3, sample_size = 1)]
fn webp_animation(bencher: Bencher) {
    bencher
        .with_inputs(|| anim("book", "frog.jpg", lossy()))
        .bench_local_refs(|anim| anim.encode().expect("webp encode"));
}

/// WebP serialization on its own: frame prediction and entropy coding of
/// already composited frames, the counterpart of `gif_serialize`.
#[divan::bench(sample_count = 3, sample_size = 1)]
fn webp_serialize(bencher: Bencher) {
    bencher
        .with_inputs(|| warm_anim("book", "frog.jpg", lossy()))
        .bench_local_refs(|anim| anim.encode().expect("webp encode"));
}

/// A single still frame through libwebp, the `--webp_single` path.
#[divan::bench]
fn webp_single_frame(bencher: Bencher) {
    let composited = common::composited_frame("toaster", 11, "frog.jpg");
    let frame = composited.get();
    let options = lossy();

    bencher.bench_local(|| WebpAnim::encode_single(frame, &options).expect("webp encode"));
}

/// The same still frame encoded losslessly, the `--webp_lossless` path. A whole
/// lossless animation is left out on purpose: it takes minutes under the CPU
/// simulator, and one frame already covers the lossless encoder.
#[divan::bench]
fn webp_single_frame_lossless(bencher: Bencher) {
    let composited = common::composited_frame("toaster", 11, "frog.jpg");
    let frame = composited.get();
    let options = lossless();

    bencher.bench_local(|| WebpAnim::encode_single(frame, &options).expect("webp encode"));
}
