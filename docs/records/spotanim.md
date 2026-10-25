[Home](../README.md) › [Records](README.md) › Spotanim

# Spotanim: graphic effects

`spotanim.dat` holds graphic effects: a model, the sequence that animates it, a scale, a rotation, lighting values and recolours. This page lists the opcodes and the recolour pairing, and says what the tools do with the result.

Decoded by [`rs2cache::config::spotanim`](../../crates/rs2cache/src/config/spotanim.rs) into `SpotAnimDef` ([`rs2cache::types::config`](../../crates/rs2cache/src/types/config.rs)). Cache data is big-endian.

## Storage

Spotanim is a **sequential** type: `spotanim.dat` is `count u16` followed by the definitions back to back, with no `.idx`. See [Config archive](../cache/config.md).

```text
rs2ool cache decode spotanim --id 1 -o out/
```

## Opcodes

Each definition ends with opcode 0.

| Opcode | Reads | Meaning |
| --- | --- | --- |
| 1 | `u16` | `model` |
| 2 | `u16` | `sequence` |
| 4 | `u16` | `scale_xy`: horizontal scale, x and z (128 = 1.0) |
| 5 | `u16` | `scale_z`: vertical scale, y |
| 6 | `u16` | `rotation` |
| 7 | `u8` | `ambient` (unsigned) |
| 8 | `u8` | `contrast` (unsigned, not multiplied) |
| 40–45 | `u16` | Recolour source colour, slot `op − 40` |
| 50–55 | `u16` | Recolour target colour, slot `op − 50` |
| 46–49, 56–59 | `u16` | Read, then recorded as unknown |

Any other opcode, 3 included, is logged, added to `unknown_opcodes`, and skipped without reading anything.

Opcodes 46–49 and 56–59 fall inside the original client's recolour opcode ranges but past the end of its six-slot arrays. The decoder reads their `u16` to keep the stream aligned, then treats them as unknown.

Don't read the scale names as axes. In cache coordinates y is vertical, and the tools apply `scale_xy` to x and z and `scale_z` to y.

`model` and `sequence` keep 65535 as an id, as the original client does; see [65535 as none](../cache/config.md#65535-as-none).

## Defaults

`scale_xy` and `scale_z` start at 128. `rotation`, `ambient` and `contrast` start at 0. `model` and `sequence` are none, and all twelve recolour slots are 0.

## Post-decode derivations

As with [Idk](idk.md), the recolours are read into two six-slot arrays, `from` and `to`, both zero to start with. After decoding they're paired slot by slot, stopping at the first slot whose `from` is 0, and stored as an ordinary `recolours` list. A pair after a zero `from` is dropped.

Unlike npc and obj, `contrast` here is an unsigned byte kept as read.

## Fields

`SpotAnimDef` holds: `id`, `model`, `sequence`, `scale_xy`, `scale_z`, `rotation` (all `u16`), `ambient` and `contrast` (`u8`), `recolours`, and `unknown_opcodes` (omitted from JSON when empty).

## Use in the tools

`rs2ool pack spotanims` converts them into a model, a rig and a clip each ([Spot animations](../conversion/creatures.md#spot-animations)). In the explorer, one with a sequence plays it, posed and scaled as the original client does, and one without shows its model. That preview comes with [`rs2convert::preview`](../../crates/rs2convert/src/preview.rs): `model` with the recolours applied, scaled by `[scale_xy, scale_z, scale_xy] / 128` on x, y, z. A spotanim with no model has no preview. `rotation` isn't applied; conversion lists it in the models pack's manifest instead. The inspector links `model` to its [model](model.md) and `sequence` to its [seq](seq.md).

## See also

- [Config archive](../cache/config.md): sequential storage and the recolour pair.
- [Idk](idk.md): the same paired recolour arrays.
- [Seq](seq.md): the sequence an effect plays.
- [Coordinates](../client/coordinates.md): which cache axis is vertical.
