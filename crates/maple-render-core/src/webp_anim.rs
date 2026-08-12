//! Native animated WebP output backed by libwebp.

#[cfg(windows)]
use std::sync::OnceLock;
use std::{ffi::CStr, fs::File, io::Write, mem, path::Path, ptr, slice};

use libwebp_sys as ffi;
use rayon::prelude::*;

use crate::{
    error::{Error, Result},
    renders::Renders,
};

/// Quality used for lossy WebP encoding.
pub const DEFAULT_WEBP_QUALITY: f32 = 95.0;

/// Balanced libwebp method used by the published encoder.
pub const DEFAULT_WEBP_METHOD: usize = 4;

const MAX_WEBP_DIMENSION: u32 = 16_383;
const MAX_WEBP_FRAME_DURATION_MS: i64 = (1 << 24) - 1;
const PARALLEL_ENCODER_MEMORY_BUDGET: usize = 256 * 1024 * 1024;
const ESTIMATED_NATIVE_BYTES_PER_PIXEL: usize = 16;

// libwebp 1.6.0 synchronizes lazy DSP initialization on Unix, but its
// WEBP_USE_THREAD guard excludes Windows. Warm every native path Maple uses
// once there before Rayon starts concurrent frame encodes.
#[cfg(windows)]
static LIBWEBP_INITIALIZED: OnceLock<std::result::Result<(), String>> = OnceLock::new();

/// Per-frame WebP compression options.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WebpOptions {
    /// Lossy quality or lossless compression effort in the range 0 through 100.
    pub quality: f32,
    /// Use lossless compression when true.
    pub lossless: bool,
    /// Compression method in the range 0 through 6. Zero is fastest.
    pub method: usize,
}

impl Default for WebpOptions {
    fn default() -> Self {
        Self { quality: DEFAULT_WEBP_QUALITY, lossless: false, method: DEFAULT_WEBP_METHOD }
    }
}

/// Animation-level WebP options.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WebpAnimationOptions {
    /// Number of animation loops. Zero means infinite.
    pub loop_count: u16,
    /// Spend additional time minimizing the complete animation size.
    pub minimize_size: bool,
    /// Minimum distance between keyframes.
    pub kmin: i32,
    /// Maximum distance between keyframes. Zero disables keyframe insertion.
    pub kmax: i32,
    /// Permit libwebp to choose lossy or lossless compression per frame.
    pub allow_mixed: bool,
}

/// One borrowed RGBA frame in an animation timeline.
#[derive(Debug, Clone, Copy)]
pub struct WebpFrame<'a> {
    /// Tightly packed RGBA pixels for the complete canvas.
    pub rgba: &'a [u8],
    /// Start time in milliseconds. Frame timestamps must strictly increase.
    pub timestamp_ms: i32,
}

impl<'a> WebpFrame<'a> {
    pub fn new(rgba: &'a [u8], timestamp_ms: i32) -> Self {
        Self { rgba, timestamp_ms }
    }
}

