//! Shared asset helpers for the maple benchmarks.
//!
//! Benchmarks live in the binary crate because that is where the shipped
//! templates, example images and fonts are: `maple-render-core` deliberately
//! bundles none of them.
#![allow(dead_code)]

use std::path::PathBuf;

use maple::{Input, Inputs, Render, Renders, Repository, mapping::Mapping, render::RenderQuality};

/// Prepare the process before handing over to divan.
///
/// The compositor parallelizes rows with rayon. Left to itself, the number of
/// worker threads and the way work is split vary from machine to machine, which
/// shows up as noise in the measurements, so benchmarks default to a single
/// thread (override with `RAYON_NUM_THREADS`). One throwaway render then warms
/// the thread pool up, keeping its lazy initialization out of the first
/// measured iteration.
pub fn init() {
    if std::env::var_os("RAYON_NUM_THREADS").is_none() {
        std::env::set_var("RAYON_NUM_THREADS", "1");
    }

    let mut warmup = render("toaster", 0, RenderQuality::Sampled);
    warmup.apply(&inputs("frog.jpg")).expect("warmup render");
}

/// Path of an asset relative to the repository root.
pub fn asset(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path)
}

/// Raw bytes of a shipped template archive.
pub fn template_bytes(name: &str) -> Vec<u8> {
    let path = asset(&format!("templates/{}.zip", name));
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {}", path.display(), e))
}

/// Open a template archive from memory, so that benchmarks measure parsing and
/// not the file system.
pub fn repository(name: &str) -> Repository {
    Repository::load_from_bytes(template_bytes(name)).expect("load template")
}

/// Decode the five PNG layers of a single template frame.
pub fn mapping(template: &str, frame: i32) -> Mapping {
    repository(template).take_mapping(frame).expect("take mapping")
}

/// Load one of the shipped example photos as a render input.
pub fn input(image: &str) -> Input {
    Input::load(asset(&format!("examples/{}", image))).expect("load input")
}

pub fn inputs(image: &str) -> Inputs {
    let mut inputs = Inputs::new();
    inputs.push(input(image));
    inputs
}

/// A render with the frame's mapping already attached, ready to `apply`.
pub fn render(template: &str, frame: i32, quality: RenderQuality) -> Render {
    let mut render = Render::new(quality);
    render.attach_mapping(mapping(template, frame));
    render
}

/// A render holding a composited frame, the input every color quantization
/// step works on. Call [`Render::get`] to reach the image.
pub fn composited_frame(template: &str, frame: i32, image: &str) -> Render {
    let mut render = render(template, frame, RenderQuality::Sampled);
    render.apply(&inputs(image)).expect("apply render");
    render
}

/// The full animation pipeline for a template and an input image.
pub fn renders(template: &str, image: &str, quality: RenderQuality) -> Renders {
    Renders::new(repository(template), inputs(image), quality)
}

/// The same pipeline with every frame composited up front, so that a benchmark
/// can measure serialization on its own. [`Renders`] composites frames lazily
/// on first access, which would otherwise be counted with the encoding.
pub fn warm_renders(template: &str, image: &str, quality: RenderQuality) -> Renders {
    let mut renders = renders(template, image, quality);

    for frame in 0..renders.length() as i32 {
        renders.get_render(frame).expect("composite frame");
    }

    renders
}

pub fn font() -> Vec<u8> {
    std::fs::read(asset("fonts/DejaVuSans-Bold.ttf")).expect("read font")
}
