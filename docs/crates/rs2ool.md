[Home](../README.md) › [Crates](README.md) › rs2ool

# rs2ool

`rs2ool` is the workspace's one binary. It's also a library, which holds the `rs2ool cache` command code and, behind a cargo feature, the GUI explorer. This page covers how the crate is laid out and why. For using the commands, see the [tools section](../tools/README.md).

Sources: [`src/main.rs`](../../src/main.rs), [`src/lib.rs`](../../src/lib.rs), [`src/cli/`](../../src/cli/mod.rs), [`src/cache/`](../../src/cache/mod.rs), [`src/caption/`](../../src/caption/mod.rs), [`src/settings.rs`](../../src/settings.rs) and [`src/explorer/`](../../src/explorer/mod.rs).

## Layout

`rs2ool` is the workspace's root package, so its sources sit at the top of the repository and `cargo install --path .` works from there. The four libraries live in `crates/`.

```text
Cargo.toml       the workspace, and the rs2ool package
src/
  main.rs        the binary: argument parsing, logging, dispatch
  cli/           binary only: the clap definitions for `cache` and `pack`
  lib.rs         the library: `cache`, and with their features `caption`, `settings` and `explorer`
  cache/         list, dump, decode, verify, groups, xref, output layout
  caption/       sequence captioning, behind the `caption` feature
  settings.rs    settings.json, shared by the explorer and `cache caption`
  explorer/      the Bevy + egui GUI, behind the `explorer` feature
tests/           integration tests, CLI and explorer
crates/          rs2cache, contentpack, rs2convert, rs2classic
```

**The binary.** `main.rs` parses the command line with `clap`. `--cache <dir>` is a global option that falls back to the `RS2_CACHE` environment variable. There are two subcommand groups, `cache` and `pack`. Logging goes to stderr through `tracing-subscriber`, filtered by `RUST_LOG` and defaulting to `error`; the explorer sets up its own logging instead. Exit codes are 0 for success, 1 for a usage error, 2 for a cache or I/O error, and 3 for a partial failure.

**`cli`**, declared with `mod cli` in `main.rs` and so private to the binary, maps arguments onto library calls:

- `cli::cache` defines `list`, `dump`, `decode`, `info`, `verify`, `caption` and `explorer`, and calls into the library's `cache` module.
- `cli::pack` defines `inspect`, `materials`, `textures`, `npc`, `player-kit`, `sprites`, `floors`, `objects`, `models`, `build` and `explorer`.
- `cli::convert` holds the converting `pack` subcommands as thin wrappers over `rs2convert::run`. `pack build` calls `contentpack::loose::bake` directly.

**The `cache` module** is in the library, not the binary. Its option structs (`SourceArgs`, `IdArgs`, `OutArgs`, `DumpArgs`) describe a run without any `clap` types, and `dump::run`, `list::run` and `verify::run` do the work, returning an `Outcome` (`Ok` or `Partial`). The submodules: `dump` (dump and decode), `list`, `verify` (every sector chain, file, archive and versionlist entry), `groups` (dependency closures for `--group`), `xref` (reverse references, cached in `<out>/xref.json`), `select` (selector flags to ids), and `output` (files, sidecars and `manifest.json`).

## Why the explorer is a module

The explorer exports and converts through the same code the CLI runs. Its actions call `rs2ool::cache::dump::run`, `rs2convert::run` and `contentpack::loose` directly, so a dump or conversion started from the GUI writes files byte-identical to the command line's, and there's a test that checks it. For that to work the dump code has to be a library the explorer can call. The binary in turn opens the GUI by calling `explorer::app::run`.

Making the explorer a public library module also lets the headless tests in `tests/` add its plugins to a Bevy `App` under `MinimalPlugins` and drive them without a window. See [testing](../development/testing.md).

## The `explorer` feature

`explorer` is a default feature. It turns on the optional dependencies the GUI needs, pinned to exact versions:

| Dependency | Version |
| --- | --- |
| `bevy` | `=0.20.0` |
| `bevy_egui` | `=0.43.1` |
| `directories` | `=6.0.0` |
| `rfd` | `=0.17.2` |

Build with `--no-default-features` for a command-line-only binary without Bevy:

```text
cargo build -p rs2ool --no-default-features
```

Without the feature, the `explorer` module isn't compiled, the `rs2ool cache explorer` and `rs2ool pack explorer` subcommands don't exist, and the explorer tests compile to nothing (each starts with `#![cfg(feature = "explorer")]`).

## The `caption` feature

`caption` is a default feature too. It adds `rs2ool cache caption` and the `caption` module ([Captions](../tools/captions.md)), with two small dependencies and `directories`:

