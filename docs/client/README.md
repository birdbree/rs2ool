[Home](../README.md) › Client

# How the original client uses the data

The record pages say what the cache stores. This section says what the original Java client does with it: which way the axes point, how a frame bends a model, how parts merge, how faces are lit and drawn, and how tiles become ground. Read it when you need output that matches the original vertex for vertex or pixel for pixel.

The behaviour described here is what the code in `crates/` reproduces, mainly [`rs2convert::deform`](../../crates/rs2convert/src/deform.rs), [`rs2convert::creature`](../../crates/rs2convert/src/creature.rs), [`rs2cache::map`](../../crates/rs2cache/src/map.rs) and the software renderer in [`rs2classic`](../crates/rs2classic.md).

## Pages

| Page | Covers |
| --- | --- |
| [Coordinates](coordinates.md) | axes and handedness, 128 units per tile and 240 per plane, 2048-per-turn tables and 8-bit frame angles, pack space |
| [Colour](colour.md) | 16-bit HSL colours, the HSL to RGB table, brightness, colour 0 as transparent |
| [Animation](animation.md) | applying a frame (origin, translate, rotate, scale, alpha), rotation order, implicit origins, sequence timing, loops, interleaving |
| [Assembly](assembly.md) | merging models and welding vertices, recolours, scale after animation, equipment slots, object shaping and placement rotation |
| [Lighting](lighting.md) | per-model lighting: flat and smooth faces, textured shades, ambient and contrast, the standard light setups |
| [Rasteriser](rasteriser.md) | projection, the back-face test, depth buckets, priority interleaving, the fillers, texture pages, alpha blending |
| [Terrain](terrain.md) | tile heights and the noise function, planes, underlay blending, ground lighting and shadows, overlay shapes, textured and invisible floors, tile flags |

## Where to start

If you're converting assets, read [Coordinates](coordinates.md) first. Almost every "the model is mirrored" or "the arm bends backwards" bug comes down to an axis sign. Then [Assembly](assembly.md) and [Animation](animation.md) cover everything between the raw records and a posed mesh.

If you're reproducing the original look, [Colour](colour.md), [Lighting](lighting.md) and [Rasteriser](rasteriser.md) follow a model from face colours to pixels.

## See also

- [Records](../records/README.md): the byte layouts this section builds on.
- [Conversion](../conversion/README.md): how this behaviour is carried into packs.
- [Explorer](../tools/explorer.md): the GUI that draws records with both the reference behaviour and the classic renderer.
