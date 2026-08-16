//! Integration test: `GifAnim` uses the template's own palette/timing metadata
//! when the caller does not explicitly configure them (regression for the
//! bot-style usage that produced a blank single-frame GIF from
//! `billboard-cityscape` because frame 0 renders to a uniform palette image).

// The test opens real template/example files, which Miri isolates. Gate it so
// `cargo miri test` still passes.
#![cfg(not(miri))]

use std::{fs, io::Cursor, path::PathBuf};

use image::{AnimationDecoder, codecs::gif::GifDecoder};
use maple_render_core::{
    GifAnim,
    error::Result,
    input::{Input, Inputs},
    render::RenderQuality,
    renders::Renders,
    repository::Repository,
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

fn load_repository(template_name: &str) -> Result<Repository> {
    let bytes = fs::read(template(template_name))?;
    Repository::load_from_bytes(bytes)
}

fn build_renders(template_name: &str, input: &str) -> Result<Renders> {
    let repo = load_repository(template_name)?;
    let mut inputs = Inputs::new();
    inputs.push(Input::load(example(input))?);
    Ok(Renders::new(repo, inputs, RenderQuality::Sampled))
}

fn decode_gif(data: &[u8]) -> Vec<image::Frame> {
    GifDecoder::new(Cursor::new(data))
        .expect("decode GIF")
        .into_frames()
        .collect_frames()
        .expect("decode frames")
}

#[test]
fn gif_defaults_use_template_palette_and_timing() {
    if !template("billboard-cityscape").is_file() || !example("frog.jpg").is_file() {
        // Published crates intentionally omit Maple's template and example assets.
        return;
    }

    // Mode A: no explicit configuration, exactly how the bot's images.rs calls
    // into maple-render-core. This used to emit a blank single-frame GIF.
    let renders = build_renders("billboard-cityscape", "frog.jpg").expect("renders");
    let mut gif = GifAnim::new(renders);
    gif.apply().expect("gif apply");
    let default_bytes = gif.encode().expect("gif encode");

    // Mode B: template metadata applied explicitly (CLI/wasm behavior).
    let renders = build_renders("billboard-cityscape", "frog.jpg").expect("renders");
    let mut gif = GifAnim::new(renders);
    let repo = load_repository("billboard-cityscape").expect("repo");
    gif.set_palette_frames(repo.get_palette());
    gif.set_timing(repo.get_period(), repo.get_hold());
    gif.apply().expect("gif apply");
    let explicit_bytes = gif.encode().expect("gif encode");

    // The default path must animate (not collapse to a blank still) and must
    // match the explicitly-configured output exactly.
    let default_frames = decode_gif(&default_bytes);
    let explicit_frames = decode_gif(&explicit_bytes);

    assert!(
        default_frames.len() > 20,
        "default GIF must be animated, got {} frames",
        default_frames.len()
    );
    assert_eq!(
        default_frames.len(),
        explicit_frames.len(),
        "default and explicit GIFs must have the same frame count"
    );
    assert_eq!(
        default_bytes, explicit_bytes,
        "default GIF must be identical to explicitly-configured GIF"
    );

    // Sanity check the frames actually carry content (a palette sampled across
    // frames 7/17/27/34), not a solid blank image. Frame 0 is uniform, so scan
    // a range that includes real content.
    let mut distinct = std::collections::HashSet::new();
    for frame in default_frames.iter().skip(4).take(8) {
        for p in frame.buffer().pixels() {
            distinct.insert((p[0], p[1], p[2], p[3]));
            if distinct.len() > 64 {
                break;
            }
        }
        if distinct.len() > 64 {
            break;
        }
    }
    assert!(distinct.len() > 8, "GIF frames must contain actual content");
}

#[test]
fn gif_explicit_overrides_take_precedence() {
    if !template("billboard-cityscape").is_file() || !example("frog.jpg").is_file() {
        return;
    }

    let renders = build_renders("billboard-cityscape", "frog.jpg").expect("renders");
    let mut gif = GifAnim::new(renders);
    gif.set_palette_frames(vec![0]);
    gif.set_timing(0.1, 5.0);
    gif.apply().expect("gif apply");

    let frames = decode_gif(&gif.encode().expect("gif encode"));
    assert_eq!(frames.len(), 1, "frame-0 palette override must be honored");
    assert_eq!(frames[0].delay().numer_denom_ms(), (9_100, 1), "timing override must be honored");
}

#[test]
fn gif_defaults_still_encode_single_palette_templates() {
    if !template("toaster").is_file() || !example("frog.jpg").is_file() {
        return;
    }

    // Templates whose palette is already [0] (e.g. toaster) must be unaffected:
    // the fallback resolves to the same frames the hard-coded default used.
    let renders = build_renders("toaster", "frog.jpg").expect("renders");
    let mut gif = GifAnim::new(renders);
    gif.apply().expect("gif apply");
    let bytes = gif.encode().expect("gif encode");

    let frames = decode_gif(&bytes);
    assert!(frames.len() > 5, "toaster GIF must be animated, got {} frames", frames.len());
    assert!(bytes.len() > 100_000, "toaster GIF must carry content, got {} bytes", bytes.len());
}
