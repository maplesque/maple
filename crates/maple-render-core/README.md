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
