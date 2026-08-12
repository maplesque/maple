<img width="1300" height="500" alt="image" src="https://github.com/user-attachments/assets/df818959-85a3-4cb9-ae9f-08f18ef49684" />

[![CodSpeed](https://img.shields.io/endpoint?url=https://codspeed.io/badge.json)](https://app.codspeed.io/taskylizard/maple?utm_source=badge)

---

maple allows you to composite images/text with templates into animated GIFs/Videos. `templates/` has some fun ones to try!

Core rendering logic now lives in the publishable [`maple-render-core`](crates/maple-render-core) crate. The core crate does not bundle templates or fonts.

## Install

```
cargo install --path .
```

Requires Rust 1.80+. Video output requires ffmpeg.

## Core crate (publishable)

The standalone core crate lives at `crates/maple-render-core`.

```bash
cargo publish --manifest-path crates/maple-render-core/Cargo.toml
```

## Usage

```
maple --zip <template.zip> --in <image.png> --gif out.gif
```

### Output

| Flag                      | Description             |
| ------------------------- | ----------------------- |
| `--gif <path>`            | Animated GIF            |
| `--vid <path>`            | Video (ffmpeg required) |
| `--save "frame_%06d.jpg"` | Individual frames       |
| `--single`                | Single frame only       |

### Transform

| Flag             | Description           |
| ---------------- | --------------------- |
| `-w/-H`          | Output dimensions     |
| `--auto_zoom`    | Auto-fit content      |
| `--start <n>`    | Starting frame offset |
| `--first/--last` | Frame range           |

### Text

| Flag                    | Description         |
| ----------------------- | ------------------- |
| `--in "text:Hello"`     | Text as image layer |
| `--font_size <n>`       | Size (default: 72)  |
| `--text_color "FF0000"` | Hex color           |
| `--text_bg "FFFFFF"`    | Background hex      |

### Multi-layer

```bash
--in img1.png img2.png      # Layers 1, 2
--in img1.png +img1b.png    # Both on layer 1
```

### JSON Config

```bash
maple --json config.json
```

## Examples

```bash
# Photo to toaster GIF
maple --zip templates/toaster.zip --in photo.jpg --gif out.gif

# Text billboard
maple --zip templates/billboard-cityscape.zip --in "text:SALE" --gif ad.gif

# Book cover + back
maple --zip templates/book.zip --in front.png back.png --vid book.mp4
```

## Included Templates

back-tattoo, billboard-cityscape, book, circuitboard, flag, flag2, fortune-cookie, heart-locket, rubiks, toaster, valentine

## Benchmarks

Template parsing, frame compositing, color quantization and GIF encoding are
benchmarked with [divan](https://github.com/nvzqz/divan) in `benches/` and
tracked on every push and pull request by
[CodSpeed](https://app.codspeed.io/taskylizard/maple).

```bash
# Wall-clock numbers
cargo bench

# Same benchmarks, measured the way CI measures them
cargo install cargo-codspeed
cargo codspeed build --profile codspeed --measurement-mode simulation
cargo codspeed run
```

## License

MIT
