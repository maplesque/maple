//! Integration test: WebP output is produced, decodable, and higher-color than GIF.

// The full pipeline test opens real template/example files and drives libwebp
// via FFI, both of which Miri isolates. Gate the test so `cargo miri test`
// still passes; the in-crate unit tests exercise the pure-Rust code under Miri.
#![cfg(all(feature = "webp", not(miri)))]

use std::{io::Cursor, path::PathBuf};

use image::{AnimationDecoder, codecs::webp::WebPDecoder};
use maple_render_core::{
    GifAnim,
    error::Result,
    input::{Input, Inputs},
    render::RenderQuality,
    renders::Renders,
    repository::Repository,
    webp_anim::{
        WebpAnim, WebpAnimationOptions, WebpEncoder, WebpFrame, WebpOptions, encode_webp_animation,
    },
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
    if !template("toaster").is_file() || !example("frog.jpg").is_file() {
        // Published crates intentionally omit Maple's template and example assets.
        return;
    }

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
    let decoded = WebPDecoder::new(Cursor::new(&data))
        .expect("decode WebP")
        .into_frames()
        .collect_frames()
        .expect("decode WebP frames");
    assert!(!decoded.is_empty());
    assert!(decoded.len() as u32 <= repo.length());
    assert_eq!(decoded[0].buffer().dimensions(), (400, 300));

    let mut streaming_renders = build_renders("toaster", "frog.jpg").expect("renders");
    let frame_count = streaming_renders.length() as i32;
    let dimensions = streaming_renders.get_render(0).expect("first frame").get().dimensions();
    let mut encoder = WebpEncoder::new_with_animation_options(
        dimensions,
        WebpOptions::default(),
        WebpAnimationOptions { kmin: 3, kmax: 5, ..Default::default() },
    )
    .expect("streaming encoder");
    let mut current_ms = 0.0f64;
    let mut previous_timestamp = -1;
    for index in 0..frame_count {
        let step = if index == frame_count - 1 {
            repo.get_period() + repo.get_hold()
        } else {
            repo.get_period()
        };
        let mut timestamp = current_ms.round() as i32;
        if timestamp <= previous_timestamp {
            timestamp = previous_timestamp + 1;
        }
        encoder
            .add_frame(
                streaming_renders.get_render(index).expect("render frame").get().as_raw(),
                timestamp,
            )
            .expect("streaming frame");
        streaming_renders.remove_render(index);
        previous_timestamp = timestamp;
        current_ms += step * 1000.0;
    }
    let mut final_timestamp = current_ms.round() as i32;
    if final_timestamp <= previous_timestamp {
        final_timestamp = previous_timestamp + 1;
    }
    assert_eq!(
        data,
        encoder.finish(final_timestamp).expect("streaming output"),
        "WebpAnim must retain the bounded-memory streaming path"
    );

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

fn decode_webp(data: &[u8]) -> Vec<image::Frame> {
    WebPDecoder::new(Cursor::new(data))
        .expect("decode WebP")
        .into_frames()
        .collect_frames()
        .expect("decode frames")
}

#[test]
fn batch_encoder_preserves_lossless_replacement_and_timing() {
    let dimensions = (10, 8);
    let red = [255, 0, 0, 255].repeat(dimensions.0 as usize * dimensions.1 as usize);
    let mut odd_change = red.clone();
    let odd_pixel = (3 + 3 * dimensions.0 as usize) * 4;
    odd_change[odd_pixel..odd_pixel + 4].copy_from_slice(&[0, 255, 0, 128]);
    let mut transparent = odd_change.clone();
    transparent[odd_pixel..odd_pixel + 4].copy_from_slice(&[17, 29, 41, 0]);
    let frames = [
        WebpFrame::new(&red, 0),
        WebpFrame::new(&odd_change, 30),
        WebpFrame::new(&transparent, 70),
    ];

    let data = encode_webp_animation(
        dimensions,
        &frames,
        120,
        WebpOptions { quality: 0.0, lossless: true, method: 0 },
        0,
    )
    .expect("batch encode");
    let decoded = decode_webp(&data);

    assert_eq!(decoded.len(), 3);
    assert_eq!(decoded[0].buffer().as_raw(), &red);
    assert_eq!(decoded[1].buffer().as_raw(), &odd_change);
    assert_eq!(decoded[2].buffer().as_raw(), &transparent);
    assert_eq!(decoded[0].delay().numer_denom_ms(), (30, 1));
    assert_eq!(decoded[1].delay().numer_denom_ms(), (40, 1));
    assert_eq!(decoded[2].delay().numer_denom_ms(), (50, 1));
}

#[test]
fn batch_encoder_coalesces_identical_frames_without_losing_duration() {
    let red = [255, 0, 0, 255].repeat(8 * 8);
    let blue = [0, 0, 255, 255].repeat(8 * 8);
    let frames = [WebpFrame::new(&red, 0), WebpFrame::new(&red, 20), WebpFrame::new(&blue, 50)];
    let data = encode_webp_animation(
        (8, 8),
        &frames,
        100,
        WebpOptions { quality: 0.0, lossless: true, method: 0 },
        0,
    )
    .expect("batch encode");
    let decoded = decode_webp(&data);

    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].buffer().as_raw(), &red);
    assert_eq!(decoded[1].buffer().as_raw(), &blue);
    assert_eq!(decoded[0].delay().numer_denom_ms(), (50, 1));
    assert_eq!(decoded[1].delay().numer_denom_ms(), (50, 1));
}

