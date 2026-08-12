//! End-to-end benchmarks: the work a single `maple` invocation does.

mod common;

use divan::Bencher;
use maple::{GifAnim, Input, TextOptions, render::RenderQuality};

fn main() {
    common::init();
    divan::main();
}

fn gif_anim(template: &str, image: &str) -> GifAnim {
    let repo = common::repository(template);
    let period = repo.get_period();
    let hold = repo.get_hold();
    let palette = repo.get_palette();

    let mut anim = GifAnim::new(common::renders(template, image, RenderQuality::Sampled));
    anim.set_timing(period, hold);
    anim.set_palette_frames(palette);
    anim
}

/// Full animated GIF: every frame is decoded, composited, quantized and
/// LZW-compressed. This is what `maple --zip ... --gif out.gif` runs.
#[divan::bench(sample_count = 3, sample_size = 1)]
fn gif_animation(bencher: Bencher) {
    bencher.with_inputs(|| gif_anim("book", "frog.jpg")).bench_local_refs(|anim| {
        anim.apply().expect("gif apply");
        anim.encode().expect("gif encode")
    });
}

/// The same animation with Floyd-Steinberg dithering enabled.
#[divan::bench(sample_count = 3, sample_size = 1)]
fn gif_animation_dithered(bencher: Bencher) {
    bencher
        .with_inputs(|| {
            let mut anim = gif_anim("book", "frog.jpg");
            anim.set_dither(true);
            anim
        })
        .bench_local_refs(|anim| {
            anim.apply().expect("gif apply");
            anim.encode().expect("gif encode")
        });
}

/// GIF serialization on its own: palette table plus LZW compression of the
/// frame blocks produced by `apply`.
#[divan::bench(sample_count = 3, sample_size = 1)]
fn gif_serialize(bencher: Bencher) {
    let mut anim = gif_anim("book", "frog.jpg");
    anim.apply().expect("gif apply");

    bencher.bench_local(|| anim.encode().expect("gif encode"));
}

/// Decoding a user photo into a render input.
#[divan::bench]
fn load_photo(bencher: Bencher) {
    let path = common::asset("examples/monkey.jpg");

    bencher.bench_local(|| Input::load(&path).expect("load input"));
}

/// Rasterizing a text layer, the `--in "text:..."` path.
#[divan::bench]
fn text_layer(bencher: Bencher) {
    let font = common::font();
    let options = TextOptions::default();

    bencher.bench_local(|| {
        Input::from_text("Hello from maple", 1, &options, &font).expect("render text")
    });
}
