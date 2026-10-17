[Home](../README.md) › [Conversion](README.md) › Palette

# Palette

Converted records refer to colours by index into one palette of at most 1024 RGB entries. This page shows how that palette is built from the cache's face colours, how a colour finds its entry, and what the error report measures.

Implemented in [`rs2convert::palette`](../../crates/rs2convert/src/palette.rs), with the colour maths and k-means in [`contentpack::palette`](../../crates/contentpack/src/palette.rs). The output records are described on [Materials](../pack/materials.md).

## The corpus

The corpus is every untextured face colour in the cache, each with the number of faces that use it:

- every model in index 1 is decoded (in parallel), and a model that fails to decode is skipped;
- a face is textured when the model has face types and bit 1 (`type & 2`) of the face's type is set. Textured faces carry a texture id instead of a colour, so they're left out;
- each remaining face adds 1 to the count of its 16-bit HSL colour.

The result is a map from HSL colour to face count, in ascending colour order.

## RGB values

Each HSL colour is turned into 24-bit RGB with the original client's HSL→RGB conversion at its default brightness of 0.8, described on [Colour](../client/colour.md). The original client adds a small random jitter when it builds its table at startup. The conversion here leaves that out. A result of 0 becomes 1, because the original client reserves 0 for transparency.

Distinct HSL values can land on the same RGB value. They stay separate points.

## k-means

The points are `(r, g, b)` as floating point, each weighted by **√count**. The square root keeps rare colours from being swallowed by very common ones. `kmeans(points, 1024, 100)` then runs:

1. **Few points.** If there are at most 1024 points, the points themselves are the centres and nothing else happens.
2. **Seeding.** The first centre is the heaviest point, with ties going to the earliest point (the lowest HSL value). Each following centre is the point with the largest `d² × √w`, where `d²` is its squared distance to the nearest centre chosen so far and `w` is its weight. Ties go to the earliest point. Seeding stops at 1024 centres.
3. **Lloyd iterations.** Up to 100 rounds. Each round assigns every point to its nearest centre, then moves every centre to the weighted mean of its points. A centre with no points stays where it is. The loop stops early when a round's assignments match the previous round's.

Nothing here is random, so the same corpus always gives the same centres.

Each centre is rounded to the nearest integer and clamped to 0–255. The entries are then **sorted** (by r, then g, then b) and **deduplicated**, so the palette's order doesn't depend on seeding, and it can come out with fewer than 1024 entries.

## Looking up a colour

The nearest entry is the one with the smallest squared Euclidean RGB distance. Ties go to the lowest index.

| Method | Does |
| --- | --- |
| `index(hsl)` | nearest entry for an HSL colour, memoised per colour |
| `prime(corpus)` | fills the memo for every corpus colour, in parallel |
| `index_of(hsl)` | the memo's answer if there is one, otherwise computes it without storing |
| `index_rgb(rgb)` | nearest entry for a 24-bit RGB colour (used by [floors](floors.md)) |

Every conversion primes the palette with the corpus and then calls `index_of`. Colours outside the corpus, such as recolour targets, are computed on demand. Memoised or not, the answer is the same.

## Error metrics and `palette.json`

For every corpus colour, the error is the Euclidean distance in RGB units between its own RGB value and its palette entry:

- `mean` is averaged over **faces**: `Σ error × count / Σ count`;
- `max` is the worst **distinct colour**, regardless of how many faces use it, and `worst_hsl` is that colour.

`rs2ool pack materials` writes this as `palette.json` (pretty-printed, with a trailing newline):

```json
{
  "colours": 0,
  "faces": 0,
  "mean": 0.0,
  "max": 0.0,
  "worst_hsl": 0
}
```

`colours` is the number of distinct corpus colours and `faces` the total face count. The run's log line repeats the entry count, colours, faces, mean and max.

## The materials pack

The `materials` subcommand writes one `materials` pack with two records:

- record `0xFFFFFFFF`, the palette: `count u16` then `count × {r, g, b u8}`, little-endian like all pack data;
- material 0, the palette material every untextured face uses: mode palette, texture 0, period 0, palette 0, roughness 255, flags 0, alpha 255.

## The swatch

`palette.png` is a 256 × 256 RGBA image: a 32 × 32 grid of 8-pixel cells. Entry `i` fills the cell at column `i % 32`, row `i / 32`, opaque. Any cell past the last entry stays transparent black, which is how you spot a deduplicated palette at a glance.

## See also

- [Colour](../client/colour.md): the HSL encoding and the HSL→RGB table.
- [Materials](../pack/materials.md): the palette record and material layout.
- [Models](models.md): where palette indices end up.
- [Determinism](determinism.md): why seeding and sorting matter for byte-identical packs.