/// Encode complete RGBA frames concurrently and assemble an animated WebP.
///
/// This is an opt-in high-throughput path for callers that already hold every
/// frame in memory. Unchanged pixels are omitted from later frames, while
/// libwebp encodes independent frame rectangles in parallel through Rayon.
/// Because frames are encoded independently, output can be materially larger
/// than [`WebpEncoder`]'s animation-wide optimization. Use [`WebpEncoder`] for
/// bounded memory and smaller output.
pub fn encode_webp_animation(
    dimensions: (u32, u32),
    frames: &[WebpFrame<'_>],
    final_timestamp_ms: i32,
    options: WebpOptions,
    loop_count: u16,
) -> Result<Vec<u8>> {
    let expected_frame_bytes = frame_bytes(dimensions)?;
    let options = normalize_options(options)?;
    let durations = validate_frames(frames, final_timestamp_ms, expected_frame_bytes)?;
    initialize_libwebp()?;
    let (width, height) = (dimensions.0 as usize, dimensions.1 as usize);

    let rectangles: Vec<_> = frames
        .par_iter()
        .enumerate()
        .map(|(index, frame)| {
            if index == 0 {
                Some(FrameRect::full(width, height))
            } else {
                dirty_rect(frames[index - 1].rgba, frame.rgba, width)
            }
        })
        .collect();

    let mut plans: Vec<FramePlan<'_>> = Vec::with_capacity(frames.len());
    for ((frame, rectangle), duration_ms) in
        frames.iter().zip(rectangles).zip(durations.into_iter())
    {
        if let Some(rectangle) = rectangle {
            plans.push(FramePlan { rgba: frame.rgba, rectangle, duration_ms });
        } else {
            let previous = plans.last_mut().expect("the first frame is always present");
            if previous.duration_ms <= MAX_WEBP_FRAME_DURATION_MS - duration_ms {
                previous.duration_ms += duration_ms;
            } else {
                plans.push(FramePlan {
                    rgba: frame.rgba,
                    rectangle: FrameRect::full(width, height),
                    duration_ms,
                });
            }
        }
    }

    let mut encoded = Vec::with_capacity(plans.len());
    let mut remaining = plans.as_slice();
    while !remaining.is_empty() {
        let chunk_len = parallel_chunk_len(remaining);
        let (chunk, rest) = remaining.split_at(chunk_len);
        let encoded_chunk: Result<Vec<_>> =
            chunk.par_iter().map(|plan| encode_frame_rect(plan, width, options)).collect();
        encoded.extend(encoded_chunk?);
        remaining = rest;
    }
    mux_frames(
        dimensions,
        &plans,
        &encoded,
        WebpAnimationOptions { loop_count, ..Default::default() },
    )
}

#[derive(Debug, Clone, Copy)]
struct FrameRect {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

impl FrameRect {
    fn full(width: usize, height: usize) -> Self {
        Self { x: 0, y: 0, width, height }
    }
}

#[derive(Debug, Clone, Copy)]
struct FramePlan<'a> {
    rgba: &'a [u8],
    rectangle: FrameRect,
    duration_ms: i64,
}

fn parallel_chunk_len(plans: &[FramePlan<'_>]) -> usize {
    parallel_chunk_len_for(plans, rayon::current_num_threads())
}

fn parallel_chunk_len_for(plans: &[FramePlan<'_>], available_threads: usize) -> usize {
    let mut estimated_bytes = 0usize;
    let mut chunk_len = 0;
    for plan in plans.iter().take(available_threads.max(1)) {
        let plan_bytes = plan
            .rectangle
            .width
            .saturating_mul(plan.rectangle.height)
            .saturating_mul(ESTIMATED_NATIVE_BYTES_PER_PIXEL)
            .max(1);
        if chunk_len > 0
            && estimated_bytes.saturating_add(plan_bytes) > PARALLEL_ENCODER_MEMORY_BUDGET
        {
            break;
        }
        estimated_bytes = estimated_bytes.saturating_add(plan_bytes);
        chunk_len += 1;
    }
    chunk_len.max(1)
}

fn validate_frames(
    frames: &[WebpFrame<'_>],
    final_timestamp_ms: i32,
    expected_frame_bytes: usize,
) -> Result<Vec<i64>> {
    if frames.is_empty() {
        return Err(webp_error("no frames were provided"));
    }
    for frame in frames {
        if frame.rgba.len() != expected_frame_bytes {
            return Err(Error::WebpEncode(format!(
                "RGBA frame has {} bytes; expected {}",
                frame.rgba.len(),
                expected_frame_bytes
            )));
        }
    }
    for timestamps in frames.windows(2) {
        if timestamps[1].timestamp_ms <= timestamps[0].timestamp_ms {
            return Err(webp_error("frame timestamps must be strictly increasing"));
        }
    }
    if final_timestamp_ms < frames.last().unwrap().timestamp_ms {
        return Err(webp_error("final timestamp must not precede the last frame"));
    }

    frames
        .iter()
        .enumerate()
        .map(|(index, frame)| {
            let next_timestamp =
                frames.get(index + 1).map_or(final_timestamp_ms, |next| next.timestamp_ms);
            let duration = i64::from(next_timestamp) - i64::from(frame.timestamp_ms);
            if duration > MAX_WEBP_FRAME_DURATION_MS {
                Err(webp_error("frame duration exceeds WebP's limit"))
            } else {
                Ok(duration)
            }
        })
        .collect()
}

fn dirty_rect(previous: &[u8], current: &[u8], width: usize) -> Option<FrameRect> {
    let row_bytes = width * 4;
    let mut rows = previous.chunks_exact(row_bytes).zip(current.chunks_exact(row_bytes));
    let top = rows.clone().position(|(before, after)| before != after)?;
    let bottom = rows.rposition(|(before, after)| before != after).unwrap();
    let mut left = width;
    let mut right = 0;

    for y in top..=bottom {
        let start = y * row_bytes;
        let before = &previous[start..start + row_bytes];
        let after = &current[start..start + row_bytes];
        if before == after {
            continue;
        }
        let first_byte = before.iter().zip(after).position(|(a, b)| a != b).unwrap();
        let last_byte = before.iter().zip(after).rposition(|(a, b)| a != b).unwrap();
        left = left.min(first_byte / 4);
        right = right.max(last_byte / 4 + 1);
    }

    let x = left & !1;
    let y = top & !1;
    Some(FrameRect { x, y, width: right - x, height: bottom + 1 - y })
}

unsafe extern "C" fn write_webp_memory(
    data: *const u8,
    size: usize,
    picture: *const ffi::WebPPicture,
) -> i32 {
    // SAFETY: libwebp invokes this callback with its live picture and output
    // bytes. custom_ptr was initialized to a live WebPMemoryWriter below.
    unsafe { ffi::WebPMemoryWrite(data, size, picture) }
}

fn encode_frame_rect(
    plan: &FramePlan<'_>,
    canvas_width: usize,
    options: WebpOptions,
) -> Result<Vec<u8>> {
    let rect = plan.rectangle;
    // SAFETY: the picture, config, and memory writer are initialized through
    // libwebp before use. Every native allocation is released before return.
    unsafe {
        let mut config = mem::zeroed();
        if ffi::WebPConfigInitInternal(
            &mut config,
            ffi::WebPPreset::WEBP_PRESET_DEFAULT,
            75.0,
            ffi::WEBP_ENCODER_ABI_VERSION as i32,
        ) == 0
        {
            return Err(webp_error("could not initialize frame options"));
        }
        config.lossless = i32::from(options.lossless);
        config.quality = options.quality;
        config.method = options.method as i32;
        config.exact = i32::from(options.lossless);
        if ffi::WebPValidateConfig(&config) == 0 {
            return Err(webp_error("libwebp rejected the frame options"));
        }

        let mut picture = mem::zeroed();
        if ffi::WebPPictureInitInternal(&mut picture, ffi::WEBP_ENCODER_ABI_VERSION as i32) == 0 {
            return Err(webp_error("could not initialize a frame"));
        }
        picture.width = rect.width as i32;
        picture.height = rect.height as i32;
        picture.use_argb = 1;
        let offset = (rect.y * canvas_width + rect.x) * 4;
        if ffi::WebPPictureImportRGBA(
            &mut picture,
            plan.rgba.as_ptr().add(offset),
            (canvas_width * 4) as i32,
        ) == 0
        {
            ffi::WebPPictureFree(&mut picture);
            return Err(webp_error("could not import an RGBA frame"));
        }

        let mut writer = mem::zeroed();
        ffi::WebPMemoryWriterInit(&mut writer);
        picture.writer = Some(write_webp_memory);
        picture.custom_ptr = (&mut writer as *mut ffi::WebPMemoryWriter).cast();
        let encoded = ffi::WebPEncode(&config, &mut picture) != 0;
        let output = if encoded && !writer.mem.is_null() {
            slice::from_raw_parts(writer.mem, writer.size).to_vec()
        } else {
            Vec::new()
        };
        ffi::WebPMemoryWriterClear(&mut writer);
        ffi::WebPPictureFree(&mut picture);

        if !encoded || output.is_empty() {
            Err(webp_error("could not encode a frame rectangle"))
        } else {
            Ok(output)
        }
    }
}

#[cfg(not(windows))]
fn initialize_libwebp() -> Result<()> {
    Ok(())
}

#[cfg(windows)]
fn initialize_libwebp() -> Result<()> {
    LIBWEBP_INITIALIZED
        .get_or_init(|| initialize_libwebp_inner().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|message| Error::WebpEncode(message.clone()))
        .copied()
}

#[cfg(windows)]
fn initialize_libwebp_inner() -> Result<()> {
    let rgba = [255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 255, 255, 255];
    let plan = FramePlan { rgba: &rgba, rectangle: FrameRect::full(2, 2), duration_ms: 1 };

    for lossless in [false, true] {
        let encoded =
            encode_frame_rect(&plan, 2, WebpOptions { lossless, ..WebpOptions::default() })?;
        let mut width = 0;
        let mut height = 0;
        // SAFETY: encoded is a live WebP bitstream. libwebp owns the returned
        // decode buffer until WebPFree releases it below.
        let decoded = unsafe {
            ffi::WebPDecodeRGBA(encoded.as_ptr(), encoded.len(), &mut width, &mut height)
        };
        if decoded.is_null() {
            return Err(webp_error("could not initialize libwebp's decoder"));
        }
        unsafe { ffi::WebPFree(decoded.cast()) };
    }
    Ok(())
}

fn mux_frames(
    dimensions: (u32, u32),
    plans: &[FramePlan<'_>],
    encoded: &[Vec<u8>],
    animation: WebpAnimationOptions,
) -> Result<Vec<u8>> {
    // SAFETY: encoded frame buffers remain alive until assembly because mux
    // receives non-owning references. The mux and assembled data are released
    // on every return path.
    unsafe {
        let mux = ffi::WebPMuxNew();
        if mux.is_null() {
            return Err(webp_error("could not create WebP muxer"));
        }
        let result = (|| {
            let canvas_status =
                ffi::WebPMuxSetCanvasSize(mux, dimensions.0 as i32, dimensions.1 as i32);
            if canvas_status != ffi::WebPMuxError::WEBP_MUX_OK {
                return Err(mux_error("could not set animation canvas", canvas_status));
            }
            let params = ffi::WebPMuxAnimParams {
                bgcolor: 0xffff_ffff,
                loop_count: i32::from(animation.loop_count),
            };
            let params_status = ffi::WebPMuxSetAnimationParams(mux, &params);
            if params_status != ffi::WebPMuxError::WEBP_MUX_OK {
                return Err(mux_error("could not set animation options", params_status));
            }

            for (plan, bitstream) in plans.iter().zip(encoded) {
                let frame = ffi::WebPMuxFrameInfo {
                    bitstream: ffi::WebPData { bytes: bitstream.as_ptr(), size: bitstream.len() },
                    x_offset: plan.rectangle.x as i32,
                    y_offset: plan.rectangle.y as i32,
                    duration: plan.duration_ms as i32,
                    id: ffi::WebPChunkId::WEBP_CHUNK_ANMF,
                    dispose_method: ffi::WebPMuxAnimDispose::WEBP_MUX_DISPOSE_NONE,
                    blend_method: ffi::WebPMuxAnimBlend::WEBP_MUX_NO_BLEND,
                    pad: [0],
                };
                let frame_status = ffi::WebPMuxPushFrame(mux, &frame, 0);
                if frame_status != ffi::WebPMuxError::WEBP_MUX_OK {
                    return Err(mux_error("could not add animation frame", frame_status));
                }
            }

            let mut data = mem::zeroed();
            ffi::WebPDataInit(&mut data);
            let assemble_status = ffi::WebPMuxAssemble(mux, &mut data);
            let output =
                if assemble_status == ffi::WebPMuxError::WEBP_MUX_OK && !data.bytes.is_null() {
                    slice::from_raw_parts(data.bytes, data.size).to_vec()
                } else {
                    Vec::new()
                };
            ffi::WebPDataClear(&mut data);
            if assemble_status != ffi::WebPMuxError::WEBP_MUX_OK {
                Err(mux_error("could not assemble animation", assemble_status))
            } else if output.is_empty() {
                Err(webp_error("muxer returned an empty file"))
            } else {
                Ok(output)
            }
        })();
        ffi::WebPMuxDelete(mux);
        result
    }
}

fn mux_error(message: &str, status: ffi::WebPMuxError) -> Error {
    Error::WebpEncode(format!("{message} (libwebp mux status {status:?})"))
}

/// Incremental RGBA animation encoder shared by Maple consumers.
///
/// The input slice is borrowed only for the duration of [`Self::add_frame`].
/// libwebp performs architecture-specific runtime dispatch internally and
/// falls back to its scalar implementation on unsupported CPUs.
pub struct WebpEncoder {
    raw: *mut ffi::WebPAnimEncoder,
    picture: ffi::WebPPicture,
    config: ffi::WebPConfig,
    expected_frame_bytes: usize,
    stride: i32,
    previous_timestamp: Option<i32>,
    failed: bool,
}

impl WebpEncoder {
    /// Create an encoder with default animation settings.
    pub fn new(dimensions: (u32, u32), options: WebpOptions) -> Result<Self> {
        Self::new_with_animation_options(dimensions, options, WebpAnimationOptions::default())
    }

    /// Create an encoder with explicit frame and animation settings.
    pub fn new_with_animation_options(
        dimensions: (u32, u32),
        options: WebpOptions,
        animation: WebpAnimationOptions,
    ) -> Result<Self> {
        let expected_frame_bytes = frame_bytes(dimensions)?;
        let options = normalize_options(options)?;
        validate_animation_options(animation)?;
        initialize_libwebp()?;
        let (width, height) = (dimensions.0 as i32, dimensions.1 as i32);

        // SAFETY: each libwebp structure is initialized before use, checked for
        // failure, owned by this value, and released in Drop.
        unsafe {
            let mut animation_config = mem::zeroed();
            if ffi::WebPAnimEncoderOptionsInitInternal(
                &mut animation_config,
                ffi::WEBP_MUX_ABI_VERSION as i32,
            ) == 0
            {
                return Err(webp_error("could not initialize animation options"));
            }
            animation_config.anim_params.loop_count = i32::from(animation.loop_count);
            animation_config.minimize_size = i32::from(animation.minimize_size);
            animation_config.kmin = animation.kmin;
            animation_config.kmax = animation.kmax;
            animation_config.allow_mixed = i32::from(animation.allow_mixed);

            let raw = ffi::WebPAnimEncoderNewInternal(
                width,
                height,
                &animation_config,
                ffi::WEBP_MUX_ABI_VERSION as i32,
            );
            if raw.is_null() {
                return Err(webp_error("could not create animation encoder"));
            }

            let mut config = mem::zeroed();
            if ffi::WebPConfigInitInternal(
                &mut config,
                ffi::WebPPreset::WEBP_PRESET_DEFAULT,
                75.0,
                ffi::WEBP_ENCODER_ABI_VERSION as i32,
            ) == 0
            {
                ffi::WebPAnimEncoderDelete(raw);
                return Err(webp_error("could not initialize frame options"));
            }
            config.lossless = i32::from(options.lossless);
            config.quality = options.quality;
            config.method = options.method as i32;
            config.exact = i32::from(options.lossless);
            if ffi::WebPValidateConfig(&config) == 0 {
                ffi::WebPAnimEncoderDelete(raw);
                return Err(webp_error("libwebp rejected the frame options"));
            }

            let mut picture = mem::zeroed();
            if ffi::WebPPictureInitInternal(&mut picture, ffi::WEBP_ENCODER_ABI_VERSION as i32) == 0
            {
                ffi::WebPAnimEncoderDelete(raw);
                return Err(webp_error("could not initialize a frame"));
            }
            picture.width = width;
            picture.height = height;
            picture.use_argb = 1;

            Ok(Self {
                raw,
                picture,
                config,
                expected_frame_bytes,
                stride: width * 4,
                previous_timestamp: None,
                failed: false,
            })
        }
    }

    /// Encode one tightly packed RGBA frame at an increasing timestamp.
    pub fn add_frame(&mut self, rgba: &[u8], timestamp_ms: i32) -> Result<()> {
        if self.failed {
            return Err(webp_error("encoder cannot be reused after a native failure"));
        }
        if rgba.len() != self.expected_frame_bytes {
            return Err(Error::WebpEncode(format!(
                "RGBA frame has {} bytes; expected {}",
                rgba.len(),
                self.expected_frame_bytes
            )));
        }
        if let Some(previous) = self.previous_timestamp {
            if validate_frame_duration(previous, timestamp_ms)? == 0 {
                return Err(webp_error("frame timestamps must be strictly increasing"));
            }
        }

        // SAFETY: rgba has the validated canvas length and remains alive for
        // both calls. libwebp imports it into picture-owned memory before the
        // animation encoder consumes the picture synchronously.
        if unsafe { ffi::WebPPictureImportRGBA(&mut self.picture, rgba.as_ptr(), self.stride) } == 0
        {
            self.failed = true;
            return Err(webp_error("could not import an RGBA frame"));
        }
        if unsafe {
            ffi::WebPAnimEncoderAdd(self.raw, &mut self.picture, timestamp_ms, &self.config)
        } == 0
        {
            let error = self.encoder_error("could not encode frame");
            self.failed = true;
            return Err(Error::WebpEncode(error));
        }
        self.previous_timestamp = Some(timestamp_ms);
        Ok(())
    }

    /// Finalize the timeline and return the complete WebP file.
    pub fn finish(self, final_timestamp_ms: i32) -> Result<Vec<u8>> {
        if self.failed {
            return Err(webp_error("encoder cannot be finalized after a native failure"));
        }
        let Some(previous_timestamp) = self.previous_timestamp else {
            return Err(webp_error("no frames were added"));
        };
        validate_frame_duration(previous_timestamp, final_timestamp_ms)?;

        // SAFETY: self.raw is live and the null frame is libwebp's documented
        // end-of-timeline sentinel.
        if unsafe {
            ffi::WebPAnimEncoderAdd(self.raw, ptr::null_mut(), final_timestamp_ms, ptr::null())
        } == 0
        {
            return Err(Error::WebpEncode(self.encoder_error("could not finalize timeline")));
        }

        // SAFETY: WebPData is initialized before assembly. Its storage is
        // copied into Rust ownership and cleared exactly once on every path.
        let mut data = unsafe {
            let mut data = mem::zeroed();
            ffi::WebPDataInit(&mut data);
            data
        };
        if unsafe { ffi::WebPAnimEncoderAssemble(self.raw, &mut data) } == 0 {
            let error = self.encoder_error("could not assemble animation");
            unsafe { ffi::WebPDataClear(&mut data) };
            return Err(Error::WebpEncode(error));
        }
        let output = if data.bytes.is_null() {
            Vec::new()
        } else {
            unsafe { slice::from_raw_parts(data.bytes, data.size) }.to_vec()
        };
        unsafe { ffi::WebPDataClear(&mut data) };
        if output.is_empty() {
            return Err(webp_error("encoder returned an empty file"));
        }

        Ok(output)
    }

    fn encoder_error(&self, fallback: &str) -> String {
        let message = unsafe { ffi::WebPAnimEncoderGetError(self.raw) };
        if message.is_null() {
            fallback.to_string()
        } else {
            unsafe { CStr::from_ptr(message) }.to_string_lossy().into_owned()
        }
    }
}

impl Drop for WebpEncoder {
    fn drop(&mut self) {
        // SAFETY: both values were initialized in the constructor and are
        // owned exclusively by this encoder.
        unsafe {
            ffi::WebPPictureFree(&mut self.picture);
            ffi::WebPAnimEncoderDelete(self.raw);
        }
    }
}

pub struct WebpAnim {
    renders: Renders,
    period: f64,
    hold: f64,
    first_frame: i32,
    options: WebpOptions,
}

impl WebpAnim {
    pub fn new(renders: Renders) -> Self {
        Self { renders, period: 0.1, hold: 5.0, first_frame: -1, options: WebpOptions::default() }
    }

    pub fn set_first_frame(&mut self, index: i32) {
        self.first_frame = index;
    }

    pub fn set_timing(&mut self, period: f64, hold: f64) {
        self.period = period;
        self.hold = hold;
    }

    pub fn set_options(&mut self, options: WebpOptions) {
        self.options = options;
    }

    /// Encode one RGBA image as a still WebP file.
    pub fn encode_single(img: &image::RgbaImage, options: &WebpOptions) -> Result<Vec<u8>> {
        let mut encoder = WebpEncoder::new(img.dimensions(), *options)?;
        encoder.add_frame(img.as_raw(), 0)?;
        encoder.finish(1)
    }

    /// Render and encode every template frame with bounded memory.
    pub fn encode(&mut self) -> Result<Vec<u8>> {
        let frame_count = i32::try_from(self.renders.length())
            .map_err(|_| webp_error("frame count exceeds WebP's timestamp range"))?;
        if frame_count == 0 {
            return Err(webp_error("no frames to encode"));
        }
        let (timeline, final_timestamp) =
            animation_timeline(self.period, self.hold, frame_count, self.first_frame)?;

        let dimensions = self.renders.get_render(0)?.get().dimensions();
        let mut encoder = WebpEncoder::new_with_animation_options(
            dimensions,
            self.options,
            WebpAnimationOptions { kmin: 3, kmax: 5, ..Default::default() },
        )?;
        for (index, timestamp) in timeline {
            encoder.add_frame(self.renders.get_render(index)?.get().as_raw(), timestamp)?;
            self.renders.remove_render(index);
        }

        encoder.finish(final_timestamp)
    }

    /// Encode all frames and write the result to `path`.
    pub fn save<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let data = self.encode()?;
        let mut writer = std::io::BufWriter::new(File::create(path)?);
        writer.write_all(&data)?;
        Ok(())
    }
}

fn validate_timing(period: f64, hold: f64, frame_count: i32) -> Result<()> {
    if !period.is_finite() || !hold.is_finite() {
        return Err(webp_error("animation timing must be finite"));
    }
    if period < 0.0 || hold < 0.0 {
        return Err(webp_error("animation timing must not be negative"));
    }

    let total_ms = (period * f64::from(frame_count) + hold) * 1000.0;
    let timestamp_upper_bound = total_ms.round() + f64::from(frame_count);
    if !timestamp_upper_bound.is_finite() || timestamp_upper_bound > f64::from(i32::MAX) {
        return Err(webp_error("animation timing exceeds WebP's timestamp range"));
    }
    Ok(())
}

fn animation_timeline(
    period: f64,
    hold: f64,
    frame_count: i32,
    first_frame: i32,
) -> Result<(Vec<(i32, i32)>, i32)> {
    validate_timing(period, hold, frame_count)?;
    let offset = if first_frame >= 0 { first_frame % frame_count } else { 0 };
    let mut timeline = Vec::with_capacity(frame_count as usize);
    let mut current_ms = 0.0f64;
    let mut previous_timestamp = -1;

    for base in 0..frame_count {
        let index = ((i64::from(base) + i64::from(offset)) % i64::from(frame_count)) as i32;
        let timestamp = rounded_timestamp_ms(current_ms, previous_timestamp)?;
        if previous_timestamp >= 0 {
            validate_frame_duration(previous_timestamp, timestamp)?;
        }
        timeline.push((index, timestamp));
        let step = if index == frame_count - 1 { period + hold } else { period };
        previous_timestamp = timestamp;
        current_ms += step * 1000.0;
    }

    let final_timestamp = rounded_timestamp_ms(current_ms, previous_timestamp)?;
    validate_frame_duration(previous_timestamp, final_timestamp)?;
    Ok((timeline, final_timestamp))
}

fn rounded_timestamp_ms(current_ms: f64, previous_timestamp: i32) -> Result<i32> {
    let rounded = current_ms.round();
    if !rounded.is_finite() || rounded < 0.0 || rounded > f64::from(i32::MAX) {
        return Err(webp_error("animation timing exceeds WebP's timestamp range"));
    }

    let timestamp = rounded as i32;
    if timestamp <= previous_timestamp {
        previous_timestamp
            .checked_add(1)
            .ok_or_else(|| webp_error("animation timing exceeds WebP's timestamp range"))
    } else {
        Ok(timestamp)
    }
}

fn validate_frame_duration(timestamp_ms: i32, next_timestamp_ms: i32) -> Result<i64> {
    let duration = i64::from(next_timestamp_ms) - i64::from(timestamp_ms);
    if duration < 0 {
        Err(webp_error("frame timestamps must be non-decreasing"))
    } else if duration > MAX_WEBP_FRAME_DURATION_MS {
        Err(webp_error("frame duration exceeds WebP's limit"))
    } else {
        Ok(duration)
    }
}

fn frame_bytes((width, height): (u32, u32)) -> Result<usize> {
    if width == 0 || height == 0 {
        return Err(webp_error("dimensions must be positive"));
    }
    if width > MAX_WEBP_DIMENSION || height > MAX_WEBP_DIMENSION {
        return Err(Error::WebpEncode(format!(
            "dimensions exceed WebP's {MAX_WEBP_DIMENSION}px limit"
        )));
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| webp_error("RGBA frame size overflowed"))
}

fn normalize_options(mut options: WebpOptions) -> Result<WebpOptions> {
    if !options.quality.is_finite() {
        return Err(webp_error("quality must be a finite number"));
    }
    options.quality = options.quality.clamp(0.0, 100.0);
    options.method = options.method.min(6);
    Ok(options)
}

fn validate_animation_options(options: WebpAnimationOptions) -> Result<()> {
    let valid_keyframes = options.kmax <= 0
        || options.kmax == 1
        || (options.kmin > options.kmax / 2 && options.kmin < options.kmax);
    if !valid_keyframes {
        return Err(webp_error("invalid keyframe interval"));
    }
    Ok(())
}

fn webp_error(message: &str) -> Error {
    Error::WebpEncode(message.to_string())
}

#[cfg(test)]
mod tests {
    #[cfg(not(miri))]
    use super::WebpEncoder;
    use super::{
        FramePlan, FrameRect, WebpAnimationOptions, WebpFrame, WebpOptions, animation_timeline,
        encode_webp_animation, frame_bytes, normalize_options, parallel_chunk_len_for,
        rounded_timestamp_ms, validate_animation_options, validate_timing,
    };

    #[test]
    fn validates_dimensions() {
        assert!(frame_bytes((0, 1)).is_err());
        assert!(frame_bytes((16_384, 1)).is_err());
    }

    #[test]
    fn validates_batch_inputs_before_ffi() {
        let options = WebpOptions::default();
        assert!(encode_webp_animation((1, 1), &[], 0, options, 0).is_err());

        let short = [0; 3];
        let malformed = [WebpFrame::new(&short, 0)];
        assert!(encode_webp_animation((1, 1), &malformed, 1, options, 0).is_err());

        let pixel = [0; 4];
        let duplicate_timestamps = [WebpFrame::new(&pixel, 0), WebpFrame::new(&pixel, 0)];
        assert!(encode_webp_animation((1, 1), &duplicate_timestamps, 1, options, 0).is_err());
    }

    #[test]
    fn limits_large_frame_parallelism() {
        let rgba = [0; 4];
        let small = FramePlan { rgba: &rgba, rectangle: FrameRect::full(640, 360), duration_ms: 1 };
        let large =
            FramePlan { rgba: &rgba, rectangle: FrameRect::full(8_192, 8_192), duration_ms: 1 };

        assert_eq!(parallel_chunk_len_for(&[small; 32], 24), 24);
        assert_eq!(parallel_chunk_len_for(&[large; 2], 24), 1);
        let mut full_then_dirty = vec![large];
        full_then_dirty.extend([small; 24]);
        assert_eq!(parallel_chunk_len_for(&full_then_dirty, 24), 1);
        assert_eq!(parallel_chunk_len_for(&full_then_dirty[1..], 24), 24);
    }

    #[cfg(not(miri))]
    #[test]
    fn validates_frame_lengths_before_import() {
        let mut encoder = WebpEncoder::new((2, 2), WebpOptions::default()).unwrap();
        assert!(encoder.add_frame(&[0; 15], 0).is_err());
    }

    #[cfg(not(miri))]
    #[test]
    fn validates_timestamps_without_entering_ffi() {
        let frame = [0; 16];
        let mut encoder = WebpEncoder::new((2, 2), WebpOptions::default()).unwrap();
        encoder.add_frame(&frame, 0).unwrap();
        assert!(encoder.add_frame(&frame, 0).is_err());
    }

    #[test]
    fn normalizes_public_options() {
        let options =
            normalize_options(WebpOptions { quality: 120.0, lossless: false, method: 20 }).unwrap();
        assert_eq!(options.quality, 100.0);
        assert_eq!(options.method, 6);
        assert!(
            normalize_options(WebpOptions { quality: f32::NAN, ..Default::default() }).is_err()
        );
    }

    #[test]
    fn validates_animation_timing() {
        assert!(validate_timing(0.1, 5.0, 10).is_ok());
        assert!(validate_timing(f64::NAN, 0.0, 1).is_err());
        assert!(validate_timing(0.1, -1.0, 1).is_err());
        assert!(validate_timing(f64::MAX, 0.0, 1).is_err());
        assert!(validate_timing(f64::from(i32::MAX) / 1000.0, 0.0, 1).is_err());
        assert_eq!(rounded_timestamp_ms(0.1, 0).unwrap(), 1);
        assert!(rounded_timestamp_ms(f64::from(i32::MAX), i32::MAX).is_err());

        let (timeline, final_timestamp) =
            animation_timeline(0.1, 0.0, 3, i32::MAX).expect("large rotation is normalized");
        assert_eq!(timeline, vec![(1, 0), (2, 100), (0, 200)]);
        assert_eq!(final_timestamp, 300);
        assert!(
            animation_timeline(0.1, (super::MAX_WEBP_FRAME_DURATION_MS + 1) as f64 / 1000.0, 2, 0)
                .is_err()
        );
    }

    #[test]
    fn validates_keyframe_intervals() {
        assert!(validate_animation_options(WebpAnimationOptions::default()).is_ok());
        assert!(
            validate_animation_options(WebpAnimationOptions {
                kmin: 1,
                kmax: 5,
                ..Default::default()
            })
            .is_err()
        );
    }
}
