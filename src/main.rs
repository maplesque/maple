use std::{
    fs,
    path::{Path, PathBuf},
};

#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use clap::{ArgGroup, Parser};
use color_eyre::eyre::{Result, WrapErr, eyre};
use image::Rgba;
use maple_render_core::{
    gif_anim::GifAnim,
    input::{Input, Inputs, TextOptions},
    render::{Render, RenderQuality},
    renders::Renders,
    repository::Repository,
    vid_anim::VidAnim,
    webp_anim::{WebpAnim, WebpOptions},
};
use serde::Deserialize;

#[derive(Parser, Debug)]
#[command(name = "maple")]
#[command(about = "Put pictures into animations")]
#[command(arg_required_else_help(true))]
#[command(group(ArgGroup::new("input_source").required(true).args(["zip", "json"])))]
#[command(after_help = "\
Examples:
  maple --zip foo.zip --in layer1.png layer2.png
  maple --zip foo.zip --in layer1.png layer2.png +layer2_more.png
  maple --json setup.json
  maple --zip foo.zip --in layer1.png --w 200 --h 150
  maple --zip foo.zip --in layer1.png --first 2
  maple --zip foo.zip --in layer1.png --last 4
  maple --zip foo.zip --in layer1.png --single
  maple --zip foo.zip --in layer1.png --auto_zoom
  maple --zip foo.zip --in layer1.png --start 10
  maple --zip foo.zip --in layer1.png --save \"frame_%06d.jpg\"
  maple --zip foo.zip --in layer1.png --gif example.gif
  maple --zip foo.zip --in layer1.png --webp out.webp
  maple --zip foo.zip --in layer1.png --webp out.webp --webp-quality 90
  maple --zip foo.zip --in layer1.png --webp out.webp --webp-lossless
  maple --zip foo.zip --in layer1.png --webp-single frame.webp
  maple --zip foo.zip --in layer1.png --stats
  maple --zip foo.zip --in layer1.png --files
")]
struct Cli {
    /// Template zip file
    #[arg(long)]
    zip: Option<PathBuf>,

    /// JSON configuration file (alternative to --zip)
    #[arg(long)]
    json: Option<PathBuf>,

    /// Input images (space-separated, prefix with + for same layer as previous)
    #[arg(long = "in", num_args = 1..)]
    inputs: Vec<String>,

    /// Output GIF path
    #[arg(long)]
    gif: Option<PathBuf>,

    /// Output video path
    #[arg(long)]
    vid: Option<PathBuf>,

    /// Output animated WebP path (native builds only; no banding, full color)
    #[arg(long)]
    webp: Option<PathBuf>,

    /// WebP lossy quality 0..=100 (default 95; only for --webp)
    #[arg(long, default_value_t = 95.0)]
    webp_quality: f32,

    /// WebP lossy method 0..=6 (default 4; 0=fastest. only for --webp)
    #[arg(long, value_parser = clap::value_parser!(u8).range(..=6))]
    webp_method: Option<u8>,

    /// Use lossless WebP encoding (only for --webp)
    #[arg(long)]
    webp_lossless: bool,

    /// Save individual frames (printf format, e.g., "frame_%06d.jpg")
    #[arg(long)]
    save: Option<String>,

    /// Save a single frame as a WebP image (lossy, default quality 95)
    #[arg(long)]
    webp_single: Option<PathBuf>,

    /// Output width
    #[arg(long, short = 'w')]
    width: Option<u32>,

    /// Output height
    #[arg(long, short = 'H')]
    height: Option<u32>,

    /// First frame index
    #[arg(long)]
    first: Option<i32>,

    /// Last frame index
    #[arg(long)]
    last: Option<i32>,

    /// Render single frame only
    #[arg(long)]
    single: bool,

    /// Auto-zoom to fit content
    #[arg(long)]
    auto_zoom: bool,

    /// Starting frame offset (rotation)
    #[arg(long)]
    start: Option<i32>,

    /// Scan for optimal positioning
    #[arg(long)]
    scan: bool,

    /// Print statistics
    #[arg(long)]
    stats: bool,

    /// Print file information
    #[arg(long)]
    files: bool,

    /// Font size for text inputs (default: 72)
    #[arg(long)]
    font_size: Option<f32>,

    /// Text color as hex (e.g., "000000" for black, "FF0000" for red)
    #[arg(long)]
    text_color: Option<String>,

    /// Background color for text inputs as hex (default: "FFFFFF" white)
    #[arg(long)]
    text_bg: Option<String>,

    /// Enable Floyd-Steinberg dithering during GIF quantization (slower, can improve gradients)
    #[arg(long)]
    dither: bool,
}

