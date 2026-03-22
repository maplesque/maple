# maple-render-core

Core rendering engine extracted from Maple.

This crate contains:

- Template repository loading (`Repository`)
- Frame compositing and mapping (`Render`, `Renders`)
- GIF/video helpers (`GifAnim`, `VidAnim`)
- Quantization pipeline
- Input handling for images and text

It intentionally does **not** bundle templates or font assets.
For text rendering, callers must pass font bytes into `Input::from_text`.

## Publish checklist

From the repository root:

```bash
# 1) Validate locally
cargo check --manifest-path crates/maple-render-core/Cargo.toml
cargo test --manifest-path crates/maple-render-core/Cargo.toml

# 2) Inspect package contents
cargo package --manifest-path crates/maple-render-core/Cargo.toml --list

# 3) Full registry validation without upload
cargo publish --manifest-path crates/maple-render-core/Cargo.toml --dry-run

# 4) Publish for real
cargo publish --manifest-path crates/maple-render-core/Cargo.toml
```

## Minimal usage

```rust
use maple_render_core::{GifAnim, Input, Inputs, Renders, Repository, TextOptions};
use maple_render_core::render::RenderQuality;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load a template zip generated using Maple's template format.
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
