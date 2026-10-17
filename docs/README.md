# rs2ool documentation

rs2ool (say it "R.S. Tool") is a Rust workspace for reading the revision-317 RS2 game cache, the data files of the original Java client. It decodes every record in that cache, renders models the way the original client draws them, and converts records into a separate, documented content-pack format. Everything runs offline against a cache you supply. No cache is included in the repository; you point the tools at your own copy of one.

This wiki covers the tools and also the formats themselves. If you're here to understand the 317 cache, start at [The cache](cache/README.md) and [Records](records/README.md). If you just want to dump or convert something, start at [Getting started](getting-started.md).

## Start here

- [Getting started](getting-started.md): build the workspace, point it at a cache, run your first commands.
- [Architecture](architecture.md): the five crates, how data moves between them, and the rules they all follow.
- [Glossary](glossary.md): the vocabulary used everywhere else (smarts, frame bases, labels, regions, packs).

## The 317 cache

- [The cache](cache/README.md): the data file, the five indices, and what lives where.
  - [Primitives](cache/primitives.md): integers, smarts, strings and HSL colours.
  - [Data and index files](cache/data-and-index-files.md): sectors, chains and version trailers.
  - [Compression](cache/compression.md): gzip and headerless bzip2.
  - [Archives](cache/archives.md): jag archives and the entry-name hash.
  - [Versionlist](cache/versionlist.md): per-file versions, CRCs and `map_index`.
  - [Config](cache/config.md): the `.dat`/`.idx` definition tables and their opcode streams.
- [Records](records/README.md): one page per record type.
  - Geometry and animation: [models](records/model.md), [frame packs](records/frame-pack.md), [sequences](records/seq.md).
  - Definitions: [locations](records/loc.md), [NPCs](records/npc.md), [items](records/obj.md), [identity kit](records/idk.md), [spot animations](records/spotanim.md), [floors](records/flo.md), [variables](records/varp-varbit.md).
  - World: [map regions](records/map.md).
  - 2D: [sprites](records/sprite.md), [textures](records/texture.md), [fonts](records/font.md), [interfaces](records/interface.md).
  - Audio: [MIDI](records/midi.md).

## How the original client uses the data

- [Client behaviour](client/README.md): what the original Java client does with what it reads.
  - [Coordinates and units](client/coordinates.md)
  - [Colour](client/colour.md)
  - [Animation](client/animation.md)
  - [Model assembly](client/assembly.md)
  - [Lighting](client/lighting.md)
  - [Rasteriser](client/rasteriser.md)
  - [Terrain](client/terrain.md)

## The content-pack format

- [Packs](pack/README.md): what a pack is and the kinds it can hold.
  - [Container](pack/container.md) and [record ids](pack/record-ids.md).
  - Records: [models](pack/models.md), [materials](pack/materials.md), [textures](pack/textures.md), [sprites](pack/sprites.md), [objects](pack/objects.md), [kits](pack/kits.md), [strings](pack/strings.md), [rigs](pack/rigs.md), [anims](pack/anims.md).
- [Conversion](conversion/README.md): how cache records become pack records.
  - [Palette](conversion/palette.md), [models](conversion/models.md), [creatures](conversion/creatures.md), [rig inference](conversion/rig-inference.md), [clips](conversion/clips.md), [validation](conversion/validation.md), [objects](conversion/objects.md), [floors](conversion/floors.md), [textures and sprites](conversion/textures-and-sprites.md), [determinism](conversion/determinism.md).

## Tools and crates

- [Using rs2ool](tools/README.md): the one binary.
  - [`rs2ool cache`](tools/cache-commands.md) and [what a dump writes](tools/dump-output.md).
  - [`rs2ool pack`](tools/pack-commands.md).
  - [The explorer](tools/explorer.md).
- [Crates](crates/README.md): [rs2cache](crates/rs2cache.md), [contentpack](crates/contentpack.md), [rs2convert](crates/rs2convert.md), [rs2classic](crates/rs2classic.md), [rs2ool](crates/rs2ool.md).
- [Development](development/README.md): [testing](development/testing.md) and [conventions](development/conventions.md).
