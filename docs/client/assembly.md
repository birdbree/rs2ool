[Home](../README.md) › [Client](README.md) › Assembly

# Assembly

How several cache models become the one model the original client draws: merging, recolouring, scaling, dressing a player from equipment slots, and shaping and placing objects. Read this to get the same vertices, labels and colours the client would have.

Implemented in [`rs2convert::creature`](../../crates/rs2convert/src/creature.rs), [`rs2convert::object`](../../crates/rs2convert/src/object.rs) and [`rs2convert::preview`](../../crates/rs2convert/src/preview.rs). The explorer's player kit is [`kit.rs`](../../src/explorer/kit.rs) and its map placement is [`map_view.rs`](../../src/explorer/map_view.rs).

## Merging models

An NPC, an identity-kit part or an item's equipment model lists several cache models. Merging them follows the original client:

1. Faces are appended model by model, in order, keeping their colours.
2. Each face corner is **welded**: if the merged model already has a vertex at exactly the same coordinates, the face uses that vertex; otherwise the vertex is appended. A welded vertex keeps the label of the first vertex that landed there, even when the later model labelled it differently.
3. Vertices that no face or texture triangle uses are never copied, so they drop out.
4. Texture triangles (the `[P, M, N]` triples) are welded the same way and appended after each model's faces.

Optional per-face sections exist in the merged model when any input has them, and models without one are filled with defaults:

| Section | Default for a model without it |
| --- | --- |
| face types | 0 |
| face priorities | the model's single priority |
| face alphas | 0 |
| face labels | 0 |
| vertex labels | 0 |

Per-face priorities are also created when every input uses a single priority but those priorities differ. When they all agree, the merged model keeps that one priority.

A textured face's type stores its texture triangle index in the bits above the low two. Merging shifts it: for a face whose type has the textured bit (value 2) set, `type + (offset << 2)` is stored (in 8-bit arithmetic), where `offset` is the number of texture triangles from earlier models.

A single model is used as it is, with no welding and no dropped vertices. Welding compares exact positions only, so two parts that merely touch keep separate vertices unless their coordinates match exactly. That's usually what the art intends, and occasionally not.

## Recolours

Definitions carry a recolour list of `(from, to)` pairs of 16-bit HSL colours (see [Colour](colour.md)). They apply in order, each one replacing every face colour equal to `from` with `to`. Order matters, because a later pair can match a colour an earlier pair produced: `5 → 7` followed by `7 → 9` turns colour 5 into 9.

## NPC scale

An NPC is its models merged, then recoloured. Its scale is `(scale_xz, scale_y, scale_xz)` over 128, so x and z share one value. The scale applies after any animation frame (see [Animation](animation.md#scale-after-animating)). The converted rest mesh, rig pivots and validation all start from the merged model with the scale baked in.

The model viewers assemble other records the same way:

| Record | Drawn as |
| --- | --- |
| NPC | models merged, recoloured, scaled `(scale_xz, scale_y, scale_xz)` |
| item | the inventory model, recoloured, scaled `(scale_x, scale_y, scale_z)` |
| identity kit | models merged and recoloured |
| spot animation | its model, recoloured, scaled `(scale_xy, scale_z, scale_xy)` |
| location | the type-10 model (or the first one), shaped, then recoloured |

## Equipment models

An item's equipment model for a body type comes from its three equipment model ids for that body type:

- No first model means no equipment model.
- The third model is only used together with the second.
- The models are merged, every vertex's y gets the item's offset for that body type added (when it isn't 0), and then the item's recolours apply.

## Equipment slots

A player is assembled from 12 slots. Each slot holds an item's equipment model when an item is worn there, or else an identity-kit part when the slot has one by default. The slot models merge in slot order.

Identity-kit records store a body part from 0 to 13. Body part `p` is kit body part `p % 7` for body type `p / 7` (0 male, 1 female).

| Slot | Name | Default kit body part |
| --- | --- | --- |
| 0 | head | none |
| 1 | cape | none |
| 2 | amulet | none |
| 3 | weapon | none |
| 4 | body | 2 (torso) |
| 5 | shield | none |
| 6 | arms | 3 (arms) |
| 7 | legs | 5 (legs) |
| 8 | hair | 0 (head) |
| 9 | hands | 4 (hands) |
| 10 | feet | 6 (feet) |
| 11 | jaw | 1 (jaw) |

The default appearance fills each kit-backed slot with the first selectable kit part of that body part and body type. The `player-kit` conversion builds the default male body the same way, from the first selectable part of each body part 0–6, and rigs it with seven fixed player stance sequences, since the cache doesn't say which sequences a player uses.

## Object shaping

A location definition shapes each of its models before placement, in this order:

1. **Mirror**, when the definition sets it: z is negated on every vertex, and each face swaps its first and third vertex so it still faces outward.
2. **Scale**: each axis multiplied by `scale / 128` (skipped when all three are 128).
3. **Offset**: the definition's x, y and z offsets are added.

Models can be typed (each paired with a placement type) or untyped. When every model of a definition is untyped, they merge into a single type-10 model, which is how the original client draws them; that model serves placement types 10 and 11, since type 11 is a rotated type 10. Otherwise each model keeps its own type, and an untyped model in a mixed list counts as type 10. To draw a placement of type `t`, the viewers pick the model of type `t` (10 for type 11), then apply the definition's recolours.

## Placement rotation

A placed location has a rotation of 0–3 quarter turns. Each step turns a vertex about the vertical axis:

```text
x' =  z
z' = −x
y unchanged
```

Steps are taken `rotation & 3` times. The region viewer then puts the model at the centre of the location's footprint, with the definition's width and length swapped for odd rotations:

```text
centre x = tile x · 128 + width · 64
centre z = tile y · 128 + length · 64
y        = the tile's ground height on the location's plane
```

## See also

- [Models](../records/model.md): the sections being merged.
- [Locations](../records/loc.md), [NPCs](../records/npc.md), [Items](../records/obj.md), [Identity kits](../records/idk.md): the definitions that drive assembly.
- [Creatures](../conversion/creatures.md): how assembled NPCs and the player kit are converted.
- [Objects](../conversion/objects.md): how shaped location models are converted.
- [Terrain](terrain.md): ground heights that placement reads.
