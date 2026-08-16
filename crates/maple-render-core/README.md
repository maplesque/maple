# maple-render-core

maple's renderer and compositing core.

This crate contains:

- Template repository loading (`Repository`)
- Frame compositing and mapping (`Render`, `Renders`)
- GIF/video helpers (`GifAnim`, `VidAnim`)
- Quantization pipeline
- Input handling for images and text

It intentionally does **not** bundle templates or font assets.
For text rendering, you must pass font bytes into `Input::from_text`.

## Usage

The documentation in http://docs.rs/maple-render-core is your best bet for most of your usage of this crate.

You generally want to work with the respective `[Format]Anim` structs like `GifAnim` or `WebpAnim` for rendering.

Start by using [`Repository`](https://docs.rs/maple-render-core/latest/maple_render_core/repository/struct.Repository.html)'s either of `load` which can take a filesystem path or `load_from_bytes` which can take a `Vec<u8>` and is generally recommended to do so, as it avoids the need to read from the filesystem. You can just copy the templates and fonts from this repo's `templates` directory into your project, and `include_bytes!` each template into your code, like so, assuming you keep them in your project's `assets/templates` directory and `assets/fonts` directory:

```rust
const BACK_TATTOO_TEMPLATE: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/templates/back-tattoo.zip"));
const BILLBOARD_CITYSCAPE_TEMPLATE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/assets/templates/billboard-cityscape.zip"
));
const BOOK_TEMPLATE: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/templates/book.zip"));
const CIRCUITBOARD_TEMPLATE: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/templates/circuitboard.zip"));
const FLAG_TEMPLATE: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/templates/flag.zip"));
const FLAG2_TEMPLATE: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/templates/flag2.zip"));
const FORTUNE_COOKIE_TEMPLATE: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/templates/fortune-cookie.zip"));
const HEART_LOCKET_TEMPLATE: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/templates/heart-locket.zip"));
const RUBIKS_TEMPLATE: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/templates/rubiks.zip"));
const TOASTER_TEMPLATE: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/templates/toaster.zip"));
const VALENTINE_TEMPLATE: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/templates/valentine.zip"));

fn template_bytes(template: &str) -> Option<&'static [u8]> {
    match template {
        "back-tattoo.zip" => Some(BACK_TATTOO_TEMPLATE),
        "billboard-cityscape.zip" => Some(BILLBOARD_CITYSCAPE_TEMPLATE),
        "book.zip" => Some(BOOK_TEMPLATE),
        "circuitboard.zip" => Some(CIRCUITBOARD_TEMPLATE),
        "flag.zip" => Some(FLAG_TEMPLATE),
        "flag2.zip" => Some(FLAG2_TEMPLATE),
        "fortune-cookie.zip" => Some(FORTUNE_COOKIE_TEMPLATE),
        "heart-locket.zip" => Some(HEART_LOCKET_TEMPLATE),
        "rubiks.zip" => Some(RUBIKS_TEMPLATE),
        "toaster.zip" => Some(TOASTER_TEMPLATE),
        "valentine.zip" => Some(VALENTINE_TEMPLATE),
        _ => None,
    }
}

const FONT: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts/DejaVuSans-Bold.ttf"));
```

### Inputs

An [`Input`](https://docs.rs/maple-render-core/latest/maple_render_core/input/struct.Input.html) is a single image or piece of text composited onto one template layer. Layers map to different surfaces in the template (the screen, the book cover, the tattoo, ...), and they start at 1:

```rust
use maple_render_core::{Input, Inputs};

let mut inputs = Inputs::new();

inputs.push(Input::load("front.png")?); // layer 1
inputs.push(Input::load("back.png")?); // layer 2

// Two images stacked on the same layer? Just set it
let mut stamp = Input::load("stamp.png")?;
stamp.layer = 2;
inputs.push(stamp);
```

Text is an input too, and it needs a font, which is where that `FONT` const up there comes in. Loading the font bytes are on you either way:

```rust
use maple_render_core::TextOptions;

let sale = Input::from_text(
    "SALE", 3, &TextOptions { font_size: 96.0, ..Default::default() }, FONT,
)?;
inputs.push(sale);
```

`TextOptions::default()` is 72px black text on a white background with 40px of padding. `color` and `background` are `image::Rgba<u8>` values (so you'll want the `image` crate installed), and `padding` grows the canvas around the text.

### Rendering

Everything goes through [`Renders`](https://docs.rs/maple-render-core/latest/maple_render_core/renders/struct.Renders.html) struct, which uses the repository plus your inputs and renders frames on demand:

```rust
use maple_render_core::{Renders, render::RenderQuality};

let mut renders = Renders::new(repo, inputs, RenderQuality::Sampled);

renders.set_size(400, 300); // optional; content is fit inside, aspect kept

let frame = renders.get_render(0)?;
frame.save("frame0.png")?;
```

[`RenderQuality`](https://docs.rs/maple-render-core/latest/maple_render_core/render/enum.RenderQuality.html) is `Sampled` (the default, 9-tap antialiased sampling), `Simple` (fast, nearest-sample), or `None` (just the template's neutral frame). Generally, just use `Sampled` unless you're benchmarking.

Good to know: `renders.auto_zoom()` scales inputs up to fill the mapped area (returns `true` if anything changed), and `renders.length()` tells you how many frames the animation has.

### GIF

[`GifAnim`](https://docs.rs/maple-render-core/latest/maple_render_core/gif_anim/struct.GifAnim.html) turns a `Renders` into an animated GIF. GIFs only have 256 colors, so the frames get quantized against a palette. By default the palette frames and per-frame timing come from the template itself (`Repository::get_palette()`, `get_period()`, `get_hold()`); call `set_palette_frames`/`set_timing` only when you want to override them:

```rust
use maple_render_core::GifAnim;

let mut gif = GifAnim::new(renders);
// The template's own palette and timing apply automatically; these just
// override them:
gif.set_palette_frames(vec![0]); // quantize against frame 0
gif.set_timing(0.1, 5.0); // seconds per frame, extra hold on the last
gif.set_dither(true); // slower but smoother gradients
gif.apply()?;
// save to filesystem
gif.save("out.gif")?;
// or grab the bytes (`Vec<u8>`) instead:
let data = gif.encode()?;
```

The template's own timings are available on the repository too via `repo.get_period()` and `repo.get_hold()`.

### WebP

[`WebpAnim`](https://docs.rs/maple-render-core/latest/maple_render_core/webp_anim/struct.WebpAnim.html) is the same idea with full color and no banding. It's backed by `libwebp` and defaults to lossy quality 95 with method 4:

```rust
use maple_render_core::{WebpAnim, WebpOptions};

let mut webp = WebpAnim::new(renders);
webp.set_timing(0.1, 5.0);
webp.set_options(WebpOptions { quality: 90.0, lossless: false, method: 4 });
webp.save("out.webp")?;
```

Need just one still image? `WebpAnim::encode_single(&image, &options)`. If you want the finer-grained knobs, streaming frames one at a time with `WebpEncoder`, or handing `encode_webp_animation` a pile of RGBA frames you already have in memory and letting it encode them in parallel, that's all in the [crate docs for WebpAnim](https://docs.rs/maple-render-core/latest/maple_render_core/webp_anim/index.html). WebP is native-only and gated behind the default `webp` feature.

### Video

[`VidAnim`](https://docs.rs/maple-render-core/latest/maple_render_core/vid_anim/struct.VidAnim.html) renders every frame and pipes them into ffmpeg at 30fps (H.264), so ffmpeg has to be on your PATH:

```rust
use maple_render_core::VidAnim;

let mut vid = VidAnim::new(renders);
vid.set_timing(0.1, 5.0);
vid.set_first_frame(3); // start the loop partway through
vid.apply("out.mp4")?;
```

### Putting it together

```rust
use maple_render_core::{
    GifAnim, Input, Inputs, Renders, Repository, TextOptions, render::RenderQuality,
};

// Template code from above...

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo =
        Repository::load_from_bytes(template_bytes("toaster.zip").expect("template").to_vec())?;

    let mut inputs = Inputs::new();
    inputs.push(Input::load("monkey.jpg")?);
    inputs.push(Input::from_text("Helloooooooo", 2, &TextOptions::default(), FONT)?);

    let mut renders = Renders::new(repo, inputs, RenderQuality::Sampled);
    renders.auto_zoom()?;

    let mut gif = GifAnim::new(renders);
    gif.set_timing(0.1, 5.0);
    gif.apply()?;
    gif.save("out.gif")?;

    Ok(())
}
```

## Animated WebP

`WebpAnim` and `WebpEncoder` use libwebp's animation encoder sequentially. This is the default path because it keeps memory bounded and performs animation-wide compression.

`encode_webp_animation` is an opt-in batch API for callers that already hold complete RGBA frames. It detects dirty rectangles and encodes them concurrently with rayon. This can reduce encoding time by roughly an order of magnitude on multicore machines, but it retains all source frames and can produce materially larger files because each rectangle is compressed independently. You could benchmark both output size and latency for your usecase before selecting it.

The native `libwebp-sys` dependency is pinned exactly at 0.14.4. Its bundled libwebp 1.6.0 synchronizes lazy DSP initialization on Unix; maple performs a one-time serialized warm-up on Windows, where that release uses an unsynchronized fallback. Upgrading the binding requires re-reviewing that initialization work.

WebP APIs are native-only and are excluded from `wasm32` builds.
