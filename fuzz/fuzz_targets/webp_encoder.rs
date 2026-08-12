#![no_main]

use std::io::Cursor;

use image::{AnimationDecoder, ImageDecoder, codecs::webp::WebPDecoder};
use libfuzzer_sys::fuzz_target;
use maple_render_core::{WebpEncoder, WebpFrame, WebpOptions, encode_webp_animation};

fuzz_target!(|data: &[u8]| {
    if data.len() < 5 {
        return;
    }
    let width = u32::from(data[0] % 16 + 1);
    let height = u32::from(data[1] % 16 + 1);
    let frame_count = usize::from(data[2] % 4 + 1);
    let frame_bytes = width as usize * height as usize * 4;
    let options = WebpOptions {
        quality: f32::from(data[3] % 101),
        lossless: data[4] & 1 != 0,
        method: usize::from(data[4] % 7),
    };

    let mut frames = Vec::with_capacity(frame_count);
    let mut cursor = 5;
    for frame_index in 0..frame_count {
        let mut rgba = vec![0; frame_bytes];
        for byte in &mut rgba {
            *byte = data.get(cursor).copied().unwrap_or(frame_index as u8);
            cursor = cursor.saturating_add(1);
        }
        frames.push(rgba);
    }
    let borrowed: Vec<_> = frames
        .iter()
        .enumerate()
        .map(|(index, rgba)| WebpFrame::new(rgba, index as i32 * 3 + 1))
        .collect();
    if let Ok(encoded) =
        encode_webp_animation((width, height), &borrowed, frame_count as i32 * 3 + 1, options, 0)
    {
        let decoder = WebPDecoder::new(Cursor::new(encoded)).expect("encoder output must decode");
        if decoder.has_animation() {
            let decoded = decoder.into_frames().collect_frames().expect("frames must decode");
            assert!(!decoded.is_empty());
        } else {
            let mut pixels = vec![0; decoder.total_bytes() as usize];
            decoder.read_image(&mut pixels).expect("still must decode");
        }
    }

    let mut streaming = WebpEncoder::new((width, height), options).expect("valid dimensions");
    for (index, rgba) in frames.iter().enumerate() {
        streaming.add_frame(rgba, index as i32 * 3 + 1).expect("valid frame");
    }
    let encoded = streaming.finish(frame_count as i32 * 3 + 1).expect("valid timeline");
    WebPDecoder::new(Cursor::new(encoded)).expect("streaming output must decode");

    let mut invalid = WebpEncoder::new((width, height), options).expect("valid dimensions");
    let short = &frames[0][..frame_bytes.saturating_sub(1)];
    assert!(invalid.add_frame(short, 0).is_err());
    assert!(invalid.add_frame(&frames[0], 0).is_ok());
    assert!(invalid.add_frame(&frames[0], 0).is_err());
});
