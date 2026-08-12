//! Benchmarks for template loading and frame compositing.
//!
//! `Render::apply` is the hot path of every maple output: it is run once per
//! animation frame, for every input layer.

mod common;

use divan::Bencher;
use maple::{Repository, render::RenderQuality};

fn main() {
    common::init();
    divan::main();
}

/// Parsing a template archive: zip central directory plus `template.json`.
#[divan::bench]
fn open_template(bencher: Bencher) {
    let bytes = common::template_bytes("toaster");

    bencher
        .with_inputs(|| bytes.clone())
        .bench_local_values(|bytes| Repository::load_from_bytes(bytes).expect("load template"));
}

/// Decoding the five PNG layers (light, dark, map, sel, transparent) of a frame.
#[divan::bench]
fn decode_frame_layers(bencher: Bencher) {
    bencher
        .with_inputs(|| common::repository("toaster"))
        .bench_local_refs(|repo| repo.take_mapping(11).expect("take mapping"));
}

/// Compositing a frame with the default 9-tap antialiased sampler.
#[divan::bench]
fn composite_sampled(bencher: Bencher) {
    let inputs = common::inputs("frog.jpg");
    let mut render = common::render("toaster", 11, RenderQuality::Sampled);

    bencher.bench_local(|| render.apply(&inputs).expect("apply render"));
}

/// Compositing the same frame with the cheaper nearest-neighbour sampler.
#[divan::bench]
fn composite_simple(bencher: Bencher) {
    let inputs = common::inputs("frog.jpg");
    let mut render = common::render("toaster", 11, RenderQuality::Simple);

    bencher.bench_local(|| render.apply(&inputs).expect("apply render"));
}

/// A template whose frames use the edge-smoothing post-process.
#[divan::bench]
fn composite_multi_layer(bencher: Bencher) {
    let inputs = common::inputs("monkey.jpg");
    let mut render = common::render("book", 6, RenderQuality::Sampled);

    bencher.bench_local(|| render.apply(&inputs).expect("apply render"));
}

/// Compositing plus the downscale/letterbox pass used by `-w`/`-H`.
#[divan::bench]
fn composite_scaled(bencher: Bencher) {
    let inputs = common::inputs("frog.jpg");
    let mut render = common::render("toaster", 11, RenderQuality::Sampled);

    bencher.bench_local(|| render.apply_scaled(&inputs, 800, 600).expect("apply render"));
}

/// The point cloud scan backing `--auto_zoom` and layer detection.
#[divan::bench]
fn frame_cloud(bencher: Bencher) {
    let input = common::input("frog.jpg");
    let render = common::render("toaster", 11, RenderQuality::Sampled);

    bencher.bench_local(|| render.get_cloud(&input).expect("get cloud"));
}
