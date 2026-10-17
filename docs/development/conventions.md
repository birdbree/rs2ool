[Home](../README.md) › [Development](README.md) › Conventions

# Conventions

The rules the code follows: where things go, how errors work, what has to stay deterministic, and how to add a record type. Following them keeps the CLI and the explorer producing the same bytes, and keeps a bad cache from crashing anything.

## Where code goes

| You're writing… | It goes in |
| --- | --- |
| A cache reader or record decoder | `rs2cache`, one module per format |
| A decoded data structure | `rs2cache::types` (plain `serde` structs, no logic beyond small accessors) |
| A pack record layout | `contentpack::records`, with `encode` and `decode` |
| A cache → pack conversion | `rs2convert` |
| Drawing as the original client draws | `rs2classic` |
| Command code the explorer also runs | `rs2ool::cache` (library side) |
| Argument parsing | `rs2ool`'s `cli` module (binary side) |
| GUI | `rs2ool::explorer` |

The libraries have no `clap`, `bevy` or `egui` dependency, and `rs2classic` renders into a plain pixel buffer. Libraries log through `tracing`; only the binary installs a subscriber. See [crates](../crates/README.md) for the dependency graph.

## Reading cache bytes

Cache integers are big-endian, and decoders read them through one type: `rs2cache::io::Cursor`. It provides `u8`, `i8`, `u16`, `i16`, `u24`, `u32`, `i32`, `smart` (unsigned), `signed_smart`, and `string` (`\n`-terminated, Latin-1), and every read is bounds-checked and returns an `IoError` rather than panicking. Don't write another reader for a new decoder; add a method to `Cursor` if one is missing.

The only big-endian reads outside `Cursor` are the fixed-size headers in `rs2cache::cache` (6-byte index records, 8-byte sector headers) and the 2-byte version trailer in `rs2cache::versionlist`. Those read from slices whose length has already been checked.

The two smart forms are easy to mix up: `smart` is `b < 128 ? b : u16 − 32768`, and `signed_smart` is `b < 128 ? b − 64 : u16 − 49152`. Vertex, face-index and frame deltas use the signed form; map location deltas use the unsigned one. [Primitives](../cache/primitives.md) has the details.

Pack data, by contrast, is little-endian, read and written through `contentpack::bytes`.

## Errors and bad input

- **Decoders never panic on bad input.** Truncation, out-of-range indices, bad type bytes and size mismatches all come back as errors. A model face pointing past the vertex list is a `ModelError::FaceIndex`, not an index panic.
- **Unknown opcodes warn and skip.** A config decoder that meets an opcode it doesn't know logs it at `warn` through `tracing` (with the type, definition id, opcode and offset), records it in the definition's `unknown_opcodes`, and carries on without consuming any data, which is what the original client does.
- **One `thiserror` enum per module.** `CacheError`, `ArchiveError`, `ConfigError`, `ModelError`, `FrameError` and the rest each describe one module's failures; `rs2cache::Error` wraps them with context. `contentpack` has `PackError`, `RecordError` and `LooseError`, and `rs2convert` has `ConvertError`.
- **`anyhow` only in `rs2ool`.** The binary and its library modules use `anyhow` for context and reporting; no other crate depends on it.

## Files and the cache

Library code doesn't wander the filesystem. File access in the libraries is limited to:

- `rs2cache::Cache::open`, which memory-maps the data file read-only and reads the index files;
- `contentpack::PackReader::open` and `PackWriter::write_to`;
- `contentpack::loose`, which reads a loose directory and writes the baked pack;
- `rs2convert::run::write`, which writes an `Outcome` (packs, side files, validation reports) into a pack directory, with PNG encoding handed back to the caller.

Everything else takes bytes or decoded structs and returns them.

The cache is read-only, always. `Cache` maps it without write access, and no code path writes into the cache directory. Tests that need a damaged cache copy it to a temp directory first. No cache is included in the repository; you point the tools at your own copy.

## Deterministic output

The same input must produce the same bytes. In practice:

- Ordered maps (`BTreeMap`, `BTreeSet`) wherever iteration order can reach output. No `HashMap` iteration leaks into a file.
- Pack records are sorted by id, and manifests are serialised from ordered maps.
- Nothing reads the clock; there are no timestamps in packs or dumps.
- Parallel work (with `rayon`) either collects into an ordered `Vec` before results are combined, or writes one file per record and sorts the dump's `manifest.json` before writing it.
- Algorithms that could depend on seeding are made deterministic: the palette's k-means seeds from the heaviest colour, and its entries are sorted afterwards.

Packs are named by the SHA-256 of the whole file, so a determinism break shows up as a different file name. `tests/c_determinism.rs` checks this; see [testing](testing.md) and [determinism](../conversion/determinism.md).

## The explorer

**It never blocks the UI thread.** Every cache read, decode, conversion and export runs on Bevy's `AsyncComputeTaskPool`, through `Jobs::spawn_job` in `explorer::jobs` or the source-opening tasks in `explorer::sources`. UI systems only poll for finished tasks. If you add something slow, make it a job.

**It has no decoders or converters of its own.** It calls `rs2cache`, `rs2convert`, `contentpack` and `rs2ool::cache` exactly as the CLI does, so anything it writes is byte-identical to the CLI's output. `tests/d8_actions.rs` checks that for conversion and export. If the explorer needs a new view of the data, add it to a library and call it from both sides.

## Lints

`cargo clippy --workspace --all-targets -- -D warnings` must pass, and the code fixes what clippy flags rather than silencing it. There are no `#[allow(...)]` attributes in the workspace; keep it that way. `cargo fmt --check` must pass too.

## Adding a record type

A record type is wired in at a small, fixed set of places:

1. **`RecordType`**: one line in the `record_types!` list in [`types/record.rs`](../../crates/rs2cache/src/types/record.rs), giving the variant and its command-line name. `ALL`, `name()` and parsing follow from it.
2. **`Record`**: one variant holding the decoded struct (defined in `rs2cache::types`), plus its arm in `record_type()`.
3. **`TypeSummary`**: one arm for the sidecar summary, and its case in `record::summarize`. A config-style type can use the existing `Config` arm instead.
4. **The decoder**: a module in `rs2cache` (a `ConfigType` impl for a config type), with a `synth` encoder or inline test bytes, hooked into `Records` in [`record.rs`](../../crates/rs2cache/src/record.rs): its ids in `ids`, decoding in `decode`, and its stored files in `files`.
5. **The explorer tree node**: the tree lists every `RecordType::ALL` entry, so the node appears by itself; add a case to `explorer::tree::type_rows` if the rows should show names.

If the type dumps as something other than JSON (an image, a raw file), add its writer in `rs2ool`'s `cache::dump`. Then add a cache-gated test that decodes every record of the type, and document it under [records](../records/README.md).

## See also

- [Testing](testing.md): synthetic inputs, cache-gated tests and the determinism test.
- [rs2cache](../crates/rs2cache.md): `Records`, `RecordType` and the error types.
- [Architecture](../architecture.md): the design principles behind these rules.
- [Config archive](../cache/config.md): opcode streams and how unknown opcodes behave.
