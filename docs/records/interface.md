[Home](../README.md) › [Records](README.md) › Interfaces

# Interfaces

Interface components are the 2D widgets the original client builds its screens from: layers, rectangles, text, sprites, models and item grids. All of them are stored in one archive entry. This page gives the full byte layout and the tree the dump builds from it.

Implemented in [`rs2cache::interface`](../../crates/rs2cache/src/interface.rs), with the types in [`rs2cache::types::interface`](../../crates/rs2cache/src/types/interface.rs). The tree output is written by [`rs2ool`'s dump](../../src/cache/dump.rs).

## The `data` entry

Every component is in entry `data` of the interface archive (archive 3). All integers are big-endian; `string` is `\n`-terminated (see [primitives](../cache/primitives.md)).

```text
data
  u16   count          size of the original client's component table
  component …          back to back until the entry ends
```

There's no per-component length and no end marker: the decoder reads components until the bytes run out. The count isn't used to stop. Running out of bytes in the middle of a component is an error that names the component id and the offset it started at.

Each component starts with its id. The value 65535 in that position is a marker instead: it introduces a new root, and is followed by the root's id and then the component's own id.

```text
u16   id
if id == 65535:
  u16   parent         root that this and later components belong to
  u16   id
```

The parent stays in force for every component after the marker until the next one. Components before the first marker have no parent. A root is a component whose parent is its own id. The record id of an interface is its component id.

## Common header

```text
u8      type             selects the type-specific block
u8      action type
u16     content type
u16     width
u16     height
u8      alpha
opt16   hover popup
u8      condition count
  per condition:
  u8      op
  u16     value
u8      script count
  per script:
  u16     length n
  u16 × n words
type-specific block      see below
spell block              if action type == 2 or type == 2
string  text             type 8 only: the type 8 block
string  tooltip          if action type is 1, 4, 5 or 6
```

`opt16` is a packed optional `u16`, used for the hover popup and in the model block: one byte `hi`, and if `hi` is 0 the value is absent and nothing more is read. Otherwise one more byte `lo` follows and the value is `(hi − 1) << 8 | lo`. It saves a byte when the value is absent, and can't express anything above 65279.

Flags in this format are a `u8` where 1 means true and anything else false.

### Conditions and scripts

Condition `i` is paired with script `i`: the original client runs the script and compares its result with `value` using `op`, and the component shows its enabled state (enabled text, colours, sprite or model) when every condition holds. A script is a list of `u16` opcode and operand words. The decoder keeps both exactly as stored and doesn't interpret them.

## Type-specific blocks

A text style, used by several types, is three bytes:

```text
u8   centred      flag
u8   font         0 p11_full, 1 p12_full, 2 b12_full, 3 q8_full
u8   shadow       flag
```

### Type 0: layer

```text
u16   scroll max
u8    hidden       flag
u16   child count n
  per child:
  u16   id
  i16   x          offset from the layer's corner
  i16   y
```

### Type 1

```text
u16          a
u8           b
text style
i32          colour
```

The original client reads this block and never uses it. The decoder keeps the fields under neutral names.

### Type 2: inventory

```text
u8    swappable     flag
u8    inventory     flag
u8    usable        flag
u8    replaceable   flag
u8    pad x
u8    pad y
20 × slot sprite:
  u8      present   flag; if 1:
  i16     x
  i16     y
  string  sprite    name,index reference
5 × string action   empty means none
```

The grid has `width × height` slots.

### Type 3: rectangle

```text
u8    filled                flag
i32   colour
i32   enabled colour
i32   hover colour
i32   enabled hover colour
```

### Type 4: text

```text
text style
string   text
string   enabled text
i32      colour
i32      enabled colour
i32      hover colour
i32      enabled hover colour
```

### Type 5: sprite

```text
string   sprite           name,index reference
string   enabled sprite   name,index reference
```

### Type 6: model

```text
opt16   model
opt16   enabled model
opt16   animation
opt16   enabled animation
u16     zoom
u16     rotation x
u16     rotation y
```

### Type 7: item text

```text
text style
i32        colour
i16        pad x
i16        pad y
u8         inventory    flag
5 × string action       empty means none
```

### Type 8: tooltip

No block here. Its one `string`, the tooltip text, comes after the spell block instead. That ordering is easy to miss: read it in the type-specific position and every following component is garbage.

Any other type has no type-specific block.

## Spell block

Read when the action type is 2, or when the component type is 2 (inventory):

```text
string   selected action
string   spell name
u16      spell usable on
```

## Tooltip

Read when the action type is 1, 4, 5 or 6. An empty string is replaced by a default:

| Action type | Default tooltip |
| --- | --- |
| 1 | `Ok` |
| 4, 5 | `Select` |
| 6 | `Continue` |

For other action types there's no tooltip and nothing is read.

## Sprite references

Sprites are named by a string `name,index`: sub-sprite `index` of entry `<name>.dat` in the media archive. The string is split at its last comma and `index` must parse as a number; an empty string, a string without a comma, or a non-numeric index gives no sprite. See [sprites](sprite.md).

## Tree output

```text
rs2ool cache decode interface --all -o out/
```

writes one `interface/<id>.json` per component and, when more than one component was dumped, `interface/tree.json`. The tree is a list of every root (a component whose parent is itself) in id order. Each node is the component's JSON. A layer's node also gets a top-level `children` array (beside the plain list in `content.children`) holding the child nodes, each with the child's `x` and `y` from the layer added. A child id that wasn't decoded appears as `{"id": …, "missing": true}`. Nesting stops at depth 32, which only a malformed cache would reach.

In the JSON, the type-specific block is the `content` object, tagged by a `type` field (`layer`, `unused`, `inventory`, `rectangle`, `text`, `sprite`, `model`, `item_text`, `tooltip`, `other`). With `--meta`, the sidecar's summary gives the type, size and child count. See [dump output](../tools/dump-output.md).

## See also

- [Sprites](sprite.md): what a `name,index` reference points at.
- [Fonts](font.md): the four fonts a text style selects.
- [Models](model.md): the model a type 6 component shows.
- [Dump output](../tools/dump-output.md): `interface/tree.json` alongside the other outputs.
- [Jag archives](../cache/archives.md): the interface archive.
