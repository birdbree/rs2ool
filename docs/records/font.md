[Home](../README.md) › [Records](README.md) › Fonts

# Fonts

The cache's bitmap fonts are sprite entries with exactly 256 glyphs. The metrics the original client draws text with aren't stored; they're derived from the glyph pixels. This page gives the derivation and what the dump writes.

Implemented in `decode_font` in [`rs2cache::sprite`](../../crates/rs2cache/src/sprite.rs); the `Font` type is in [`rs2cache::types::sprite`](../../crates/rs2cache/src/types/sprite.rs).

## The four fonts

Fonts live in the title archive (archive 1) and share its `index.dat` with the title sprites. Font ids follow this fixed order:

| Id | Entry |
| --- | --- |
| 0 | `p11_full.dat` |
| 1 | `p12_full.dat` |
| 2 | `b12_full.dat` |
| 3 | `q8_full.dat` |

The same order is the font index in an [interface](interface.md) component's text style. These four entries are left out of the [sprite](sprite.md) id list.

## Glyphs

A font entry has the ordinary [sprite layout](sprite.md#layout): a `u16` offset into `index.dat`, a header with the canvas size and palette, then one 7-byte record per glyph, all big-endian. There's no guessing at the count here: the decoder reads exactly 256 records, one per character code 0–255. Each glyph's pixels may be row-major or column-major, as for any sprite.

## Derived metrics

Each glyph gets an advance (how far the pen moves) and an x draw offset. Both come from the "column rule", which asks whether a glyph's leftmost or rightmost column is empty enough to trim.

For a glyph of width `w` and height `h`, take one column `x` and add up the stored pixel values (palette indices) in rows `h/7` to `h − 1`, using integer division. The top seventh is ignored. The column counts as empty when that sum is at most `h/7`.

```text
advance  = w + 2
x offset = 1
if w > 0 and column 0 is empty:      advance -= 1, x offset = 0
if w > 0 and column w − 1 is empty:  advance -= 1
advance is clamped to 0–255
```

A one-pixel-wide glyph has a single column that counts as both ends, so it can lose 2. Note that the rule adds up palette values rather than counting inked pixels. That's what the original client does too.

The stored x offset of each glyph is ignored for drawing; the x draw offset from the rule replaces it.

### The space advance

After the rule runs, the advance of character 32 (space) is overwritten with another glyph's: the advance of 73 (`I`) for `q8_full`, and of 105 (`i`) for the other three. The space glyph's own pixels play no part. This one surprises people when their spaces come out the wrong width.

### Y offsets

The y draw offset is the glyph's stored y offset, reinterpreted as a signed byte.

## Dump output

```text
rs2ool cache decode font --id 1 -o out/
```

writes two files per font:

- `font/<name>.png`: a glyph sheet. The 256 glyphs are laid out 16 × 16 in character order (code `c` in column `c % 16`, row `c / 16`). Every cell is the same size: the widest glyph plus 2 wide, and the largest `height + y offset` tall. Each glyph is drawn into its cell at its x draw offset and its y offset, with palette index 0 transparent.
- `font/<name>.metrics.json`: the metrics as JSON, one array entry per character code.

```text
{
  "name":          entry name without .dat
  "advance":       [256 × u8]
  "draw_offset_x": [256 × i8]    0 or 1
  "draw_offset_y": [256 × i8]
  "glyph_sizes":   [256 × [width, height]]
}
```

With `--meta`, the sidecar's summary gives the glyph count and the canvas height. See [dump output](../tools/dump-output.md).

## See also

- [Sprites](sprite.md): the shared byte layout.
- [Interfaces](interface.md): the font index in text styles.
- [Pack sprites](../pack/sprites.md): font metrics in packs.
- [Textures and sprites conversion](../conversion/textures-and-sprites.md): how fonts are converted.
- [Cache commands](../tools/cache-commands.md): `decode` and `dump`.