fn rgb_mean_absolute_error(actual: &[u8], expected: &[u8]) -> f64 {
    let error: u64 = actual
        .chunks_exact(4)
        .zip(expected.chunks_exact(4))
        .map(|(actual, expected)| {
            u64::from(actual[0].abs_diff(expected[0]))
                + u64::from(actual[1].abs_diff(expected[1]))
                + u64::from(actual[2].abs_diff(expected[2]))
        })
        .sum();
    error as f64 / (actual.len() / 4 * 3) as f64
}

#[test]
fn batch_encoder_retains_duplicates_when_merged_duration_would_overflow() {
    let pixel = [255, 0, 0, 255];
    let frames = [WebpFrame::new(&pixel, 0), WebpFrame::new(&pixel, 10_000_000)];
    let data = encode_webp_animation(
        (1, 1),
        &frames,
        20_000_000,
        WebpOptions { quality: 0.0, lossless: true, method: 0 },
        0,
    )
    .expect("batch encode");
    let decoded = decode_webp(&data);

    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].delay().numer_denom_ms(), (10_000_000, 1));
    assert_eq!(decoded[1].delay().numer_denom_ms(), (10_000_000, 1));
}

#[test]
fn batch_lossy_quality_tracks_streaming_encoder() {
    let dimensions = (64, 48);
    let mut frames = Vec::new();
    for frame_index in 0..4usize {
        let mut pixels = Vec::with_capacity(dimensions.0 as usize * dimensions.1 as usize * 4);
        for y in 0..dimensions.1 as usize {
            for x in 0..dimensions.0 as usize {
                pixels.extend_from_slice(&[
                    (x * 3 + y + frame_index * 5) as u8,
                    (y * 5 + x / 2) as u8,
                    (x + y * 2) as u8,
                    255,
                ]);
            }
        }
        let left = 3 + frame_index * 7;
        for y in 11..25 {
            for x in left..left + 9 {
                let offset = (y * dimensions.0 as usize + x) * 4;
                pixels[offset..offset + 4].copy_from_slice(&[240, 20, 180, 255]);
            }
        }
        frames.push(pixels);
    }
    let options = WebpOptions { quality: 85.0, lossless: false, method: 0 };
    let batch_frames: Vec<_> = frames
        .iter()
        .enumerate()
        .map(|(index, frame)| WebpFrame::new(frame, index as i32 * 40))
        .collect();
    let batch =
        encode_webp_animation(dimensions, &batch_frames, 160, options, 0).expect("batch encode");
    let mut streaming = WebpEncoder::new(dimensions, options).expect("streaming encoder");
    for (index, frame) in frames.iter().enumerate() {
        streaming.add_frame(frame, index as i32 * 40).expect("streaming frame");
    }
    let streaming = streaming.finish(160).expect("streaming finish");
    let batch_decoded = decode_webp(&batch);
    let streaming_decoded = decode_webp(&streaming);

    assert_eq!(batch_decoded.len(), frames.len());
    assert_eq!(streaming_decoded.len(), frames.len());
    for ((batch_frame, streaming_frame), source) in
        batch_decoded.iter().zip(&streaming_decoded).zip(&frames)
    {
        let batch_error = rgb_mean_absolute_error(batch_frame.buffer().as_raw(), source);
        let streaming_error = rgb_mean_absolute_error(streaming_frame.buffer().as_raw(), source);
        assert!(batch_error < 12.0, "batch RGB error {batch_error:.2} is too high");
        assert!(
            batch_error <= streaming_error + 3.0,
            "batch RGB error {batch_error:.2} exceeds streaming error {streaming_error:.2}"
        );
        assert!(batch_frame.buffer().pixels().all(|pixel| pixel[3] == 255));
        assert_eq!(batch_frame.delay().numer_denom_ms(), (40, 1));
    }
}

#[test]
fn streaming_encoder_preserves_timing_and_lossless_pixels() {
    let red = [255, 0, 0, 255].repeat(8 * 8);
    let blue = [0, 0, 255, 255].repeat(8 * 8);
    let mut encoder =
        WebpEncoder::new((8, 8), WebpOptions { quality: 0.0, lossless: true, method: 0 })
            .expect("encoder");
    encoder.add_frame(&red, 0).expect("red frame");
    encoder.add_frame(&blue, 40).expect("blue frame");
    let data = encoder.finish(120).expect("finish");
    let decoded = WebPDecoder::new(Cursor::new(data))
        .expect("decode WebP")
        .into_frames()
        .collect_frames()
        .expect("decode frames");

    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].buffer().as_raw(), &red);
    assert!(
        decoded[1]
            .buffer()
            .as_raw()
            .iter()
            .zip(&blue)
            .all(|(actual, expected)| actual.abs_diff(*expected) <= 1),
        "image-webp's alpha compositor may round opaque channels down by one"
    );
    assert_eq!(decoded[0].delay().numer_denom_ms(), (40, 1));
    assert_eq!(decoded[1].delay().numer_denom_ms(), (80, 1));
}
