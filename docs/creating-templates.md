# Creating Templates for Maple (Paint.NET Guide)

This guide explains how to build your own maple templates using [Paint.NET](https://www.getpaint.net/). A template is a `.zip` file containing a set of specially crafted PNG images and a JSON manifest that tells maple how to composite your input images into an animation.

## How Maple Rendering Works

For each frame of an animation, maple uses **five images** to determine how to place your input image onto a scene:

| Image | Purpose |
|---|---|
| **light** | The scene lit with a white surface where the input goes |
| **dark** | The scene lit with a black surface where the input goes |
| **map** | UV coordinate map — tells maple *where* each pixel samples from the input |
| **sel** | Layer selection — tells maple *which* input layer a pixel belongs to |
| **transparent** | The neutral/background frame shown where no input is mapped |

The rendering formula for each pixel is:

```
output = dark + (light - dark) × input_sample
```

This means:
- Where your input image is **white (255)**, the output matches the `light` image.
- Where your input image is **black (0)**, the output matches the `dark` image.
- In-between values are interpolated, producing realistic lighting and shading on the composited image.

## Template Zip Structure

A template zip file contains:

```
template.json
frame0_light.png
frame0_dark.png
frame0_map.png
frame0_sel.png
frame0_transparent.png
frame1_light.png
frame1_dark.png
...
```

For a **static (single-frame) template**, you only need `frame0_*` files.

For an **animated template**, provide `frame0_*` through `frameN_*` (0-indexed), where N = `frames - 1` in your JSON config.

### template.json

```json
{
  "width": 400,
  "height": 300,
  "frames": 1,
  "delay": 0.1,
  "hold": 1.0,
  "palette": [0]
}
```

| Field | Description |
|---|---|
| `width` | Output width in pixels |
| `height` | Output height in pixels |
| `frames` | Total number of frames (1 for static templates) |
| `delay` | Delay between frames in seconds (e.g., `0.05` = 20 FPS, `0.1` = 10 FPS) |
| `hold` | Extra hold time on the last frame in seconds (0 for seamless loops) |
| `palette` | Frame indices used for GIF color palette generation (usually `[0]`) |

## Creating Each Image Layer

All frame images must be the **same dimensions** (`width` × `height` from your JSON).

### Step 1: Create the Scene — `light` and `dark`

These two images define how your scene looks when the input surface is fully white vs fully black. They control the lighting and shading effect.

**In Paint.NET:**

1. Create your scene (e.g., a picture frame, a billboard, a TV screen, a book cover).
2. For the `light` image: render/paint the scene with the target surface as **pure white** (`#FFFFFF`).
3. For the `dark` image: render/paint the scene with the target surface as **pure black** (`#000000`).
4. Everything else in the scene (the frame, the wall, shadows, etc.) should look the same in both images.

The difference between light and dark is what creates the realistic blending effect. Shadows should darken both images equally. Specular highlights should appear in the light image.

> **Tip:** If you're working with a 3D render, render it twice with different surface colors. For hand-drawn scenes, duplicate your artwork and fill the target area with white in one copy and black in the other.

### Step 2: Create the Background — `transparent`

This is what viewers see where no input image is mapped. Typically, this is a full render of your scene without any input content — showing just the background environment.

If your scene has no transparent areas, you can make this identical to the `light` image (maple falls back to `light` if `transparent` is missing).

### Step 3: Create the UV Map — `map`

This is the most technical image. Each pixel's RGBA channels encode where to sample from the input image using a **4096×4096 coordinate system** centered at (2048, 2048).

**Channel encoding:**

| Channel | Encodes |
|---|---|
| **R (Red)** | Low byte of X coordinate (0–255) |
| **G (Green)** | Low byte of Y coordinate (0–255) |
| **B (Blue)** | High bits, packed: `floor(B / 16)` = Y high nibble, `B % 16` = X high nibble |
| **A (Alpha)** | Activation mask — values > 25 mean "active" (map this pixel), ≤ 25 means "skip" |

The full coordinate is computed as:
```
X = R + 256 × (B % 16) - 2048
Y = G + 256 × floor(B / 16) - 2048
```

Coordinates near (0, 0) map to the center of the input image. The range −2048 to +2047 covers the full input.

**Practical approach in Paint.NET for a flat surface:**

For a simple, flat rectangular surface (like a screen or a poster on a wall), you need smooth gradients:

1. Create a new image at your template dimensions.
2. Set the alpha to 255 everywhere the input should appear, 0 everywhere else.
3. For the mapped region:
   - **Red channel**: Create a horizontal gradient from 0 on the left to 255 on the right. If your surface spans more than 256 pixels of coordinate range, you'll need the Blue channel's low nibble to carry the overflow.
   - **Green channel**: Create a vertical gradient from 0 at the top to 255 at the bottom. Similarly, the Blue channel's high nibble carries overflow.
   - **Blue channel**: For small surfaces fitting within a 256-pixel coordinate range, set this to `128` (which centers around coordinate 0: `128 / 16 = 8` for Y high, `128 % 16 = 0` for X high, giving offset 2048 which cancels the -2048). For larger or offset surfaces, compute the appropriate packed value.

> **Simplified approach for beginners:** For a flat, axis-aligned rectangular region, paint the region with R going from 0→255 left to right, G going from 0→255 top to bottom, B = `0x88` (136, which gives high nibble 8 for both X and Y, centering the coordinate), and A = 255. Leave everything else at A = 0. This maps a centered square from the input image onto your surface.

**For curved or perspective-distorted surfaces:**

You will need to compute or paint the UV coordinates accounting for the distortion. Each pixel's RGBA tells maple where in the input image to sample from.

If you have a 3D modeling tool, you can render the UV map directly from your 3D scene. Otherwise, you can approximate perspective distortion by skewing your gradients.

### Step 4: Create the Layer Selection — `sel`

This image determines which input layer each pixel belongs to.

**Channel encoding:**

| Channel | Purpose |
|---|---|
| **R (Red)** | Layer index: `1` = first input, `2` = second input, etc. `0` = no layer |
| **G (Green)** | Unused for layer selection |
| **B (Blue)** | Edge smoothing flag: values > 0 mark pixels for anti-aliased edge blending |

**In Paint.NET:**

1. Create a new image at your template dimensions.
2. Fill the entire image with `R=0, G=0, B=0` (no layer mapped anywhere).
3. Paint the area where input layer 1 should appear with `R=1, G=0, B=0`.
4. If you have a second input layer, paint that area with `R=2, G=0, B=0`.
5. Along the edges of your mapped regions, paint a 1-pixel border with `B > 0` (e.g., `R=0, G=0, B=128`) to enable edge smoothing. This makes the boundary between the composited area and the background look smooth.

> **Tip:** Use the pencil tool (not the brush) to paint exact pixel values. Open the color picker, set the hex values manually, and draw your regions. You can zoom in to verify pixel-level accuracy.

## Single-Layer vs Multi-Layer Templates

**Single-layer** (e.g., flag, toaster): Only uses layer 1. The sel image has `R=1` in the mapped region and `R=0` elsewhere.

**Multi-layer** (e.g., book with front + back cover): Uses layers 1 and 2. Different regions in the sel image have `R=1` or `R=2`. Each layer maps independently via the same UV map — the sel image controls which input goes where.

When a user runs a multi-layer template:

```bash
maple --zip mytemplate.zip --in front.png back.png --gif out.gif
```

`front.png` is layer 1, `back.png` is layer 2.

## Animated Templates

For animations, create the full set of 5 images for every frame. Each frame can have different:
- Camera angles (light/dark/transparent change)
- UV mappings (map changes for moving/rotating surfaces)
- Layer visibility (sel can show/hide layers per frame)

Number your frames starting from 0: `frame0_*.png`, `frame1_*.png`, ..., `frame22_*.png` for a 23-frame animation.

Set `delay` in `template.json` to control playback speed and `hold` for how long the last frame lingers.

## Packaging the Template

1. Select all your frame images and `template.json`.
2. Zip them into a single `.zip` file (files at the root of the archive, no subdirectory).
3. Test it:

```bash
maple --zip mytemplate.zip --in examples/monkey.jpg --gif test.gif
```

Use `--files` to verify maple reads your template correctly:

```bash
maple --zip mytemplate.zip --in examples/monkey.jpg --files
```

## Quick-Start Checklist

- [ ] Decide on output dimensions (`width` × `height`)
- [ ] Decide frame count (1 for static)
- [ ] For each frame, create all 5 PNGs at the same dimensions
- [ ] Write `template.json` with correct `frames` count
- [ ] Ensure the `map` image alpha is 255 in mapped regions, 0 elsewhere
- [ ] Ensure the `sel` image R channel matches your layer count (1 for single-layer)
- [ ] Zip everything (flat, no subdirectories) and test

## Paint.NET-Specific Tips

- **Viewing individual channels:** Use the Channels window (Window → Colors → click channel buttons) to inspect R, G, B, A individually.
- **Setting exact pixel colors:** Use the Color Picker dialog (F8), switch to hex mode, and type exact values like `01000000` (R=1, G=0, B=0, A=0) for sel maps.
- **Gradients:** The built-in gradient tool works for simple flat UV maps. Draw a linear gradient in the R channel for horizontal mapping and G channel for vertical mapping.
- **Working with alpha:** Make sure "Overwrite" blending mode is selected when painting alpha values, so you get exact values rather than blended results.
- **Saving PNGs:** Always save as 32-bit PNG (RGBA) to preserve all four channels. Paint.NET does this by default with the "Flatten" option unchecked.

## Troubleshooting

| Problem | Likely Cause |
|---|---|
| Input image doesn't appear | `sel` R channel isn't set to 1 (or the correct layer number) |
| Black rectangle instead of input | `map` alpha is 0 in the target area — set it to 255 |
| Distorted or garbled output | UV map coordinates are wrong — check R, G, B encoding |
| Seams/harsh edges at boundaries | Missing edge smoothing — paint `B > 0` in the `sel` image along edges |
| "MissingFile" error | Frame file naming is wrong — must be `frame0_light.png` (0-indexed) |
| Wrong number of frames | `frames` in `template.json` doesn't match the number of frame sets in the zip |