| Dependency | Version | Why |
| --- | --- | --- |
| `ureq` | `3`, without default features, with `json` | a blocking HTTP client, with no async runtime |
| `base64` | `0.23` | images in JSON request bodies |
| `directories` | `=6.0.0` | the settings file and the caption store |

The default build speaks plain HTTP only, which is all a server on this machine needs. `caption-tls` adds `ureq`'s rustls support for https endpoints.

The module has three parts:

- `caption::frames` draws a sequence as a grid of frames with `rs2classic`.
- `caption::client` talks to the server, in Ollama's API or the OpenAI-compatible one.
- `caption::store` keeps the captions, one JSON file per cache in the data directory.
- `caption::classify` turns reviewed captions into labels, measures them, and grows and scores a role tree for `cache classify`.

`caption::run` puts them together. It reports progress through a callback and takes the store behind a mutex, locked only to look up and add captions, so the explorer can run it as a job while the window reads and reviews the same store.

`settings` is compiled with either feature. It's the one `Settings` struct both read and write, with the explorer's output folder and the caption settings, so a change made in one applies to the other. With the `explorer` feature it's also a Bevy resource.

## Explorer module map

Every cache read and conversion runs on Bevy's `AsyncComputeTaskPool` through `jobs` or `sources`; UI systems only poll finished tasks.

| Module | What it does |
| --- | --- |
| [`actions`](../../src/explorer/actions.rs) | Export, dump, convert and bake through the same library calls the CLI makes, as jobs; remembers the output folder in `settings.json` (see `settings`). |
| [`app`](../../src/explorer/app.rs) | The window, and the startup options the command line sets (`--select`, `--compare`, `--classic`). |
| [`captions`](../../src/explorer/captions.rs) | With the `caption` feature: the Captions window (settings, connection test, the caption job for the selected NPC), caption tags in the sequence picker, and reviewing the chosen sequence's caption. The job and the window share one caption store. |
| [`classic`](../../src/explorer/classic.rs) | The classic renderer view: the selected model drawn by `rs2classic` into a pixel buffer, shown nearest-neighbour over the viewport. |
| [`inspector`](../../src/explorer/inspector.rs) | What the inspector shows (sidecar rows, decoded fields, hex dump), kept apart from egui so it can be tested. |
| [`jobs`](../../src/explorer/jobs.rs) | Background jobs on the `AsyncComputeTaskPool` (`Jobs::spawn_job`), with progress reported through atomics the bottom bar reads. |
| [`kit`](../../src/explorer/kit.rs) | The player kit: the default body from identity-kit parts with the stance sequences, dressed through the 12 equipment slots. |
| [`map_view`](../../src/explorer/map_view.rs) | Map regions as terrain: a mesh per plane from corner heights, tiles coloured from their floors, locations placed as their models. Neighbouring regions aren't stitched. |
| [`mesh`](../../src/explorer/mesh.rs) | Bevy meshes from decoded models and converted pack models, three vertices per face for hard normals. |
| [`playback`](../../src/explorer/playback.rs) | Animation playback: a frame clock driven by sequence delays (20 ms per cycle), shared by both compare panes so a cache creature and its conversion play in step. |
| [`renderable`](../../src/explorer/renderable.rs) | What the viewports draw, from any source: cache records as the original client assembles them, dumped model JSON, pack models, and 2D images. |
| [`selection`](../../src/explorer/selection.rs) | What is selected and its decoded form; selecting spawns a decode on the compute pool. |
| [`sources`](../../src/explorer/sources.rs) | Open sources (caches, dump directories, packs); opening runs as a task. |
| [`tree`](../../src/explorer/tree.rs) | The data behind the left-hand tree, kept apart from egui; id lists load on the compute pool when a node is first expanded. |
| [`ui`](../../src/explorer/ui.rs) | The egui layout: menu bar, tree, inspector, jobs and playback; publishes the remaining central rect for the cameras. |
| [`viewport2d`](../../src/explorer/viewport2d.rs) | Textures, sprites, fonts and interfaces painted with egui, nearest-filtered at an integer zoom, over a checkerboard. |
| [`viewport3d`](../../src/explorer/viewport3d.rs) | The world camera following the central rect, the orbit controller, the selected model and scale furniture. |

## See also

- [Tools](../tools/README.md): the command tree, `--cache` and exit codes.
- [Cache commands](../tools/cache-commands.md): `list`, `dump`, `decode`, `info` and `verify`.
- [Pack commands](../tools/pack-commands.md): every `rs2ool pack` subcommand.
- [Explorer](../tools/explorer.md): using the GUI.
- [Getting started](../getting-started.md): building with and without the feature.