#[derive(Deserialize)]
struct JsonConfig {
    inputs: Vec<JsonInput>,
}

#[derive(Deserialize)]
struct JsonInput {
    filename: String,
    layer: i32,
    #[serde(default = "default_scale")]
    xs: f64,
    #[serde(default = "default_scale")]
    ys: f64,
    #[serde(default)]
    xo: f64,
    #[serde(default)]
    yo: f64,
    #[serde(default)]
    in_scale: i32,
    #[serde(default)]
    in_x0: f64,
    #[serde(default)]
    in_y0: f64,
    #[serde(default = "default_xa")]
    in_xa: f64,
    #[serde(default)]
    in_ya: f64,
}

const DEFAULT_FONT: &[u8] = include_bytes!("../fonts/DejaVuSans-Bold.ttf");

fn default_scale() -> f64 {
    1.0
}
fn default_xa() -> f64 {
    1.0
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    let mut inputs = Inputs::new();

    if let Some(json_path) = &cli.json {
        let json_text = fs::read_to_string(json_path)
            .wrap_err_with(|| format!("Failed to read JSON file: {}", json_path.display()))?;
        let config: JsonConfig =
            serde_json::from_str(&json_text).wrap_err("Failed to parse JSON configuration")?;

        for jin in config.inputs {
            let mut input = Input::load(&jin.filename)
                .wrap_err_with(|| format!("Failed to load input: {}", jin.filename))?;
            input.layer = jin.layer as u8;
            input.xs = jin.xs;
            input.ys = jin.ys;
            input.xo = jin.xo;
            input.yo = jin.yo;
            if jin.in_scale != 0 {
                input.in_scale = jin.in_scale;
            }
            input.in_x0 = jin.in_x0;
            input.in_y0 = jin.in_y0;
            input.xa = jin.in_xa;
            input.ya = jin.in_ya;
            inputs.push(input);
        }
    }

    let zip_path = cli.zip.as_ref().ok_or_else(|| eyre!("No zip file specified"))?;
    let repo = Repository::load(zip_path)
        .wrap_err_with(|| format!("Failed to load template: {}", zip_path.display()))?;

    let text_options = TextOptions {
        font_size: cli.font_size.unwrap_or(72.0),
        color: parse_hex_color(&cli.text_color).unwrap_or(Rgba([0, 0, 0, 255])),
        background: parse_hex_color(&cli.text_bg).unwrap_or(Rgba([255, 255, 255, 255])),
        ..Default::default()
    };

    let mut layer = 1u8;
    for inp_str in &cli.inputs {
        let (path, same_layer) = if inp_str.starts_with('+') && layer > 1 {
            (inp_str[1..].to_string(), true)
        } else {
            (inp_str.clone(), false)
        };

        if same_layer {
            layer -= 1;
        }

        let mut input = if let Some(text) = path.strip_prefix("text:") {
            eprintln!("Rendering text: \"{}\"", text);
            Input::from_text(text, layer, &text_options, DEFAULT_FONT)
                .wrap_err_with(|| format!("Failed to render text: {}", text))?
        } else if Path::new(&path).exists() {
            Input::load(&path).wrap_err_with(|| format!("Failed to load input: {}", path))?
        } else {
            eprintln!("File '{}' not found, treating as text input", path);
            Input::from_text(&path, layer, &text_options, DEFAULT_FONT)
                .wrap_err_with(|| format!("Failed to render text: {}", path))?
        };

        input.layer = layer;
        inputs.push(input);
        layer += 1;
    }

    let mut renders = Renders::new(repo, inputs, RenderQuality::Sampled);

    if let (Some(w), Some(h)) = (cli.width, cli.height) {
        renders.set_size(w as i32, h as i32);
    }

    let first = cli.first.unwrap_or(0);
    let last = cli.last.unwrap_or(renders.length() as i32 - 1);
    let (first, last) = if cli.single { (0, 0) } else { (first, last) };

    if cli.auto_zoom {
        renders.auto_zoom()?;
    }

    if cli.scan {
        let frame = cli.start.unwrap_or(-1);
        renders.scan(frame)?;
    }

    if let Some(save_pattern) = &cli.save {
        for i in first..=last {
            let render = renders.get_render(i)?;
            let filename = format_frame_path(save_pattern, i);
            render
                .save(&filename)
                .wrap_err_with(|| format!("Failed to save frame to {}", filename))?;
            eprintln!("Wrote to {}", filename);
        }
    }

    if let Some(webp_path) = &cli.webp_single {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let render = renders.get_render(first)?;
            let img = render.get().clone();

            let mut options = WebpOptions {
                quality: cli.webp_quality.clamp(0.0, 100.0),
                lossless: cli.webp_lossless,
                ..WebpOptions::default()
            };
            if let Some(method) = cli.webp_method {
                options.method = usize::from(method);
            }

            let data = WebpAnim::encode_single(&img, &options)
                .map_err(|e| eyre!("Failed to encode single WebP frame: {}", e))?;

            std::fs::write(webp_path, &data).wrap_err_with(|| {
                format!("Failed to save WebP frame to {}", webp_path.display())
            })?;
            eprintln!("Saved single WebP frame to {}", webp_path.display());

            // If the only requested output was the single frame, we are done.
            if cli.gif.is_none() && cli.vid.is_none() && cli.webp.is_none() {
                return Ok(());
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            return Err(eyre!("WebP output is not available in the WASM build"));
        }
    }

    if let Some(gif_path) = &cli.gif {
        let mut anim = GifAnim::new(renders);

        let repo_ref = Repository::load(zip_path)?;
        anim.set_palette_frames(repo_ref.get_palette());
        anim.set_timing(repo_ref.get_period(), repo_ref.get_hold());
        anim.set_dither(cli.dither);

        if let Some(start) = cli.start {
            anim.set_first_frame(start);
        }

        anim.apply().wrap_err("Failed to generate animation frames")?;
        anim.save(gif_path)
            .wrap_err_with(|| format!("Failed to save GIF to {}", gif_path.display()))?;

        eprintln!("Saved GIF to {}", gif_path.display());
        return Ok(());
    }

    if let Some(webp_path) = &cli.webp {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let repo_ref = Repository::load(zip_path)
                .wrap_err_with(|| format!("Failed to reload template: {}", zip_path.display()))?;
            let mut anim = WebpAnim::new(renders);
            anim.set_timing(repo_ref.get_period(), repo_ref.get_hold());

            if let Some(start) = cli.start {
                anim.set_first_frame(start);
            }

            let mut options = WebpOptions {
                quality: cli.webp_quality.clamp(0.0, 100.0),
                lossless: cli.webp_lossless,
                ..WebpOptions::default()
            };
            if let Some(method) = cli.webp_method {
                options.method = usize::from(method);
            }
            anim.set_options(options);

            anim.save(webp_path)
                .wrap_err_with(|| format!("Failed to save WebP to {}", webp_path.display()))?;

            eprintln!("Saved WebP to {}", webp_path.display());
            return Ok(());
        }
        #[cfg(target_arch = "wasm32")]
        {
            return Err(eyre!("WebP output is not available in the WASM build"));
        }
    }

    if let Some(vid_path) = &cli.vid {
        let repo_ref = Repository::load(zip_path)?;
        let mut anim = VidAnim::new(renders);
        anim.set_timing(repo_ref.get_period(), repo_ref.get_hold());

        if let Some(start) = cli.start {
            anim.set_first_frame(start);
        }

        anim.apply(vid_path)
            .wrap_err_with(|| format!("Failed to save video to {}", vid_path.display()))?;

        eprintln!("Saved video to {}", vid_path.display());
        return Ok(());
    }

    if cli.files {
        let repo = Repository::load(zip_path)?;
        println!("animation={}", repo.is_animation());
        println!("length={}", repo.length());
        println!("period={}", repo.get_period());
        println!("hold={}", repo.get_hold());
    }

    if cli.stats {
        eprintln!("Peak number of mappings cached: {}", renders.repo().peak());
        eprintln!("Peak number of renders cached: {}", renders.peak());
        eprintln!("Total number of frames: {}", renders.length());
        eprintln!("Total number of renders: {}", Render::render_count());

        let repo = Repository::load(zip_path)?;
        let palette = repo.get_palette();
        eprint!("Palette frame(s):");
        for p in palette {
            eprint!(" {}", p);
        }
        eprintln!();
    }

    Ok(())
}

/// Format frame path with frame number
fn format_frame_path(pattern: &str, frame: i32) -> String {
    let mut result = pattern.to_string();

    // Find %...d pattern and replace with frame number
    if let Some(start) = result.find('%') {
        if let Some(end) = result[start..].find('d') {
            let spec = &result[start..start + end + 1];
            let formatted = if spec.contains('0') {
                // Extract width
                let width: usize = spec[1..end].trim_start_matches('0').parse().unwrap_or(1);
                format!("{:0width$}", frame, width = width)
            } else {
                format!("{}", frame)
            };
            result = result.replace(spec, &formatted);
        }
    }

    result
}

fn parse_hex_color(hex: &Option<String>) -> Option<Rgba<u8>> {
    hex.as_ref().and_then(|s| {
        let s = s.trim_start_matches('#');
        if s.len() == 6 {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            Some(Rgba([r, g, b, 255]))
        } else {
            None
        }
    })
}
