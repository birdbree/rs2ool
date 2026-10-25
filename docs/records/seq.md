[Home](../README.md) › [Records](README.md) › Sequences

# Sequences

A sequence is a definition in `seq.dat`: the list of frames an animation plays, how long each lasts, and how it loops and mixes with other animations. This page gives every opcode, the defaults, and how frame delays are resolved.

Implemented in [`rs2cache::config::seq`](../../crates/rs2cache/src/config/seq.rs); the decoded struct is `SeqDef` in [`rs2cache::types::config`](../../crates/rs2cache/src/types/config.rs). All integers are big-endian.

## Storage

`seq.dat` is an entry of the config archive (archive 2 of index 0) and is sequential: `count u16`, then the definitions back to back, each an opcode stream ending with opcode 0. A sequence's id is its position. There's no `seq.idx`, so decoding one sequence means decoding every one before it; `Records` decodes the whole table once and keeps it. See [config archives](../cache/config.md) for the shared rules.

## Opcodes

| Opcode | Reads | Meaning |
| --- | --- | --- |
| 0 | | end of definition |
| 1 | `count u8`, then per frame `{frame u16, secondary u16, delay u16}` | `frames`; a secondary of 65535 means none |
| 2 | `u16` | `loop_offset`; 65535 is kept as is |
| 3 | `count u8`, then `u8 × count` | `interleave_labels` |
| 4 | | `stretches = true` |
| 5 | `u8` | `priority` |
| 6 | `u16` | `offhand_override` |
| 7 | `u16` | `mainhand_override` |
| 8 | `u8` | `max_loops` |
| 9 | `u8` | `animating_precedence` |
| 10 | `u8` | `walking_precedence` |
| 11 | `u8` | `replay_mode` |
| 12 | 4 bytes | skipped |

Opcode 1 interleaves the three values per frame; it doesn't store three separate arrays.

Any other opcode is logged at `warn`, recorded in `unknown_opcodes`, and skipped without reading anything, which is what the original client does. If an unknown opcode did carry data, everything after it would be misread. The client would misread it too, so at least you'd be in good company.

The off-hand and main-hand overrides keep 65535 as read, as the original client does; only a frame's secondary frame turns 65535 into none. See [65535 as none](../cache/config.md#65535-as-none).

## Fields and defaults

| Field | Default | Notes |
| --- | --- | --- |
| `frames` | empty | one `SeqFrame` per frame |
| `loop_offset` | none | frames from the end to replay when looping |
| `interleave_labels` | none | which parts of the pose the creature's movement drives while this plays as it moves (see below) |
| `stretches` | false | |
| `priority` | 5 | |
| `offhand_override` | none | stored as read |
| `mainhand_override` | none | stored as read |
| `max_loops` | 99 | |
| `animating_precedence` | derived | see below |
| `walking_precedence` | derived | see below |
| `replay_mode` | 2 | |

Each `SeqFrame` is:

```text
SeqFrame
  frame      u16           global frame id
  secondary  optional u16  a second frame applied after `frame`, in interfaces
  delay      u16           length in client cycles of 20 ms; 0 = use the frame's own delay
```

`frame` and `secondary` are global frame ids, found through the [frame index](frame-pack.md#global-frame-ids), not index-2 file ids.

## Secondary frames

The original client only uses a secondary frame for a model drawn in an interface, which in practice means a chat head: it applies the frame in full, then the secondary frame in full. Creatures in the world play the primary frames only. In this cache 68 sequences have secondary frames, all of them chat-head expressions, and none of them has interleave labels. See [animation](../client/animation.md#secondary-frames).

## Interleave labels

These belong to a different feature from secondary frames, whatever the field names suggest. When a creature plays a sequence with interleave labels while it moves, the original client mixes the sequence's frame with the frame its movement sequence (walk, run or turn) is on: the movement drives the listed slots, the sequence the rest. Despite the name, the values are frame base *slot numbers*, not vertex labels. The original client walks the list in step with the slots, so it only works when the values are ascending. The decoder stores the bytes in the order read and doesn't interpret them.

In this cache 177 sequences have interleave labels, all with the same list: the player skeleton's whole-body translate and rotate and its leg rotations, so the walk carries the body and moves the legs while the sequence moves the rest. See [animation](../client/animation.md#interleaving).

## Derived precedences

Opcodes 9 and 10 set the two precedences explicitly. When either is absent, it's derived after the whole stream has been read, so opcode order doesn't matter:

- 2 if the sequence has interleave labels (opcode 3),
- 0 otherwise.

## Delays

A sequence frame's delay of 0 means "use the frame pack's delay for that frame". The clip converter resolves each frame's length in client cycles like this ([`rs2convert::clip::delays`](../../crates/rs2convert/src/clip.rs)):

1. the sequence frame's `delay`, if it isn't 0;
2. otherwise the `delay` byte of the frame in its [frame pack](frame-pack.md);
3. and never less than 1, so a frame with no delay anywhere, or one that can't be found, lasts one cycle.

How the original client steps through frames, loops and interleaves is on [animation](../client/animation.md).

## Dump

`rs2ool cache decode seq --id 1 -o out/` writes `seq/1.json`, the `SeqDef` with the fields above; `unknown_opcodes` appears only when it isn't empty. With `--meta`, the sidecar's summary holds the whole definition.

## See also

- [Frame packs](frame-pack.md): where frames and their default delays live.
- [Animation](../client/animation.md): sequence timing, loops and interleaving.
- [Config archives](../cache/config.md): sequential types and unknown opcodes.
- [Clips](../conversion/clips.md): how a sequence becomes a clip.
- [Spot animations](spotanim.md): definitions that refer to a sequence.
- [Inference](../conversion/inference.md): working out which NPCs a sequence is for, which the cache doesn't say.
