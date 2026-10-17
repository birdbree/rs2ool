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

## CI

[`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) runs on every pull request and every push to `main`, in three jobs:

- `fmt`: `cargo fmt --check`.
- `cli`: clippy and the tests built with `--no-default-features`, with no system libraries installed, which proves the command line builds without Bevy.
- `full`: Bevy's Linux libraries (ALSA, udev, Wayland, xkbcommon), then clippy with `-D warnings`, `cargo doc` with warnings as errors, and the full test suite.

CI has no cache, so the cache-gated tests skip there. Run them locally with `RS2_CACHE` set before you merge anything that touches a decoder or a converter.

## The GitHub wiki

The [wiki](https://github.com/birdbree/rs2ool/wiki) is generated from `docs/`, so edit the pages here, never there. [`scripts/wiki.py`](../../scripts/wiki.py) gives every page a unique wiki name, rewrites links between pages, points links to source files at GitHub, and adds a sidebar. A workflow republishes it whenever `docs/` changes on `main`. To check the output or publish by hand:

```text
python3 scripts/wiki.py build /tmp/wiki    # write the pages somewhere to look at
python3 scripts/wiki.py publish            # build, commit and push to the wiki
```

## See also

- [Crates](../crates/README.md): what each crate holds and how they depend on each other.
- [rs2ool](../crates/rs2ool.md): the `explorer` feature and `--no-default-features`.
- [Architecture](../architecture.md): how the pieces fit together.
