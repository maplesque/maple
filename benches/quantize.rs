//! Benchmarks for the median-cut quantizer used by the GIF encoder.
//!
//! Every GIF frame goes through `Quantizer::quantize`, and the palette is
//! rebuilt for each palette frame of a template.

mod common;

use divan::Bencher;
use maple::quantize::Quantizer;

fn main() {
    common::init();
    divan::main();
}

/// Histogram prescan plus median-cut selection of the 256 palette entries.
#[divan::bench]
fn build_palette(bencher: Bencher) {
    let composited = common::composited_frame("toaster", 11, "frog.jpg");
    let frame = composited.get();

    bencher.bench_local(|| Quantizer::new(frame));
}

/// Mapping a frame onto the palette with a cold inverse colormap cache.
#[divan::bench]
fn map_frame_cold_cache(bencher: Bencher) {
    let composited = common::composited_frame("toaster", 11, "frog.jpg");
    let frame = composited.get();

    bencher
        .with_inputs(|| Quantizer::new(frame))
        .bench_local_refs(|quantizer| quantizer.quantize_no_dither(frame));
}

/// Mapping a frame once the colormap cache is warm, which is what every frame
/// after the first one of an animation hits.
#[divan::bench]
fn map_frame_warm_cache(bencher: Bencher) {
    let composited = common::composited_frame("toaster", 11, "frog.jpg");
    let frame = composited.get();
    let mut quantizer = Quantizer::new(frame);
    quantizer.quantize_no_dither(frame);

    bencher.bench_local(|| quantizer.quantize_no_dither(frame));
}

/// Floyd-Steinberg dithering, enabled with `--dither`.
#[divan::bench]
fn map_frame_dithered(bencher: Bencher) {
    let composited = common::composited_frame("toaster", 11, "frog.jpg");
    let frame = composited.get();
    let mut quantizer = Quantizer::new(frame);
    quantizer.quantize_fs_dither(frame);

    bencher.bench_local(|| quantizer.quantize_fs_dither(frame));
}
