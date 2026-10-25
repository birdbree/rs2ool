[Home](../README.md) › [Client](README.md) › Animation

# Animation

How a frame deforms a model, vertex for vertex, as the original client does it, and how sequences time their frames. Read this to reproduce a posed mesh exactly or to check a converted clip against the original.

The reference deformation is [`rs2convert::deform`](../../crates/rs2convert/src/deform.rs). It runs in the same integer arithmetic as the original client, and clip validation and the explorer's cache playback both use it. Frames come from [`rs2cache::anim`](../../crates/rs2cache/src/anim.rs) (layout on [Frame packs](../records/frame-pack.md)), and the explorer's clock is in [`playback.rs`](../../src/explorer/playback.rs).

## Vertex groups

Every model vertex carries a label byte (the model's vertex labels, see [Models](../records/model.md)). Before animating, vertices are grouped by label: group `l` lists, in vertex order, every vertex whose label is `l`, for `l` from 0 to the highest label present.

A frame-pack base slot names a list of labels. A transform on that slot visits the groups for those labels in the order listed. A label with no group (higher than any label in the model) is skipped. Models without vertex labels have no groups, so nothing moves.

## Applying a frame

A frame is a list of transforms, each naming a base slot and carrying three signed values `(dx, dy, dz)`. The transforms run in order. One piece of state carries between them: the **origin**, which starts at (0, 0, 0) for every frame.

The slot's kind decides what the values mean:

| Kind | Name | Effect on the slot's vertices |
| --- | --- | --- |
| 0 | origin | none; sets the origin |
| 1 | translate | add `(dx, dy, dz)` |
| 2 | rotate | rotate about the origin |
| 3 | scale | scale about the origin |
| 5 | alpha | none; changes face alpha |

Any other kind does nothing. A slot that names a missing base slot is skipped.

### Origin

The origin becomes the integer centroid of the slot's vertices plus the offset:

```text
n      = number of vertices visited
origin = (Σx / n + dx, Σy / n + dy, Σz / n + dz)    integer division, truncating
origin = (dx, dy, dz)                                when n = 0
```

The sums run over the vertices as they are at that moment, after any earlier transforms in the same frame.

### Translate

Each vertex gets `(dx, dy, dz)` added. The origin isn't involved.

### Rotate

Each component is an 8-bit angle: `kx = (dx & 0xff) × 8`, likewise `ky` from dy and `kz` from dz, giving indices into the 2048-entry 16.16 tables (see [Coordinates](coordinates.md#angles)). Each vertex is moved to be relative to the origin, rotated about z, then x, then y, and moved back. A component of 0 skips its step.

```text
(x, y, z) -= origin

if kz ≠ 0:  s = sin[kz], c = cos[kz]
            x' = (y·s + x·c) >> 16
            y' = (y·c − x·s) >> 16
if kx ≠ 0:  s = sin[kx], c = cos[kx]
            y' = (y·c − z·s) >> 16
            z' = (y·s + z·c) >> 16
if ky ≠ 0:  s = sin[ky], c = cos[ky]
            x' = (z·s + x·c) >> 16
            z' = (z·c − x·s) >> 16

(x, y, z) += origin
```

Each step uses the values from the step before, and `>>` is an arithmetic shift, so negative results round toward minus infinity. The truncations add up: a quarter turn of a point 100 units out can land a unit off.

### Scale

Each axis is scaled by its value over 128, about the origin:

```text
v = (v − origin) × d / 128 + origin    per axis, integer division
```

A component the frame doesn't store defaults to 128, which leaves that axis alone.

### Alpha

Alpha transforms act on faces rather than vertices. In the original client, an alpha transform adds `dx × 8` to the alpha of every face whose face label is in the slot's list, clamped to 0–255. The decoder sets the frame's `alpha` flag when it holds any alpha transform. The reference deformation moves vertices only, so it skips alpha slots.

## The implicit origin

A frame only stores the slots it uses. When a stored slot isn't an origin, the decoder looks back for the nearest origin slot after the previous stored slot and before this one. If it finds one, it inserts that origin with zero offsets just before the transform, the way the original client does. A rotation or scale then pivots about the centroid its base intends, even though the frame never mentioned the origin.

The search only covers slots the frame skipped. An origin the frame stores explicitly is used as stored, and one sitting before the previous stored slot isn't revisited.

## Scale after animating

NPC definitions carry a scale. The original client animates the model first and scales it afterwards, so frame deltas and origins work in unscaled units. The reference deformation does the same: it applies the frame to the merged, unscaled model, then multiplies each vertex by `scale / 128` per axis (skipped when all three are 128). An NPC's scale is `(scale_xz, scale_y, scale_xz)`; see [Assembly](assembly.md#npc-scale).

## Sequence timing

A sequence (`seq.dat`, see [Sequences](../records/seq.md)) lists frames, each with a global frame id, an optional secondary frame id (see [Secondary frames](#secondary-frames)) and a delay in cycles of the original client. One cycle is 20 ms.

A frame's length is:

1. the sequence's delay for that frame, if it isn't 0;
2. otherwise the frame's own delay byte from its frame pack;
3. otherwise 1.

The explorer's clock also treats any remaining 0 as 1, so every frame lasts at least one cycle.

**Loop offset** (opcode 2) is stored as read, with 65535 kept as 65535 rather than turned into "none". It counts frames from the end. When playback runs past the last frame with a loop offset `o` where `0 < o ≤ frame count`, it carries on at frame `frame count − o`; any other value, or no offset, restarts at frame 0. With looping off, the explorer stops on the last frame.

**Replay mode** (opcode 11, default 2) is stored and copied unchanged into converted clips. The explorer's playback doesn't act on it.

The other sequence fields (priority, maximum loops, precedences, the hand overrides) are decoded and stored and don't affect playback.

## Secondary frames

Each sequence frame can name a secondary frame (65535 in the stream meaning none). The original client only applies it to models drawn in interfaces, such as chat heads: the frame first, in full, then the secondary frame, in full, each from a zero origin and through its own frame base. The reference deformation does the same whenever a sequence has secondary frames (`Creature::frames`, `Creature::posed`): the sequences that have them only ever play in interfaces. Clips hold both, one key per bone for the pair ([Clips](../conversion/clips.md#secondary-frames)). The `npc:` and `player-kit` dump groups include the frame packs that secondary frames live in.

## Interleaving

Opcode 3 reads a count byte and that many bytes, and the decoder keeps them as `interleave_labels`, in the order read, with nothing appended. When they're present and the sequence doesn't set its precedences (opcodes 9 and 10), both default to 2 instead of 0.

A creature can play an action sequence while it moves. With walking precedence 2, the default for a sequence with interleave labels, it keeps moving and the original client mixes the two frames each time it draws the creature, as long as the movement sequence isn't the creature's stand:

1. The action frame's transforms, except those whose slot is in the list, are applied from a zero origin.
2. The movement frame's transforms whose slot is in the list are applied, again from a zero origin.
3. Origin transforms are applied from both, whatever the list says.
4. Both frames' slots are looked up in the action frame's base, even if the movement frame comes from a pack with a different one. When the two bases differ, the movement frame's slots mean other things and the mix is garbled.
5. The list is walked once, forwards, in step with each frame's ascending slots, so a value smaller than one before it never matches.

Each sequence runs on its own clock, so the movement keeps its own pace under the action. Without a list, the action's frame is drawn alone whether the creature moves or not.

[`deform::apply_interleaved`](../../crates/rs2convert/src/deform.rs) does exactly this, and `Creature::posed_moving` gives the mixed, scaled vertices. The explorer and `rs2ool cache render --moving` play actions over a movement sequence this way. Converted clips carry the mix as moving bones and mix keys ([Anims](../pack/anims.md#moving-bones-and-mix-keys)).

## See also

- [Frame packs](../records/frame-pack.md): the byte layout of frames and bases.
- [Sequences](../records/seq.md): every `seq.dat` opcode.
- [Clips](../conversion/clips.md): turning these deformations into bone keys.
- [Validation](../conversion/validation.md): comparing converted clips with the reference deformation.
- [Explorer](../tools/explorer.md#playback): playing sequences in the GUI.
