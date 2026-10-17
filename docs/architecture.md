[Home](README.md) › Architecture

# Architecture

The workspace is five crates: four libraries and one binary. This page shows how data moves between them and the rules every crate follows, so you know where a change belongs before you make it.

## The crates

| Crate | Kind | What it does |
| --- | --- | --- |
| [`rs2cache`](crates/rs2cache.md) | library | Reads the cache: sectors, archives, every record decoder. Its `types` module holds the plain `serde` structs every decoder returns. |
| [`contentpack`](crates/contentpack.md) | library | The content-pack container, the encoder and decoder for every pack record, and the palette helpers. |
| [`rs2convert`](crates/rs2convert.md) | library | Turns decoded records into pack records: palette, models, creatures, rig inference, clip conversion, validation, objects, floors, textures, sprites. |
| [`rs2classic`](crates/rs2classic.md) | library | A reimplementation of the original client's software model renderer, drawing into a plain pixel buffer. |
| [`rs2ool`](crates/rs2ool.md) | binary + library | The `rs2ool` command line (`cache …` and `pack …`) and the explorer GUI. |

## How data flows

```text
                 cache directory (read-only)
                            │
                        rs2cache
          sectors → archives → decoders → rs2cache::types
                            │
       ┌──────────────┬─────┴──────┬────────────────┐
       │              │            │                │
 rs2ool cache     rs2convert   rs2classic    rs2ool explorer
 JSON, PNG,          │         pixel buffer   (Bevy + egui)
 manifests,      contentpack        │                │
 sidecars        records → packs    └──── drawn ─────┘
```

Every reader starts at `rs2cache`. A decoder takes bytes and returns a struct from `rs2cache::types`: a `Model`, a `FramePack`, a `Region`, a `LocDef` and so on. Those structs are the one shared vocabulary. The dump commands serialise them, the converter transforms them, the classic renderer draws them, and the explorer shows all three.

Conversion is one direction only: cache to packs. `contentpack` knows nothing about the cache. It defines the pack container and its records, and `rs2convert` is the bridge that fills them.

## Rules every crate follows

- **The cache is read-only.** `rs2cache::Cache` memory-maps the data file read-only. Nothing writes to a cache, repacks one, or downloads one.
- **Decoders never panic on bad input.** Every read is bounds-checked. A truncated or corrupt record returns a typed error naming where it failed. An unknown config opcode is logged and skipped, which is also what the original client does. See [Conventions](development/conventions.md).
- **Libraries stay small.** No library depends on `clap`, `bevy` or `egui`. File access is kept to a few named places: opening a cache, reading and writing packs and loose directories, and writing a conversion's output. Everything else takes bytes or structs and hands them back. The renderer draws into a `Vec<u32>`, not a window.
- **Output is deterministic.** Pack records are sorted by id, maps are ordered, nothing reads the clock, and parallel work is collected in order. Two runs on the same cache write byte-identical packs with the same names. See [Determinism](conversion/determinism.md).
- **The GUI has no decoders of its own.** The explorer's exports and conversions call the same library functions the CLI calls, so a file made from a button is byte-identical to one made from the command line. A test checks this.
- **The GUI never blocks.** Every cache read and conversion in the explorer runs as a task on Bevy's compute pool. UI systems only poll finished tasks.

## One binary

`rs2ool` is a single package holding both the command line and the explorer. The explorer calls the dump code, and the binary calls the explorer, so the two can't be separate crates without depending on each other. The explorer sits behind a default cargo feature named `explorer`, and `--no-default-features` builds the CLI without Bevy. See [rs2ool](crates/rs2ool.md).

## Where things go

| If you're changing… | Look in |
| --- | --- |
| How a cache record is read | `crates/rs2cache/src/` ([Records](records/README.md)) |
| A decoded struct's fields | `crates/rs2cache/src/types/` |
| A pack record's bytes | `crates/contentpack/src/records/` ([Packs](pack/README.md)) |
| How a record is converted | `crates/rs2convert/src/` ([Conversion](conversion/README.md)) |
| How the classic renderer draws | `crates/rs2classic/src/` ([Rasteriser](client/rasteriser.md)) |
| A command or flag | `src/cli/` and `src/cache/` |
| The GUI | `src/explorer/` |

Adding a record type touches a short, fixed list of places; [Conventions](development/conventions.md#adding-a-record-type) has it.

## See also

- [Crates](crates/README.md): the dependency graph and one page per crate.
- [Glossary](glossary.md): terms used across the crates.
- [Testing](development/testing.md): how each crate is tested.
