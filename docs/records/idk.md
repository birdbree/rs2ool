[Home](../README.md) › [Records](README.md) › Idk

# Idk: identity-kit body parts

`idk.dat` holds identity-kit parts: the interchangeable pieces of a player's body, each a set of models for one body slot and body type. This page covers the opcodes, the fixed-size recolour arrays, and how the tools turn the parts into kit models and a default body.

Decoded by [`rs2cache::config::idk`](../../crates/rs2cache/src/config/idk.rs) into `IdkDef` ([`rs2cache::types::config`](../../crates/rs2cache/src/types/config.rs)). Cache data is big-endian.

## Storage

Idk is a **sequential** type: `idk.dat` is `count u16` followed by the definitions back to back, with no `.idx`. Reaching definition `n` means decoding every one before it. See [Config archive](../cache/config.md).

```text
rs2ool cache decode idk --all -o out/
```

## Opcodes

Each definition ends with opcode 0.

| Opcode | Reads | Meaning |
| --- | --- | --- |
| 1 | `u8` | `body_part`: 0–6 male, 7–13 female |
| 2 | `count u8`, then `model u16` × count | `models` |
| 3 | nothing | `selectable = false` |
| 40–45 | `u16` | Recolour source colour, slot `op − 40` |
| 50–55 | `u16` | Recolour target colour, slot `op − 50` |
| 60–64 | `u16` | `chat_head_models[op − 60]` |
| 46–49, 56–59, 65–69 | `u16` | Read, then recorded as unknown |

Any other opcode is logged, added to `unknown_opcodes`, and skipped without reading anything.

The opcodes in the last row are odd ones. The original client's opcode tests take whole ranges of ten (40–49, 50–59, 60–69), but its arrays have only six recolour slots and five chat-head slots. So these opcodes sit inside a range the client accepts, past the end of the array it would write to. The decoder consumes their `u16` so the stream stays aligned, then logs them and adds them to `unknown_opcodes` like any other unknown opcode.

## Defaults

`selectable` starts true. `body_part` is none, `models` is empty, every chat-head slot is none, and all twelve recolour slots are 0.

## Post-decode derivations

The recolours are held during decoding as two six-slot arrays, `from` and `to`, both starting at zero. After the stream ends they're paired slot by slot, stopping at the first slot whose `from` is 0. So:

- a pair after a zero `from` is dropped, even if it was set;
- a `to` of 0 is kept, as long as its `from` isn't 0;
- a `from` with no matching target opcode pairs with 0.

That's how the original client applies these arrays, and the result is an ordinary `recolours` list like every other config type.

## Fields

`IdkDef` holds: `id`, `body_part` (`Option<u8>`), `models`, `selectable`, `recolours`, `chat_head_models` (five optional model ids), and `unknown_opcodes` (omitted from JSON when empty).

## Body part, slot and body type

`body_part` packs two things into one number. Values 0–6 are the seven body slots for the male body; 7–13 are the same seven slots for the female body. The tools split it as:

```text
slot      = body_part % 7
body_type = body_part / 7     0 = male, 1 = female
```

## Conversion

`rs2ool pack player-kit` converts the parts with [`rs2convert::pipeline::player_kit`](../../crates/rs2convert/src/pipeline.rs):

- A definition with no `body_part` is skipped.
- Each remaining part's models are merged (vertex welding) and recoloured, and written as model record `0x2000_0000 | idk`.
- Each part's metadata (`idk`, `slot`, `body_type`, `hide_mask`, `selectable`) goes in the models pack manifest under `kit_parts`, keyed by the model record id. `hide_mask` is always 0: a kit part never hides another, as [Record ids](../pack/record-ids.md#manifest-extras) explains.
- The default male body takes, for each body part 0–6, the first selectable part in id order, and the default female body does the same for 7–13. Each body's parts are merged into one model, rigged with the player stance sequences, and written as a creature.

The explorer's kit view offers, for a slot and body type, every selectable part with that `slot` and `body_type`, and starts from the first of each. See [Creature conversion](../conversion/creatures.md) for the rig and clips, and [Models pack records](../pack/models.md) for the model records and manifest.

## See also

- [Config archive](../cache/config.md): sequential storage and the shared recolour pair.
- [Obj](obj.md): equipment models, which replace a kit part when an item fills the slot.
- [Model assembly](../client/assembly.md): how parts merge into one body.
- [Spotanim](spotanim.md): the other type with paired recolour arrays.
- [Pack record ids](../pack/record-ids.md): the kit-model id namespace.
