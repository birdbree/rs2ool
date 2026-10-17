# rs2ool

rs2ool (say it "R.S. Tool") is a set of tools for reading the RS2 revision-317 cache, the data files of the original Java client. They decode every record in it, draw models the way the original client draws them, and convert records into a documented content-pack format. Everything runs offline. No cache is included in the repository, so point the tools at your own copy of one.

## Quick start

```sh
export RS2_CACHE=/absolute/path/to/cache   # holds main_file_cache.dat and .idx0–4
cargo build --workspace

cargo run -p rs2ool -- cache verify
cargo run -p rs2ool -- cache decode npc --id 1 -o out/
cargo run -p rs2ool -- pack models --all -o packs/
cargo run -p rs2ool -- cache explorer
```

`cargo build -p rs2ool --no-default-features` gets you the command line without the GUI (and without Bevy). [Getting started](docs/getting-started.md) walks through all of it.

## What's in here

| Crate | What it does | Docs |
| --- | --- | --- |
| `rs2cache` | Reads the cache: sectors, archives, and a decoder for every record type | [crate page](docs/crates/rs2cache.md) |
| `contentpack` | The content-pack container and its record encoders and decoders | [crate page](docs/crates/contentpack.md) |
| `rs2convert` | Turns cache records into pack records: palette, models, rigs, clips, validation | [crate page](docs/crates/rs2convert.md) |
| `rs2classic` | A reimplementation of the original client's software model renderer | [crate page](docs/crates/rs2classic.md) |
| `rs2ool` | The one binary: `rs2ool cache …`, `rs2ool pack …`, and the explorer GUI | [README](crates/rs2ool/README.md), [crate page](docs/crates/rs2ool.md) |

[Architecture](docs/architecture.md) shows how they fit together.

## Docs

The [docs wiki](docs/README.md) covers the tools and also the formats themselves:

- [The cache](docs/cache/README.md): the data file, indices, archives and config tables.
- [Records](docs/records/README.md): a page per record type, byte layouts included.
- [Client behaviour](docs/client/README.md): coordinates, colour, animation, lighting, the rasteriser and terrain, as the original client does them.
- [Packs](docs/pack/README.md) and [conversion](docs/conversion/README.md): the content-pack format and how records get there.
- [Using rs2ool](docs/tools/README.md): every command, what it writes, and the explorer.
- [Development](docs/development/README.md): testing and conventions.

API docs come from rustdoc: `cargo doc --workspace --no-deps --open`.

## Tests

```sh
cargo test --workspace
```

Unit tests build their inputs synthetically. Tests that need real data read `RS2_CACHE` and skip when it isn't set. See [Testing](docs/development/testing.md).

## Licence

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
