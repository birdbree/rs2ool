[Home](../README.md) › [Records](README.md) › Frame packs

# Frame packs

A frame pack is one file of index 2: a frame base (a list of transform slots, each with the labels it moves) and any number of animation frames that use it. This page covers the byte layout, how the decoder expands each frame, and how a global frame id is found in the right pack.

Implemented in [`rs2cache::anim`](../../crates/rs2cache/src/anim.rs); the decoded structs are in [`rs2cache::types::anim`](../../crates/rs2cache/src/types/anim.rs). All integers are big-endian. How the original client applies a frame to a model is on [animation](../client/animation.md).

## Layout

The last 8 bytes of the inflated file are a trailer of four `u16` section lengths:

```text
L1  u16   length of section 1, not counting its leading frameCount
L2  u16   length of section 2
L3  u16   length of section 3
L4  u16   length of section 4
```

The sections start at offset 0 and run back to back. The frame base comes straight after section 4:

```text
section 1 (frame ids), at 0
  frameCount     u16
  per frame:
    frameId      u16
    slotCount    u8
section 2 (masks), at L1 + 2
  per frame, per slot 0..slotCount:
    mask         u8
section 3 (values), at L1 + 2 + L2
  per set mask bit, in frame then slot order:
    value        signed smart
section 4 (delays), at L1 + 2 + L2 + L3
  per frame:
    delay        u8
frame base, at L1 + 2 + L2 + L3 + L4
  count          u8
  kind           u8 × count
  per slot:
    labelCount   u8
    label        u8 × labelCount
trailer          8 bytes
```

Yes, L1 leaves out section 1's own leading count, so the masks start at `L1 + 2`. It catches everyone once.

The decoder reads the four sections in parallel, one frame at a time: an id and slot count from section 1, a delay from section 4, then a mask per slot from section 2 and the values those masks announce from section 3. If the frame base would start past the trailer, decoding fails with `SectionOverrun`; a frame whose `slotCount` exceeds the base's slot count fails with `SlotOutOfRange`.

## The frame base and transform kinds

The base is shared by every frame in the pack. Slot `i` has a kind and a list of labels, which refer to a [model](model.md)'s vertex labels (or face labels, for alpha):

| Kind | Name | What it does |
| --- | --- | --- |
| 0 | origin | sets the pivot to the centroid of the labelled vertices, plus the frame's offset |
| 1 | translate | moves the labelled vertices |
| 2 | rotate | rotates the labelled vertices about the pivot, by 8-bit angles |
| 3 | scale | scales the labelled vertices about the pivot, in 1/128 units |
| 5 | alpha | changes the alpha of the labelled faces |

Other kind values have no name in the decoder. Rotation angles, order and the sine tables are on [animation](../client/animation.md).

## Masks and default components

Each slot a frame covers has a mask byte. Bit 1 means a dx value follows in section 3, bit 2 a dy, bit 4 a dz, read in that order. A slot with mask 0 is skipped: the frame doesn't use it.

A component whose bit is clear takes a default: 0, except on scale slots, where it's 128 (scale by one). So a scale slot with only bit 2 set decodes as `(128, dy, 128)`.

## The implicit origin rule

The original client inserts an origin the frame didn't list, and the decoder does the same, so the decoded `transforms` are exactly what gets applied.

When a frame uses a slot that isn't an origin, the decoder looks back over the slots between the last slot the frame used (of any kind) and this one. If any of them is an origin, the nearest one is inserted first, with offsets `(0, 0, 0)`. At most one origin is inserted per transform, and a frame's first transform looks back to slot 0. An origin slot the frame lists itself is decoded like any other slot.

For example, with a base `origin, rotate` and a frame that only sets the rotate slot, the decoded transforms are `origin (0, 0, 0)` then `rotate`.

## Decoded structs

```text
FramePack
  base      [BaseTransform { kind: u8, labels: [u8] }]
  frames    [Frame]
Frame
  id          u16    global frame id
  delay       u8     default length in client cycles
  transforms  [FrameTransform { transform: u8, dx, dy, dz: i32 }]   expanded, in order
  alpha       bool   whether any transform is an alpha transform
```

`transform` indexes `base`. The dump writes a pack to `anim/<id>.json`, keyed by file id; its sidecar lists the frame ids, the base's slot count and the distinct labels the base uses.

A frame's delay is what a [sequence](seq.md) falls back to when its own delay for that frame is 0.

## Global frame ids

Frame ids are global 16-bit ids, and they're what [`seq.dat`](seq.md) refers to. The id doesn't say which pack holds the frame: there are no pack and index bits packed into it, and nothing in the cache stores the frame → pack map. The original client loads every pack at startup and so never needs one.

The decoder builds the map by scanning. `FrameIndex::build` reads only section 1 of each pack (`frame_ids`), and records which file defines each id. If more than one pack defines an id, the last one wins in `pack_of`, and every pack that defines it is listed in `duplicates`. `Cache::frame_index` does this over every present file of index 2, skipping absent ones; `pack_for(frame)` then looks up one id.

The versionlist's `anim_index` is **not** this map. It's one `u16` per global frame id, and the original client uses only its length. See [versionlist](../cache/versionlist.md).

## Strict and non-strict decoding

`decode` (used by `Cache::frame_pack`, `Records` and the dump) reads as the original client does: it trusts the section start offsets and doesn't check where each section ends.

`decode_with(data, true)` adds a check that every section was read to exactly its stated length: section 1 ends where the masks start, the masks where the values start, the values where the delays start, the delays where the base starts, and the base where the trailer starts. A mismatch fails with `SectionLength`, naming the section (1–5, the base being 5). Use it when you want to prove the layout, not just read frames.

## See also

- [Animation](../client/animation.md): applying a frame, rotation order, implicit origins and timing.
- [Sequences](seq.md): the frame lists that refer to global frame ids.
- [Models](model.md): the vertex and face labels the base refers to.
- [Versionlist](../cache/versionlist.md): `anim_index` and the per-file tables.
- [Rig inference](../conversion/rig-inference.md) and [clips](../conversion/clips.md): how frames become rigs and clips.
- [Inference](../conversion/inference.md): frame-base families, and how they narrow which sequences fit a model.
