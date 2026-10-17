[Home](../README.md) › [Client](README.md) › Colour

# Colour

Model faces in the 317 cache are coloured with 16-bit HSL values, and the original client turns them into RGB through one fixed table. This page gives the bit layout and the exact conversion, so you can reproduce the client's colours to the last bit.

The conversion is implemented in [`contentpack::palette`](../../crates/contentpack/src/palette.rs) (`hsl_to_rgb`, `hsl_to_rgb_with`, `gamma`). The renderer builds its 65,536-entry lookup table from it in [`rs2classic::tables`](../../crates/rs2classic/src/tables.rs).

## Bit layout

```text
bits 15–10   hue          6 bits
bits 9–7     saturation   3 bits
bits 6–0     lightness    7 bits

hue        = (c >> 10) & 63
saturation = (c >> 7) & 7
lightness  = c & 127
```

So there are 64 hues, 8 saturations and 128 lightness steps. [Lighting](lighting.md) only ever changes the low 7 bits: hue and saturation pass through untouched.

## HSL to RGB

The original client fills its table with a standard HSL-to-RGB conversion, with two quirks: hue and saturation are offset by half a step, and the result goes through a gamma curve. In floating point (`f64`):

```text
h = hue / 64 + 1/128           (0.0078125)
s = saturation / 8 + 1/16      (0.0625)
l = lightness / 128

q = l < 0.5 ? l · (1 + s) : l + s − l · s
p = 2l − q

tr = h + 1/3, minus 1 if > 1
tg = h
tb = h − 1/3, plus 1 if < 0

channel(t) =
  p + (q − p) · 6t             if 6t < 1
  q                            else if 2t < 1
  p + (q − p) · (2/3 − t) · 6  else if 3t < 2
  p                            otherwise

R = trunc(channel(tr) · 256), likewise G, B
rgb = R << 16 + G << 8 + B
```

Because of the saturation offset, saturation 0 is never quite grey: it still carries a faint tint of its hue.

## Brightness

Each channel then goes through a power curve with the client's brightness as the exponent:

```text
gamma(c) = trunc((c / 256) ^ brightness · 256)
```

The default brightness is 0.8 (`DEFAULT_BRIGHTNESS`), which lifts the darker values. The result is masked to 24 bits.

The same `gamma` function is applied to texture palettes when [texture pages](rasteriser.md#texture-pages) are built (the brightness is a parameter there; the tools pass 0.8). Textures store RGB, so they skip the HSL step.

## Zero is transparent

The original client reserves RGB 0 for transparency, so a colour that comes out as pure black is replaced with 1 (`0x000001`). That happens exactly when the lightness is 0. Every one of the 65,536 HSL values therefore maps to a non-zero 24-bit colour.

## Startup jitter

At startup, the original client adds a small random amount, uniform between −0.015 and +0.015, to the brightness before building the table. Every session's colours are therefore very slightly different. The tools leave this out and always use exactly 0.8, so their colours are the centre of that range and are the same on every run.

## See also

- [Lighting](lighting.md): how lit colours change the lightness bits.
- [Rasteriser](rasteriser.md): where the table is used, and texture pages.
- [Primitives](../cache/primitives.md): 16-bit HSL among the cache's basic types.
- [Palette conversion](../conversion/palette.md): turning these colours into pack materials.
- [Pack materials](../pack/materials.md): the palette record.
