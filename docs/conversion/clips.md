[Home](../README.md) › [Conversion](README.md) › Clips

# Clips

A 317 sequence is a list of frames, and each frame is a list of transform ops on label sets. A clip is a list of per-bone keys, each a rotation and a translation. This page explains how one becomes the other, and which parts of a sequence don't make it across.

Implemented in [`rs2convert::clip`](../../crates/rs2convert/src/clip.rs), with the integer frame application in [`rs2convert::deform`](../../crates/rs2convert/src/deform.rs). The record layout is on [Anims](../pack/anims.md), and the bones come from [Rig inference](rig-inference.md).

## Two deformations side by side

For each frame, `bone_transforms` replays the frame's ops in order, as the original client applies them (see [Animation](../client/animation.md)). It tracks two things at once:

- **The integer reference.** The unscaled merged mesh, deformed one op at a time with the same integer arithmetic, sine tables and truncation as the original client. Origins come from this mesh exactly: an origin op sets the pivot to the integer mean of the current positions of its labelled vertices, plus the op's offset (just the offset when no vertex has those labels).
- **A per-bone affine transform**, in floating point, starting at identity. An op is folded into bone B's transform when B's whole label set is inside the op's label set (restricted to mesh labels):

| Op | Affine |
| --- | --- |
| origin | none (it only moves the pivot `o`) |
| translate `d` | `T(d)` |
| rotate `d` | `T(o) · R317(d) · T(−o)` |
| scale `d` | `T(o) · S(d / 128) · T(−o)` |
| alpha | none |

`R317(rx, ry, rz) = Ry · Rx · Rz`, which applies z first, then x, then y. Each angle is `(v & 0xff) × 8` into the original client's 2048-entry sine table, divided by 65536. The root, bone 0, always stays at identity.

## The scale conjugation

The rest mesh has the creature's scale `S = diag(scale / 128)` baked in, but the frames act on the unscaled mesh, and the original client scales afterwards. So each bone's transform is conjugated, `S · A · S⁻¹`, which maps scaled rest positions to scaled deformed positions.

## Pack space

Pack space negates y and z, `P = diag(1, −1, −1)`, which is half a turn about x and therefore a rotation. A cache-space transform `A` becomes `P · A · P`, and a key rotation `(rx, ry, rz)` means `P · R317 · P`.

## Parents first, against quantised parents

`frame_keys` handles bones in index order, so a parent always comes before its children. For bone B with pack-space pivot `p_B`:

```text
target = P · A_B · P · T(p_B)
local  = Q_parent⁻¹ · target            (Q_parent = identity for the root)
rest   = p_B − p_parent
```

`Q_parent` is the parent's **quantised** transform, rebuilt from the key that was actually chosen for it. Each child absorbs its parent's rounding instead of rounding error building up down the chain.

## Angle extraction

`angles_317` reads 8-bit angles back from a cache-space `Ry · Rx · Rz` matrix `m`, where `m[r][c]` is row r, column c:

```text
a = asin(clamp(−m[1][2], −1, 1))                      x angle
if |cos a| > 1e-6:
    b = atan2(m[0][2], m[2][2])                       y angle
    c = atan2(−m[1][0], m[1][1])                      z angle
else (gimbal lock):
    b = atan2(−m[2][0], m[0][0])                      y angle, z folded in
    c = 0                                             z angle
each angle → round(θ / 2π × 256) mod 256
```

First the scale comes off: the linear part of `local` is taken as `R · S`, each axis's scale is the length of that column, rounded to 1/256 and clamped to 1–65535, and the rotation is what's left, `M · S⁻¹`. The bone's exact angles are `angles_317(P · M · S⁻¹ · P)`.

## The 27-neighbour search

Rounding to 1/256 of a turn loses detail, so the exact angles are only a starting point. Each bone's bound vertices, the ones whose label binds to it, are used as `(o, d)` pairs: `o` is the rest position minus `p_B`, and `d` is the reference position mapped by `Q_parent⁻¹`, both in pack space.

For each of the 27 candidates (each angle −1, 0 or +1 from exact, mod 256):

1. `R` is the candidate's pack-space key rotation, times the rounded scale `S`.
2. The least-squares translation for that is the mean of `d − R·S·o`.
3. `pos` is that mean minus `rest`, rounded and clamped to `i16`.
4. The error is the **largest** `|R·S·o + rest + pos − d|` over the bound vertices.

The candidate with the smallest error wins. On equal errors the first one tried is kept (−1 before 0 before +1, x outermost, z innermost). A bone with no bound vertices skips the search and keeps the exact angles, with `pos = round(local.translation − rest)`.

The quantised transform is then `Q_B = Q_parent · T(rest + pos) · R(rot) · S(scale)`, and the key is `{t: frame index, bone: B, rot, pos, scale}`. Every bone gets a key in every frame, root included, so keys come out sorted by `(t, bone)`.

## Clip fields

| Field | Value |
| --- | --- |
| `rig` | the rig id the clip drives |
| `frameStart` | per frame, the sum of the lengths of the frames before it, clamped to 65535 |
| `duration` | sum of all frame lengths, clamped to 65535 |
| `loopOffset` | the sequence's loop offset, or 65535 when it has none |
| `priority` | the sequence's priority |
| `replayMode` | the sequence's replay mode |

A frame's length is the sequence's delay for it, or the frame's own delay from its frame pack when that's 0, and never less than 1. Units are the original client's 20 ms cycles.

A sequence with more than 255 frames is an error, since `frameCount` is a `u8`. So is a frame that can't be found in its pack.

## What isn't carried

- **Shear.** A key scales along the bone's own axes before turning it. A scale op followed by a rotation is exact. A non-uniform scale applied after a rotation shears the bone, which no key can hold, and that shows up as [validation](validation.md) error. Of the 592 spot animation clips, which scale a lot, 33 are flagged that way.
- **Secondary frames.** Only each sequence frame's primary frame is converted. Secondary frames and the sequence's interleave labels are ignored.
- **Alpha.** Alpha ops change face transparency, not positions, and keys have nowhere to put them.

## See also

- [Validation](validation.md): how the chosen keys are checked.
- [Animation](../client/animation.md): the original client's frame application and timing.
- [Coordinates](../client/coordinates.md): angle units and pack space.
- [Anims](../pack/anims.md): the clip record layout.
