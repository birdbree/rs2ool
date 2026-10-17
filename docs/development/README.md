[Home](../README.md) › Development

# Development

This section is for working on the workspace itself: building, testing, linting, and the conventions the code follows. If you only want to run the tools, start with [getting started](../getting-started.md).

## Pages

- [Testing](testing.md): how the tests are organised, which ones need a cache, and the test tooling.
- [Conventions](conventions.md): error handling, determinism, where code goes, and how to add a record type.

## Commands

Run these from the repository root. The workspace builds on stable Rust (edition 2024).

Cache-gated tests read the cache from `RS2_CACHE`. Set it to an absolute path: `cargo test` runs each crate's tests from that crate's directory, so a relative path points somewhere else. No cache is included in the repository; point it at your own copy.

```text
export RS2_CACHE=/absolute/path/to/your/cache
```

Build everything, or just the command line without the GUI:

```text
cargo build --workspace
cargo build -p rs2ool --no-default-features
```

Run the tests: all of them, one crate's, or only those whose names match a filter:

```text
cargo test --workspace
cargo test -p rs2cache
cargo test -p rs2cache smart
```

The filter matches anywhere in a test's full path, so `smart` picks up both smart round-trip property tests in `rs2cache::io`.

Check formatting and lints. Clippy runs with warnings as errors, and the workspace sets clippy's `all` group to `warn`, so any clippy warning fails this:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
```

Review snapshot changes, when a test records `insta` snapshots:

```text
cargo insta review
```

`cargo insta` comes from the separate `cargo-insta` tool (`cargo install cargo-insta`). See [testing](testing.md) for where snapshots fit in.

Before you call a change done, `cargo fmt --check`, the clippy command above and `cargo test --workspace` should all pass. With `RS2_CACHE` set, the cache-gated tests run too; without it they print a skip note and pass.

## See also

- [Crates](../crates/README.md): what each crate holds and how they depend on each other.
- [rs2ool](../crates/rs2ool.md): the `explorer` feature and `--no-default-features`.
- [Architecture](../architecture.md): how the pieces fit together.
