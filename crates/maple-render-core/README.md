# maple-render-core

Core rendering engine extracted from maple.

This crate contains:

- Template repository loading (`Repository`)
- Frame compositing and mapping (`Render`, `Renders`)
- GIF/video helpers (`GifAnim`, `VidAnim`)
- Quantization pipeline
- Input handling for images and text

It intentionally does **not** bundle templates or font assets.
For text rendering, callers must pass font bytes into `Input::from_text`.

## Animated WebP

`WebpAnim` and `WebpEncoder` use libwebp's animation encoder sequentially. This is the default path because it keeps memory bounded and performs animation-wide compression.

`encode_webp_animation` is an opt-in batch API for callers that already hold complete RGBA frames. It detects dirty rectangles and encodes them concurrently with Rayon. This can reduce encoding time by roughly an order of magnitude on multicore machines, but it retains all source frames and can produce materially larger files because each rectangle is compressed independently. Benchmark both output size and latency for your workload before selecting it.

The native `libwebp-sys` dependency is pinned exactly at 0.14.4. Its bundled libwebp 1.6.0 synchronizes lazy DSP initialization on Unix; Maple performs a one-time serialized warm-up on Windows, where that release uses an unsynchronized fallback. Upgrading the binding requires re-reviewing that initialization contract.

WebP APIs are native-only and are excluded from `wasm32` builds.

## Minimal usage

```rust
use maple_render_core::{GifAnim, Input, Inputs, Renders, Repository, TextOptions};
use maple_render_core::render::RenderQuality;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load a template zip generated using maple's template format.
    let repo = Repository::load("templates/toaster.zip")?;

    // Add image input.
    let mut inputs = Inputs::new();
    inputs.push(Input::load("examples/monkey.jpg")?);

    // Optional: add text input using your own font bytes.
    let font_data = std::fs::read("fonts/DejaVuSans-Bold.ttf")?;
    let text = Input::from_text("HELLO", 2, &TextOptions::default(), &font_data)?;
    inputs.push(text);

    // Render and encode GIF.
    let mut renders = Renders::new(repo, inputs, RenderQuality::Sampled);
    let mut gif = GifAnim::new(renders);
    gif.apply()?;
    gif.save("out.gif")?;

    Ok(())
}
```

## Load templates from bytes (embedded at build time)

You can bundle your template ZIP into your binary and load it without touching the filesystem at runtime.

`build.rs`:

```rust
use std::{env, fs, path::PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::copy("templates/toaster.zip", out_dir.join("template.zip")).unwrap();
    println!("cargo:rerun-if-changed=templates/toaster.zip");
}
```

`src/main.rs`:

```rust
use maple_render_core::Repository;

const TEMPLATE_ZIP: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/template.zip"));

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo = Repository::load_from_bytes(TEMPLATE_ZIP.to_vec())?;
    // ...
    Ok(())
}
```
