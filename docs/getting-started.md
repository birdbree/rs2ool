[Home](README.md) › Getting started

# Getting started

This page gets you from a fresh checkout to a dumped record, a converted pack and the explorer window. It assumes a stable Rust toolchain that supports edition 2024.

## You need a cache

The tools read a revision-317 cache directory: `main_file_cache.dat` plus `main_file_cache.idx0` through `main_file_cache.idx4`. No cache is included in the repository, so point the tools at your own copy of one. They only ever read it. Nothing in the workspace writes to a cache, repacks it or downloads one.

Every command that needs the cache takes `--cache <dir>`, or reads the `RS2_CACHE` environment variable when the flag is absent. Use an absolute path. `cargo test` runs each crate's tests from that crate's own directory, so a relative path that works in your shell breaks there.

```sh
export RS2_CACHE="/absolute/path/to/cache"
```

## Install

To get `rs2ool` on your `PATH`, install it from the checkout. The binary is the workspace's root package, so this works from the top of the repository:

```sh
cargo install --path . --locked
```

`--locked` builds with the committed `Cargo.lock`, which pins Bevy and everything else to the versions the tests ran against. You can also install straight from GitHub with `cargo install --git https://github.com/birdbree/rs2ool --locked rs2ool`.

## Build

To work on it rather than just use it:

```sh
cargo build --workspace
```

This builds the four libraries and the `rs2ool` binary, explorer included. The explorer pulls in Bevy, which takes a while the first time. Dependencies are optimised even in dev builds, because Bevy is painfully slow without that.

If you only want the command line, leave the explorer out:

```sh
cargo build -p rs2ool --no-default-features
cargo install --path . --locked --no-default-features
```

That build has no Bevy in it at all, and the two `explorer` subcommands don't exist.

## Check the cache first

```sh
cargo run -p rs2ool -- cache verify
```

`verify` follows every sector chain, inflates every file, decodes every archive and compares indices 1–4 against the versionlist. It exits 0 when nothing is damaged, and 3 when something is. Run it before trusting anything else: a truncated cache produces confusing errors further down. See [`rs2ool cache`](tools/cache-commands.md#verify).

## Look around

```sh
rs2ool cache list                      # the five indices and every archive
rs2ool cache list --type npc           # NPC ids, with names
rs2ool cache info --type model --id 0  # one record's metadata sidecar
```

(`rs2ool` here means the installed binary, or `cargo run --` from the checkout.)

## Dump and decode

```sh
rs2ool cache decode npc --id 1 -o out/
rs2ool cache dump --type model --id 0 --meta -o out/
rs2ool cache dump --type sprite --all -o out/
```

Decoded records land as JSON (or PNG for images) under `out/<type>/`, with a `manifest.json` describing every file written. [What a dump writes](tools/dump-output.md) has the full layout.

## Convert into packs

```sh
rs2ool pack materials -o packs/
rs2ool pack models --all -o packs/
rs2ool pack npc --id 1 --with-anims -o packs/
rs2ool pack inspect packs/models/<hash>.pack
```

Each run writes `packs/<kind>/<hash>.pack`, named by the SHA-256 of its contents. The same input always produces the same file. Creature conversions also write a `validation.json` comparing the converted animation against the original client's own deformation. See [`rs2ool pack`](tools/pack-commands.md) and [Conversion](conversion/README.md).

## Open the explorer

```sh
rs2ool cache explorer
rs2ool cache explorer --select npc:1 --compare
rs2ool pack explorer packs/models/<hash>.pack
```

The explorer browses a cache, dump directories and packs side by side. It renders models in 3D (with Bevy, or with a reimplementation of the original client's software renderer) and sprites, fonts and interfaces in 2D, and plays animations. Its export and convert actions run exactly the code the CLI runs. See [The explorer](tools/explorer.md).

## Run the tests

```sh
cargo test --workspace
```

Tests that need real data read `RS2_CACHE` and skip, rather than fail, when it isn't set. Everything else runs on synthetic inputs built inside the tests. See [Testing](development/testing.md).

## See also

- [Architecture](architecture.md): what each crate does and how they fit together.
- [Using rs2ool](tools/README.md): the whole command tree and the exit codes.
- [The cache](cache/README.md): the format the tools read.
