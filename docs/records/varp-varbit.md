[Home](../README.md) › [Records](README.md) › Varp and varbit

# Varp and varbit: player variables and bit ranges

`varp.dat` defines player variables (varps), and `varbit.dat` defines named bit ranges inside them (varbits). Morph tables on [locs](loc.md) and [npcs](npc.md) pick their child by one of these. This page covers both decoders, which mostly keep values as read, because most of their opcodes have no known meaning.

Decoded by [`rs2cache::config::varp`](../../crates/rs2cache/src/config/varp.rs) and [`rs2cache::config::varbit`](../../crates/rs2cache/src/config/varbit.rs) into `VarpDef` and `VarbitDef` ([`rs2cache::types::config`](../../crates/rs2cache/src/types/config.rs)). Cache data is big-endian.

## Storage

Both are **sequential** types: the `.dat` is `count u16` followed by the definitions back to back, with no `.idx`. A definition's position in the stream is its id. See [Config archive](../cache/config.md).

```text
rs2ool cache decode varp --all -o out/
rs2ool cache decode varbit --all -o out/
```

## Varp opcodes

Each definition ends with opcode 0. An opcode not listed (9, for instance) is logged, added to `unknown_opcodes`, and skipped without reading anything.

| Opcode | Reads | Meaning |
| --- | --- | --- |
| 1 | `u8` | Kept as read in `op1` |
| 2 | `u8` | Kept as read in `op2` |
| 3 | nothing | `op3 = true`; the original client also lists such varps in a separate table |
| 4 | nothing | `op4 = true` |
| 5 | `u16` | `client_code`: selects client-side behaviour driven by this variable |
| 6 | nothing | `op6 = true` |
| 7 | `i32` | Kept as read in `op7` |
| 8 | nothing | `flagged = true` |
| 10 | `string` | Kept as read in `op10` |
| 11 | nothing | `flagged = true` (same field as opcode 8) |
| 12 | `i32` | Kept as read in `op12` |
| 13 | nothing | `op13 = true` |

Only opcode 5 has a known meaning. The rest are stored under their opcode numbers rather than given names nobody can back up. Opcodes 8 and 11 set the same flag, so the decoder folds them into one field.

### Varp defaults and fields

Every optional field starts as none and every flag as false. `VarpDef` holds: `id`, `client_code` (`Option<u16>`), `op1`, `op2` (`Option<u8>`), `op3`, `op4`, `op6` (`bool`), `op7` (`Option<i32>`), `flagged`, `op10` (`Option<String>`), `op12` (`Option<i32>`), `op13`, and `unknown_opcodes` (omitted from JSON when empty). There are no post-decode derivations.

## Varbit opcodes

| Opcode | Reads | Meaning |
| --- | --- | --- |
| 1 | `varp u16`, `lsb u8`, `msb u8` | The varp this varbit lives in, and its lowest and highest bit |
| 2 | nothing | `flags_varp = true`: marks the varp as flagged |
| 3 | `i32` | Kept as read in `op3` |
| 4 | `i32` | Kept as read in `op4` |
| 10 | `string` | Kept as read in `op10` |

Any other opcode is logged, recorded in `unknown_opcodes`, and skipped without reading anything.

### Varbit defaults and fields

`varp`, `lsb` and `msb` start at 0, `flags_varp` at false, and the rest as none. `VarbitDef` holds: `id`, `varp` (`u16`), `lsb`, `msb` (`u8`), `flags_varp`, `op3`, `op4` (`Option<i32>`), `op10` (`Option<String>`), and `unknown_opcodes` (omitted from JSON when empty). There are no post-decode derivations.

A varbit's value is the run of bits `lsb` through `msb` of its varp's 32-bit value, both ends included. The cache-gated tests check that every varbit has `lsb ≤ msb < 32` and names a varp that exists.

## Use in the tools

The tools decode, dump and inspect both types. In the explorer's inspector, a varbit's `varp` field links to that varp. A [morph table](../cache/config.md#morph-tables) names a varbit or a varp (65535 meaning none in either slot). Object conversion turns every child of a loc's morph table into a state rather than choosing one; see [Loc](loc.md) and [Object conversion](../conversion/objects.md). The explorer evaluates them: set a varp's contents beside a morph table and it shows the child the original client would pick ([Object states](../tools/explorer.md#object-states)).

## See also

- [Config archive](../cache/config.md): sequential storage and the morph-table encoding.
- [Loc](loc.md): morph tables on world objects.
- [Npc](npc.md): morph tables on npcs.
- [Glossary](../glossary.md): morph table and the other shared terms.
