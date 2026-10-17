[Home](../README.md) › [Tools](README.md) › Explorer

# Explorer

The explorer is the GUI for browsing a cache, dump directories and packs side by side: decode a record, look at it in 3D or 2D, play its animations, compare it with its converted version, and run the same dumps and conversions as the command line. This page goes through the window and every control.

The code is the [`explorer`](../../src/explorer/) module of the `rs2ool` crate, built with the default `explorer` feature (see [Tools](README.md#building-without-the-explorer)). Every cache read, decode and conversion runs on a background task pool, so the window never waits on one; a spinner shows while something is loading.

## Opening it

```text
rs2ool cache explorer
rs2ool cache explorer --pack packs/models/<hash>.pack
rs2ool pack explorer packs/models/<hash>.pack packs/materials/<hash>.pack
```

`cache explorer` opens the cache from `--cache` or `RS2_CACHE` when one is given, plus any `--pack` files. `pack explorer` opens the packs you list (at least one) and the cache too when one is given. Startup options are under [Command-line startup](#command-line-startup).

The window has a menu bar (**View**, **Open…**, **Actions**), the source tree on the left, the inspector on the right, a bottom panel for object states, playback and jobs, and the viewport in the middle.

## Sources

The explorer can hold any number of sources at once. **Open…** offers three:

| Source | Pick | Notes |
| --- | --- | --- |
| Cache directory | a folder | the folder with `main_file_cache.dat` and the `.idx` files |
| Dump directory | a folder | a directory written by `rs2ool cache dump`; it must contain `manifest.json` |
| Pack | a `.pack` file | any pack kind |

Sources open in the background. One that fails to open shows its error in red under the tree.

## The tree and search

Each source is a collapsible node:

- A **cache** shows an *indices* node (files and present files per index), a **player kit** entry, and one node per record type.
- A **pack** lists its record ids.
- A **dump directory** lists the files in its `manifest.json`.

A list loads the first time you expand it. Above each list is a search box: a number matches that exact id, and anything else matches names containing it, ignoring case. The count above the list reads "N of M". Names come from the definition for `loc`, `npc` and `obj`, the archive entry for `sprite`, the font name for `font`, and `RX_RY` for `map`.

## The inspector

Selecting a row loads it and fills the inspector:

- **metadata**: for a cache record, its sidecar (see [Dump output](dump-output.md#sidecars)) as key and value rows; for a pack record, its kind, id (decimal and hex) and size; for a dump file, its path and size.
- **fields**: the decoded record, flattened into dotted keys. Long arrays show their first items and a total. A pack record shows a decoded summary.
- **bytes**: a hex view (offset, 16 bytes, ASCII) for MIDI records, pack records and dump files that aren't JSON.

Fields that refer to other records show their ids as links: an NPC's models and sequences, a location's models and animation, morph children, an item's models and noted or stack variants, a region's locations, an interface's children, models and animations, and so on. Clicking one selects that record and reveals it in the tree. **◀ Back** returns to the previous selection.

## The 3D viewport

Anything with a model draws in the 3D viewport:

- cache models, and definitions assembled the way the original client assembles them (see [Assembly](../client/assembly.md)): NPCs, items, identity kits, spot animations and locations;
- `model/<id>.json` files in a dump directory;
- records of a models pack, coloured with the palette from a materials pack written beside it (`<dir>/materials/`), or grey without one.

A new selection recentres the camera on it. A 128-unit grid, a 128 × 240 × 128 box (one tile, one storey) and axes sit at the origin so scale errors stand out (see [Coordinates](../client/coordinates.md)). The modern renderer can also colour vertices by label or by bone, and draw a converted creature's bones; see [Vertex colours and bones](#vertex-colours-and-bones).

### Controls

| Input | Does |
| --- | --- |
| left-drag | orbit; the drag has to start in the viewport, then may leave it |
| ← → | turn left and right |
| ↑ ↓ | tilt up and down |
| scroll | zoom in and out (mouse wheel or trackpad) |
| W, S | move the target forward and back along the ground, relative to the view |
| A, D | move the target left and right |
| Space | move the target up |
| left Shift | move the target down |

Flying moves at 0.4 of the orbit distance per second, so it's slow up close and fast far away. Keys do nothing while a text box has focus, and the mouse does nothing over the panels.

The pitch is limited to 85° above or below the horizon. Around a model, the target stays in a box around it with half its largest dimension to spare, and the zoom stays between a near limit and four times the model's size. In a map region the camera is bounded tighter (see [Maps and the world map](#maps-and-the-world-map)).

### Lighting

**View › Lighting** has two modes, for the modern renderer:

| Mode | What you see |
| --- | --- |
| 317 lighting (default) | the original client's flat directional term (ambient 64, contrast 850, light from (−30, −50, −30)) baked into each face's colour and drawn unlit, so the model looks the way the original client lights it |
| Engine lighting | the raw face colours under a directional light and ambient light, for judging shape |

Every face is flat-shaded in both. For pack models, 317 lighting takes each face normal back to cache space and applies the same term to the colour's lightness. See [Lighting](../client/lighting.md).

### Vertex colours and bones

**View › Vertex colours** picks what the modern renderer colours vertices by:

| Choice | What you see |
| --- | --- |
| Face colours (default) | each face's own colour |
| By vertex label | each vertex coloured by its label, which makes groups and welds easy to see; cache models and pack models alike |
| By bone | each vertex of a converted creature coloured by the bone it follows (its label's bone in the rig, or bone 0 when unbound), the way the skinning binds it; anything without a rig keeps its face colours |

**View › Bones** draws a converted creature's rig over the model: a dot on each bone's pivot and a line from it to its parent's pivot, each bone in the colour **By bone** paints its vertices. The bones are posed at the playback frame, with the same pose that skins the mesh, so they move with it, and at rest when nothing's playing. In compare mode they're drawn in whichever pane shows the converted creature. Cache models and NPCs have no rig, so they show no bones. It's the quickest way to check [rig inference](../conversion/rig-inference.md): a pivot in the wrong place or a bone hanging off the wrong parent shows up straight away.

## Compare mode

Tick **Compare original and converted** at the top of the inspector to see a record and its other version side by side. The viewport splits in two: the selection on the left, its match on the right.

The match is the same record in another open source:

| Selected | Shown beside it |
| --- | --- |
| a cache model | the models-pack record with the same id |
| a cache NPC | the models-pack record `0x10000000 \| id` (its converted creature) |
| a models-pack record | the cache model or NPC it came from |
| a dumped `model/<id>.json` | the cache model, or else the pack record |

So open the cache and the converted models pack together. When no open source has the other version, the inspector says so.

Both panes share one camera: orbit, zoom and fly once and both views follow. They also share one playback clock, so a cache NPC and its converted creature play the same sequence frame by frame, the cache side through the reference deformation and the converted side through its rig and clip. Choosing an object state applies to both panes. Compare mode always uses the modern renderer.

## The 2D viewport

Textures, sprites, fonts and interfaces from the cache, and sprite and texture-atlas records from packs, draw in the 2D viewport instead.

- **Zoom** buttons offer 1×, 2×, 3×, 4× and 8× (4× to start). Zoom is in physical pixels, so each texel covers exactly `zoom × zoom` screen pixels, and images are drawn with nearest filtering so they stay crisp.
- **Drag** anywhere to pan.
- A checkerboard shows through transparent pixels.
- A line of text gives the size: for a sheet, the sub-sprite's size, its offset on the canvas, the canvas size and the palette size.

**Sprites** are drawn trimmed, with the sheet's palette as swatches underneath. A sheet with more than one sub-sprite gets **◀ ▶** buttons and "sprite i of n"; a new selection starts at the first. **Textures** are drawn on their untrimmed canvas.

**Fonts** show all 256 glyphs as a 16 × 16 sheet with grid lines, and a **Metrics** panel listing every character's advance and x and y draw offsets.

**Interfaces** are laid out the way the original client nests them: each layer's children at their offsets, layers outlined, rectangles filled or outlined at their opacity, sprites from the media archive at their stored offsets, and text in a stand-in font with its colour, centring and shadow. Inventories, models, item text, tooltips and anything else draw as labelled boxes. Hovering a component outlines it and shows its id and size.

## Playback

When the selection can animate, the bottom panel shows a playback bar: a sequence picker, **Play**/**Pause**, a **Loop** box, a frame slider ("of N") and the position and length in milliseconds. Dragging the slider pauses playback.

What plays:

- a cache NPC whose sequences resolve: its stand and walk sequences, posed through the reference deformation with its scale applied afterwards;
- the player kit, with the player stance sequences;
- a converted creature: a models-pack record whose rig is in the rigs pack beside it (`<dir>/rigs/`), with its clips from `<dir>/anims/`.

Frames last their sequence delay in 20 ms cycles, as on [Animation](../client/animation.md#sequence-timing): the sequence's delay, else the frame's own, else 1. A converted creature's frame lengths come from its clip's frame start times. At the end the loop offset decides where playback restarts; with **Loop** off it stops on the last frame, and **Play** starts again from the beginning.

## Object states

A location with a morph table, or an object record in an objects pack, has states. The bottom panel shows a state picker and the chosen state's options:

- For a cache location the states are "default (loc N)" and then "loc C" for each morph child, each drawn as its own definition.
- For a pack object the states are "state 0", "state 1" and so on; models come from the models pack beside it and option names from the strings pack beside it.

A state that draws nothing says so.

## The player kit

Select **player kit** under a cache source to dress a player. The kit parts load once, in the background, and the inspector shows the controls:

- **male** / **female**, and **Reset** to go back to the default appearance;
- one row per equipment slot: its name; for slots an identity-kit part fills by default, a picker with "none" and every selectable part of that body part and body type; and an **item** id (0 for none).

An item id wins over the slot's kit part: the slot then shows the item's equipment model for the chosen body type. The dressed model rebuilds in the background after every change and plays the player stance sequences. The slots and their default kit parts are listed on [Assembly](../client/assembly.md#equipment-slots).

## Maps and the world map

Selecting a `map` record builds the region: each plane's ground and every placed location that has a model. The ground is blended, lit, shadowed, shaped and textured as the original client draws it, the same in the modern and classic views; [Terrain](../client/terrain.md#what-the-region-viewer-draws) lists what the viewer leaves out. An overlay in the corner of the viewport shows "region X_Y", **N**, **W**, **E** and **S** buttons that step to the neighbouring region (north is +y), and a checkbox for each of the four planes.

The camera starts above the middle of the region and is bounded to it: the target stays within the region's footprint, between plane 0's lowest ground and 3000 units above it, the zoom stays between 300 and 14000 units, and the pitch stays at least 0.08 radians above the horizon, so the eye never goes underground.

**View › World map** opens a window with plane 0 of every region at one pixel per tile, north up. The first time, it decodes every region, which takes a moment. The region on screen is outlined in yellow, and clicking a region opens it.

## The classic renderer

**View › Renderer** switches between **Modern** and **Classic**. The classic renderer is the [`rs2classic`](../crates/rs2classic.md) reimplementation of the original client's software model renderer, drawing into a pixel buffer that's scaled up with nearest filtering. **View › Classic pixel size** sets how many screen pixels each classic pixel covers, from 1× to 4× (2× to start). The modern renderer's field of view follows the classic projection, so switching keeps the same framing.

It draws cache models, assembled definitions, NPCs (relit at every animation frame), dumped models and map regions (terrain planes first, then objects from far to near, honouring the plane checkboxes). Textures come from the first open cache. Pack models have no classic form, and compare mode always uses the modern renderer, so switch back to Modern for those. See [Rasteriser](../client/rasteriser.md).

## The Actions menu

**Actions** runs dumps and conversions through the same library calls as the command line, so the files are byte-identical to the CLI's. Every action writes into the output folder at the top of the menu: choose it with **Choose output folder…**. The explorer remembers it between runs in `settings.json` in your platform's config directory.

| Action | Same as |
| --- | --- |
| Export TYPE ID (decoded) | `rs2ool cache dump --type TYPE --id ID --meta` |
| Export TYPE ID (file bytes) | `rs2ool cache dump --index N --id ID --meta`, for `model`, `anim` and `midi` |
| Dump every TYPE | `rs2ool cache dump --type TYPE --all --meta` |
| Convert … | the `rs2ool pack` command for the selection: a model, an NPC or the player kit (with animations), an object, the textures, the sprites, the floors |
| Convert every model / object | `rs2ool pack models --all`, `rs2ool pack objects --all` |
| Convert materials (palette) | `rs2ool pack materials` |
| Bake a loose directory… | `rs2ool pack build`, into the output folder |

Every action except baking needs the current selection to be in a cache source, and the export, dump and per-type convert actions need a cache record selected (the player kit counts for its own conversion). Jobs show in the bottom panel with a progress bar, then their result; a flagged clip is noted there. Each finished job offers **Open as source** for the packs or dump directory it wrote.

## Command-line startup

| Option | Effect |
| --- | --- |
| `--select TYPE:ID` | select a record in the cache once every startup source has opened, e.g. `npc:1` |
| `--select kit` | select the player kit |
| `--compare` | turn compare mode on (needs `--select`) |
| `--classic` | start in the classic renderer |
| `--pack FILE` | also open a pack; repeatable (`cache explorer` only; `pack explorer` takes packs as arguments) |

```text
rs2ool cache explorer --select npc:1 --compare --pack packs/models/<hash>.pack
rs2ool pack explorer packs/models/<hash>.pack --select model:0 --compare
rs2ool cache explorer --select map:12336 --classic
```

`--select` always selects in the cache, so with `pack explorer` it needs `--cache` or `RS2_CACHE`. A map id is `x << 8 | y` (12336 is region `48_48`).

## See also

- [Cache commands](cache-commands.md) and [Pack commands](pack-commands.md): what the Actions menu runs.
- [Assembly](../client/assembly.md) and [Animation](../client/animation.md): what the 3D viewport shows and plays.
- [Validation](../conversion/validation.md): the numbers behind compare mode.
- [rs2ool crate](../crates/rs2ool.md): the explorer module and the `explorer` feature.
