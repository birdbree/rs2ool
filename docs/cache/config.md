[Home](../README.md) › [Cache](README.md) › Config

# Config

The `config` archive (index 0, archive 2) holds nine tables of definitions, each an entry pair or a single entry, and each definition an opcode stream. This page covers how definitions are found, how opcode streams are read, how unknown opcodes are handled, and the encodings several types share. The opcodes of each type are on its own record page.

All integers are big-endian. Implemented in [`rs2cache::config`](../../crates/rs2cache/src/config/mod.rs), one module per type, with the decoded structs in [`rs2cache::types::config`](../../crates/rs2cache/src/types/config.rs).

## The types

| Type | Entries read | Layout | Record page |
| --- | --- | --- | --- |
| `loc` | `loc.dat`, `loc.idx` | indexed | [loc](../records/loc.md) |
| `npc` | `npc.dat`, `npc.idx` | indexed | [npc](../records/npc.md) |
| `obj` | `obj.dat`, `obj.idx` | indexed | [obj](../records/obj.md) |
| `idk` | `idk.dat` | sequential | [idk](../records/idk.md) |
| `seq` | `seq.dat` | sequential | [seq](../records/seq.md) |
| `spotanim` | `spotanim.dat` | sequential | [spotanim](../records/spotanim.md) |
| `flo` | `flo.dat` | sequential | [flo](../records/flo.md) |
| `varp` | `varp.dat` | sequential | [varp and varbit](../records/varp-varbit.md) |
| `varbit` | `varbit.dat` | sequential | [varp and varbit](../records/varp-varbit.md) |

Ids run from 0 to `count − 1` in both layouts. A missing entry is a `MissingEntry` error.

## Indexed types

An indexed type has a `.idx` entry listing the length of every definition, so any one of them can be found without reading the others:

```text
.idx
  count        u16
  count × length u16

.dat
  count        u16
  definitions, back to back
```

Definition `n` starts in the `.dat` at offset `2 + length[0] + … + length[n − 1]`. The 2 skips the `.dat`'s own leading count.

### The count check

Both entries start with a count, and the reader requires them to agree. If the `.dat`'s count differs from the `.idx`'s, decoding fails with `CountMismatch`, naming both numbers, before any definition is read.

The lengths are only used to find start offsets. Each definition is still decoded until its terminating opcode 0, and the reader doesn't check that this lands exactly where the next definition begins.

## Sequential types

A sequential type has only a `.dat`:

```text
.dat
  count        u16
  definitions, back to back
```

Nothing records where each definition starts, so reaching definition `n` means decoding definitions 0 to `n − 1` first. The reader decodes the whole table whenever it needs any one definition of a sequential type.

## Opcode streams

Every definition, in either layout, is a stream of opcodes:

```text
repeat:
  opcode       u8
  operands     depend on the opcode and the type
until opcode == 0
```

Opcode 0 ends the definition and is consumed. Fields with no opcode in the stream keep their defaults (see each record page). Opcodes that only set a flag have no operands.

A definition that runs past the end of its entry is a `Definition` error naming the type, the id, and the read that failed.

### Unknown opcodes

An opcode a decoder doesn't know is:

1. logged at `warn` through `tracing`, with the type, id, opcode and the offset where it was read;
2. appended to the definition's `unknown_opcodes` list, in the order met (the field is left out of JSON output when empty);
3. skipped **without reading any operand bytes**.

The next byte is then read as an opcode. That's what the original client does too, which means an unknown opcode with operands throws the rest of the definition out of step. The warning tells you where it started.

There are two exceptions, both in types that store recolours in fixed six-slot arrays (`idk` and `spotanim`, see below). Opcodes that fall inside the original client's opcode range test but index past the end of its arrays still carry a `u16`: `idk` opcodes 46–49, 56–59 and 65–69, and `spotanim` opcodes 46–49 and 56–59. The reader consumes that `u16` and records the opcode as unknown.

Separately, a few opcodes are known but their operands are read and discarded, without being reported: `npc` 90–92 (a `u16` each), `obj` 10 (a `u16`) and `seq` 12 (4 bytes).

## Shared encodings

### Recolours

A recolour replaces one 16-bit HSL colour (see [Primitives](primitives.md#16-bit-hsl-colours)) with another on every face of a model that uses it. Every decoded type exposes them as a list of `{from, to}` pairs, but the cache stores them two ways.

**Recolour lists** (`loc`, `npc`, `obj`, opcode 40):

```text
count        u8
count × {
  from       u16
  to         u16
}
```

**Paired arrays** (`idk`, `spotanim`): six `from` slots set by opcodes 40–45 and six `to` slots set by opcodes 50–55, one `u16` each. Unset slots are 0. The reader pairs slot `i` of `from` with slot `i` of `to`, in slot order, and stops at the first slot whose `from` is 0, which is how the original client applies them. A recolour in slot 3 is lost if slot 1's `from` was never set.

How the original client applies recolours to a model is on [Assembly](../client/assembly.md).

### Morph tables

A morph table makes one definition stand in for one of several others, chosen at run time by a variable. `loc` uses opcode 77 and `npc` opcode 106:

```text
varbit       u16          65535 = none
varp         u16          65535 = none
count        u8
children     (count + 1) × u16, each 65535 = none
```

Note the `count + 1`: the stored count is one less than the number of children. The value of the varbit (or, if there is no varbit, the varp) indexes `children`. See [varp and varbit](../records/varp-varbit.md) for what those variables are.

### Action strings and "hidden"

Option strings for `loc` and `npc` (opcodes 30–34) and the ground options of `obj` (opcodes 30–34) are [strings](primitives.md#strings) filling five slots, indexed by `opcode − 30`. A string equal to `hidden`, in any case, means the slot is empty: it decodes as none, the same as a slot that was never set.

The inventory options of `obj` (opcodes 35–39) are read as plain strings, with no "hidden" check.

### 65535 as none

Some `u16` references use 65535 to mean "no value". The reader turns 65535 into none only in these fields:

- `loc` opcode 24, the animation;
- each frame's secondary frame id in `seq` opcode 1;
- the varbit, the varp and every child of a [morph table](#morph-tables).

Every other `u16` is kept as read, 65535 included. In particular, `seq` opcode 2 (the loop offset) is stored as read, which matches the original client.

## Entries present but unread

The config archive carries more entries than the original client reads. The reader's known-name table lists them, so they show up by name in `rs2ool cache list --archive config`, but nothing decodes them:

- the `.idx` entries of the sequential types: `idk.idx`, `flo.idx`, `seq.idx`, `spotanim.idx`, `varp.idx`, `varbit.idx`;
- `mesanim.dat`, `mesanim.idx`, `mes.dat`, `mes.idx`, `param.dat`, `param.idx`.

The sequential types really do have `.idx` entries sitting right there. The original client reads their `.dat` from the start anyway, and so does this reader.

## Errors

| Error | Raised when |
| --- | --- |
| `MissingEntry` | A `.dat` or `.idx` entry the type needs isn't in the archive |
| `Index` | A count or length can't be read |
| `CountMismatch` | The `.dat` and `.idx` counts of an indexed type differ |
| `Definition` | A definition runs past its data, or starts past the end of the `.dat` |
| `NoSuchDefinition` | The id asked for is at or past the type's count |

## See also

- [Archives](archives.md): how the `config` archive and its entries are read.
- [Primitives](primitives.md): the integer, string and HSL encodings the opcodes use.
- [Records](../records/README.md): every record type, with the opcode tables.
- [Glossary](../glossary.md): morph table, recolour, HSL colour.
