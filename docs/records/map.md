[Home](../README.md) › [Records](README.md) › Map regions

# Map regions

A map region is a 64 × 64-tile square of the world, stored as two files of index 4: a tile file with heights, floors and flags for four planes, and a location file listing the objects placed on it. This page covers how regions are found, both file layouts, and the decoded region.

Implemented in [`rs2cache::map`](../../crates/rs2cache/src/map.rs); the decoded structs are in [`rs2cache::types::map`](../../crates/rs2cache/src/types/map.rs). All integers are big-endian. What the original client does with the decoded tiles is on [terrain](../client/terrain.md).

## map_index and region ids

The versionlist archive's `map_index` entry says which index-4 files belong to which region. It's a run of 7-byte records, with any trailing partial record ignored:

```text
region        u16   x << 8 | y
tileFile      u16   index-4 file id of the tile file
locationFile  u16   index-4 file id of the location file
members       u8    1 = members region
```

The region id is the region's x and y, each 0–255. Region `(x, y)` covers world tiles `x * 64 .. x * 64 + 63` and `y * 64 .. y * 64 + 63`. Record type `map` uses the region id as its record id, and the ids are the regions `map_index` lists, sorted. On the command line you name a region as `RX_RY`, as in `--region 48_48`. See [versionlist](../cache/versionlist.md).

## The tile file

The tile file walks every tile of every plane: plane 0–3, then x 0–63, then y 0–63. Each tile is a run of opcodes that ends with opcode 0 or 1:

| Opcode | Reads | Meaning |
| --- | --- | --- |
| 0 | | end of tile; height derived |
| 1 | `height u8` | end of tile; explicit height |
| 2–49 | `overlay u8` | overlay id; shape = `(op − 2) / 4`, rotation = `(op − 2) & 3` |
| 50–81 | | flags = `op − 49` |
| 82–255 | | underlay = `op − 81` |

Opcodes 2–255 can appear in any order and number before the closing 0 or 1; a later one overwrites an earlier one. Shapes run 0–11 and rotations 0–3. Overlay and underlay are 0 for none, otherwise a [`flo`](flo.md) id plus 1.

Every grid in a decoded plane is 64 × 64, indexed `x * 64 + y`.

## Heights

Heights are corner heights in client units, where negative is up and a plane is 240 units tall. See [coordinates](../client/coordinates.md).

An **explicit** height (opcode 1) reads a byte `h`; a value of 1 is treated as 0. The height is

```text
height = below − h × 8
```

where `below` is the same tile's height on the plane underneath, or 0 on plane 0.

A **derived** height (opcode 0) is:

- on plane 0, the original client's terrain noise for the tile's world position (`region x * 64 + x`, `region y * 64 + y`), from `rs2cache::map::derived_height`;
- on planes 1–3, `below − 240`.

Each plane keeps a `height_derived` flag per tile. The noise function itself is on [terrain](../client/terrain.md).

## The location file

The location file lists placed objects grouped by location id. It's two levels of [unsigned smarts](../cache/primitives.md), each level ending at a 0:

```text
repeat:
  idDelta        smart       0 = end of file; id += idDelta (id starts at −1)
  repeat:
    posDelta     smart       0 = end of this id; pos += posDelta − 1 (pos starts at 0 for each id)
    attributes   u8
```

The packed position and the attribute byte split as:

```text
pos         plane << 12 | x << 6 | y      x and y are 0–63 within the region
attributes  type << 2 | rotation          type 0–22, rotation 0–3 (quarter turns)
```

Both levels use the unsigned smart (`b < 128 ? b : u16 − 32768`), not the signed one that models and frame packs use. Two smart forms share one name in a lot of code out there; go by the formula, not the name.

## Location types

A location's type picks which of a [`loc`](loc.md) definition's models it uses and how it's placed. The converter groups the 23 types into layers ([`rs2convert::object::layer_of`](../../crates/rs2convert/src/object.rs)):

| Types | Layer |
| --- | --- |
| 0–3, 9 | wall |
| 4–8 | wall decoration |
| 10, 11 | furniture |
| 12–21 | roof |
| 22 | floor decoration |

A definition whose models carry no type counts as type 10. Type 11 uses the type-10 model, rotated. Rotation and placement on the tile are on [terrain](../client/terrain.md).

## The decoded region

```text
Region
  id         u16      x << 8 | y
  x, y       u8
  members    bool     from map_index
  planes     [Plane; 4]
  locations  [Location]
Plane (each grid 64 × 64, indexed x * 64 + y)
  heights         [i32]
  height_derived  [bool]
  overlays        [u8]
  shapes          [u8]
  rotations       [u8]
  flags           [u8]
  underlays       [u8]
Location
  id, plane, x, y, kind, rotation
```

`kind` is the location type. Decoding errors are `MapError::Index`, `Tiles` or `Locations`, each wrapping the read that ran out of data.

## Dump

`rs2ool cache dump --region 48_48 --decode -o out/` (or `decode map`) writes `map/48_48.json`. Without `--decode`, it writes the two inflated files as `map/48_48.m.bin` (tiles) and `map/48_48.l.bin` (locations). The sidecar summary gives each plane's lowest and highest height and the location count.

## See also

- [Terrain](../client/terrain.md): derived-height noise, planes, the bridge flag, overlay shapes and location placement.
- [Versionlist](../cache/versionlist.md): `map_index` and the version tables.
- [Floors](flo.md) and [locations](loc.md): the definitions tiles and placements refer to.
- [Floor conversion](../conversion/floors.md) and [object conversion](../conversion/objects.md).
- [Records](README.md): every record type.
