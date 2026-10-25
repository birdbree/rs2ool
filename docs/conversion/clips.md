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
| alpha | none (alpha goes into fades, below) |

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

## Secondary frames

A sequence frame with a secondary frame becomes one key per bone for both: [`bone_transforms`](../../crates/rs2convert/src/clip.rs) replays the frame's ops, then the secondary frame's through its own base, each from a zero origin, and the keys are fitted to the reference after both, as the original client draws them in an interface ([Animation](../client/animation.md#secondary-frames)). Rig inference counts the secondary frames' label sets too. Only chat-head sequences have secondary frames, and no pack command converts chat heads, so this matters for completeness more than for any pack written today.

## Moving bones and mix keys

A sequence with interleave labels gets two more things, for playing over a movement clip the way the original client [mixes](../client/animation.md#interleaving) the two sequences.

**Moving bones** (`clip::moving_bones`) are the bones whose local transform comes from the movement clip. Each bone is judged by its own slots, the frame base slots that move all of its labels but not all of its parent's (the ones a local transform holds):

- a bone goes to the movement when its own slots include a listed one and no unlisted one the sequence actually moves (an unlisted slot whose every transform in the sequence is the identity doesn't count);
- a bone with no own slots follows its parent.

Slots come from the base of the sequence's first frame. On the player skeleton this gives the pelvis, thighs, shins and feet for every one of the 135 actions that share the walk's base.

**Mix keys** are keys for every other bone, fitted to the sequence's frames with the listed slots removed ([`deform::interleave_part`](../../crates/rs2convert/src/deform.rs)): the part the original client applies when it mixes. They can't be the clip's own keys. A sequence that turns or leans the whole body does so through listed slots, which the mix drops, so its upper body has to be posed again without them. Mixed through its own keys, a spin attack (sequence 439) was up to 136 units out over the walk and run; through its mix keys it's under 2 over the walk.

## Fades

Alpha transforms become the clip's fades (`clip::fades`). For each frame, every face label that an alpha slot lists gets the original client's steps in order (frame, then secondary frame), each `dx × 8` added to transparency and clamped to 0–255. A chain of clamped adds always collapses into one, `clamp(x + add, lo, hi)`:

```text
clamp(clamp(x + a, l, h) + s, 0, 255) = clamp(x + a + s, clamp(l + s, 0, 255), clamp(h + s, 0, 255))
```

So one fade per face label and frame reproduces the original client exactly, whatever each face's own transparency, and [validation](validation.md#fades) expects no mismatches at all. `add` is held to ±255, past which it changes nothing for a value in 0–255. Fades are stored as opacity (255 − transparency), the pack's convention, which turns `(add, lo, hi)` into `(−add, 255 − hi, 255 − lo)`. A face label with no change at a frame gets no fade, and a model without face labels gets none, as the original client fades nothing then.

A mix with movement takes the clip's own fades: the original client fades through the action frame's slots outside the interleave list, and alpha slots are never listed. Validation checks that too.

## What isn't carried

- **Shear.** A key scales along the bone's own axes before turning it. A scale op followed by a rotation is exact. A non-uniform scale applied after a rotation shears the bone, which no key can hold, and that shows up as [validation](validation.md) error. Of the 592 spot animation clips, which scale a lot, 33 are flagged that way.
- **The movement's own share of a mix.** A movement clip's keys are fitted to its whole frames, but a mix only takes its listed slots. Where the movement also moves unlisted slots that reach vertices its moving bones carry (walking sideways and running swing the arms harder), some of that leaks into the mix. That's why the [mix threshold](validation.md#mixes) is twice the clip threshold.

## See also

- [Validation](validation.md): how the chosen keys are checked.
- [Animation](../client/animation.md): the original client's frame application and timing.
- [Coordinates](../client/coordinates.md): angle units and pack space.
- [Anims](../pack/anims.md): the clip record layout.
