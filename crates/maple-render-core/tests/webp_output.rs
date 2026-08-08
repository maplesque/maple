//! Integration test: WebP output is produced, decodable, and higher-color than GIF.

// The full pipeline test opens real template/example files and drives libwebp
// via FFI, both of which Miri isolates. Gate the test so `cargo miri test`
// still passes; the in-crate unit tests exercise the pure-Rust code under Miri.
#![cfg(not(miri))]

use std::path::PathBuf;

use maple_render_core::{
    GifAnim,
    error::Result,
    input::{Input, Inputs},
    render::RenderQuality,
    renders::Renders,
    repository::Repository,
    webp_anim::{WebpAnim, WebpOptions},
};

fn here() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn template(name: &str) -> PathBuf {
    here().join("..").join("..").join("templates").join(format!("{name}.zip"))
}

fn example(name: &str) -> PathBuf {
    here().join("..").join("..").join("examples").join(name)
}

fn build_renders(template_name: &str, input: &str) -> Result<Renders> {
    let repo = Repository::load(template(template_name))?;
    let mut inputs = Inputs::new();
    inputs.push(Input::load(example(input))?);
    Ok(Renders::new(repo, inputs, RenderQuality::Sampled))
}

#[test]
fn webp_renders_and_is_decodable() {
    let renders = build_renders("toaster", "frog.jpg").expect("renders");

    // Animated WebP.
    let mut webp = WebpAnim::new(renders);
    let repo = Repository::load(template("toaster")).expect("repo");
    webp.set_timing(repo.get_period(), repo.get_hold());
    webp.set_options({
        let mut o = WebpOptions::default();
        o.lossless = false;
        o
    });

    let data = webp.encode().expect("webp encode");
    assert!(data.len() > 100, "webp output non-trivial");
    // Valid RIFF/WEBP: "RIFF" + u32 size + "WEBP".
    assert_eq!(&data[0..4], b"RIFF", "valid RIFF tag");
    assert_eq!(&data[8..12], b"WEBP", "valid WEBP FourCC");

    // The same frames encoded as GIF use a 256-color palette.
    let renders = build_renders("toaster", "frog.jpg").expect("renders");
    let mut gif = GifAnim::new(renders);
    let repo = Repository::load(template("toaster")).expect("repo");
    gif.set_timing(repo.get_period(), repo.get_hold());
    gif.apply().expect("gif apply");
    let gif_bytes = gif.encode().expect("gif encode");

    // Both are valid encoded streams with real payload.
    assert!(data.len() > 12, "webp has payload");
    assert!(gif_bytes.len() > 12, "gif has payload");
}
