[Home](../README.md) › [Crates](README.md) › rs2classic

# rs2classic

`rs2classic` is a software renderer that reimplements the original Java client's model renderer: its lighting, projection, face ordering and triangle fillers, in the client's own fixed-point arithmetic. Read this page to draw a cache model the way the original client would.

Entry point: [`crates/rs2classic/src/lib.rs`](../../crates/rs2classic/src/lib.rs). It depends on `rs2cache` for models and texture sheets and on `contentpack::palette` for the HSL table and brightness curve. It has no windowing or GPU dependency and draws into a plain `Vec<u32>`.

## What it renders

Given a decoded `rs2cache::types::model::Model`, it does what the original client does:

- lights the model once, per face for flat faces and per corner for smooth ones, into the 16-bit HSL colour table;
- projects vertices with the model-viewer projection, `x · 512 / z` about the buffer centre (or through a world camera for scenes);
- rejects back faces with the screen-space cross product;
- sorts faces into depth buckets drawn far to near, with the twelve-group priority interleave when the model has per-face priorities;
- fills triangles with the three fillers: smooth-shaded in 4-pixel colour runs, flat, and perspective-textured in 8-pixel spans with four shade levels;
- blends transparent faces per channel as `(src · (256 − a) + dst · a) >> 8`, where `a` is the face's alpha (0 opaque).

The arithmetic follows the original client's: 2048-unit angles with 16.16 sine tables, 16.16 edge slopes, and 32-bit wrapping where the client's `int` maths wraps. That last one matters for vertices just past the near plane, whose screen coordinates are huge. [Lighting](../client/lighting.md) and [rasteriser](../client/rasteriser.md) describe the behaviour step by step.

## API

| Item | What it is |
| --- | --- |
| `light(&model, &lighting)` | Lights a model and returns a `LitModel`. |
| `Lighting` | `ambient`, `contrast` and a light `direction` in cache coordinates. |
| `light::NPC`, `light::OBJECT` | The two standard setups: ambient 64, contrast 850, direction (−30, −50, −30); and ambient 64, contrast 768, direction (−50, −10, −50). |
| `LitModel` | Faces, face kinds (`type & 3`), lit colours or shades per corner, base colours, textured-face triangles, alphas and priorities. `LitModel::flat_rgb(faces, rgb)` builds an unlit model of 24-bit RGB triangles. |
| `Camera` | The model-viewer camera: roll about z, yaw about y, translate by `(x, y, z)`, then pitch about x. |
| `Camera::orbit(yaw, pitch, distance)` | A camera that keeps a model centred on the origin at `distance` along the view axis at any pitch. |
| `render(fb, lit, vertices, cam, textures)` | Draws a lit model through a `Camera`. |
| `View` | A world camera: `eye` (cache coordinates, y down), `yaw` and `pitch`. `to_camera(v)` transforms a world point. |
| `render_in_view(fb, lit, vertices, view, textures)` | Draws a model placed in the world through a `View`, skipping faces with a vertex nearer than `render::NEAR` (50). Returns `false` when the model is wholly behind the camera. |
| `Framebuffer` | `width`, `height` and `pixels` (`0xRRGGBB`); `new`, `clear(rgb)` and `to_rgba()`. |
| `Textures` | Prepared texture pages for ids 0–49: `Textures::from_sheets(&sheets, brightness)` and `page(id)`. |
| `terrain::ground(&plane, &floors, shadows)` | One plane of a map region as a lit `Ground`: a `LitModel` of gouraud and textured triangles plus its vertices in cache coordinates, with blended underlays, ground lighting, shadows, overlay shapes and textures. `None` when the plane has nothing to draw. |
| `terrain::FloorColour` | A floor's colour terms (hue, saturation, lightness, chroma, hue weight), its texture and whether it's the invisible floor; `FloorColour::new(&flo)`. |
| `terrain::Shadows` | The per-plane shadow grids; `add(...)` one location at a time, in file order. `horizontal_radius(&vertices)` gives the radius scenery shadows scale with. |
| `terrain::blend_underlays`, `terrain::light_grid`, `terrain::pack_hsl`, `terrain::shade` | The steps `ground` is built from, public for tests and other views. |

A `LitModel` is lit once, from the vertices you light it with. To animate, pass posed vertices to `render` and keep the same `LitModel`, which is what the original client does: it lights the rest pose and keeps those colours while animating.

Texture pages are prepared the way the original client prepares them: 128 × 128 texels (64-pixel textures doubled), colours through the brightness curve and masked to `0xf8f8ff`, plus three progressively darker copies for shading. A page with no zero texel is marked opaque.

Map ground comes from `terrain`, which works one region at a time; [Terrain](../client/terrain.md#what-the-region-viewer-draws) lists what that leaves out.

The lower-level modules are public too: `raster` (the `gouraud`, `flat` and `textured` fillers and `blend`), `render::draw_projected`, and `tables` (`sin`, `cos`, the span reciprocal `recip`, and `hsl`).

## Deliberate deviations

A few things differ from the original client on purpose:

- **An extra face kind.** `light::kind::FLAT_RGB` (4) is a flat face whose colour is 24-bit RGB rather than HSL, for drawing something in an exact colour. Map ground doesn't use it; it's built like the original client's (see [Terrain](../client/terrain.md)).
- **No colour jitter.** The HSL table comes from `contentpack::palette::hsl_to_rgb` at the default brightness, without the small random jitter the original client adds when it builds its table. Every run draws the same pixels.
- **Faces behind the camera are skipped.** In `render`, a face with any vertex at depth below 1 isn't drawn. The original client's model-viewer path never puts a model there, so it never has to care.
- **Missing textures draw grey.** A textured face whose texture page isn't loaded is drawn flat in `0x808080` rather than skipped.
- **Spans are always clamped.** The flat filler clamps each row to the buffer even when the face is marked unclipped. The original client only turns clipping off when every vertex is on screen; the clamp keeps bad input from indexing out of a row.

## Example

```rust
use rs2cache::types::model::Model;
use rs2classic::{Camera, Framebuffer, Textures, light, render};

fn draw(model: &Model) -> Vec<u8> {
    let lit = light(model, &rs2classic::light::NPC);
    let mut fb = Framebuffer::new(512, 512);
    let cam = Camera::orbit(0, 128, 1200);
    render(&mut fb, &lit, &model.vertices, &cam, &Textures::default());
    fb.to_rgba()
}
```

With `Textures::default()` every textured face draws grey; build real pages from the 50 texture sheets with `Textures::from_sheets` and `contentpack::palette::DEFAULT_BRIGHTNESS`. The cache-gated test in [`crates/rs2classic/tests/render_models.rs`](../../crates/rs2classic/tests/render_models.rs) does exactly that.

## See also

- [Lighting](../client/lighting.md): flat and smooth faces, textured shades, the two light setups.
- [Rasteriser](../client/rasteriser.md): projection, depth buckets, priorities and the fillers.
- [Colour](../client/colour.md): the HSL table and brightness.
- [Explorer](../tools/explorer.md): the classic renderer view that draws with this crate.
- [rs2ool](rs2ool.md): where the classic view lives.
