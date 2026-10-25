[Home](../README.md) › Tools

# Using rs2ool

`rs2ool` is the one binary in the workspace. `rs2ool cache …` reads a 317 cache, and `rs2ool pack …` converts its records into packs and works with packs. Both have an `explorer` subcommand that opens the GUI. This page is the map of commands; the pages below go through each group.

The command line is [`main.rs`](../../src/main.rs) and [`cli/`](../../src/cli/); the cache commands themselves live in the `rs2ool` library under [`cache/`](../../src/cache/), so the explorer runs the same code.

## Command tree

```text
rs2ool [--cache DIR] cache list      [--index N | --archive NAME | --type TYPE]
                         dump      SOURCE [IDS] [--entry NAME] [--raw] [--decode] -o DIR [OUTPUT]
                         decode    TYPE IDS -o DIR [OUTPUT]
                         info      --type TYPE --id N
                         verify
                         render    TYPE [--id N] [--female] [--seq [ID]] [--moving SEQ] [--frame N | --sheet [N]] [--compare] [CAMERA] -o FILE
                         structures (--region RX_RY | --all) [-o DIR]
                         caption   (--npc N | --all | --check | --import FILE --model NAME) [--dry-run] [--limit N] [--seq A,B,…] [--images DIR] [--redo] [SERVER]
                         classify  [--labels FILE] [--trust MODEL]… [--weak W] [--depth N] [--min-leaf W] [--verbose] [--save]
                         explorer  [--select TYPE:ID|kit] [--compare] [--classic] [--camera YAW,PITCH,DISTANCE[,X,Y,Z]] [--screenshot FILE] [--pack FILE]…

rs2ool [--cache DIR] pack  materials   -o DIR
                         textures    -o DIR
                         models      (--id N | --all) -o DIR
                         npc         --id N [--with-anims] [--seq A,B,…] [--infer] [--describe] [--override FILE] -o DIR
                         player-kit  [--with-anims] [--describe] [--override FILE] -o DIR
                         sprites     [--archive media|title] [--all] -o DIR
                         floors      -o DIR
                         objects     (--id N | --all) -o DIR
                         items       (--id N | --all) -o DIR
                         spotanims   (--id N | --all) -o DIR
                         inspect     PACK
                         build       DIR -o OUT
                         explorer    PACK… [--select TYPE:ID|kit] [--compare] [--classic]

SOURCE  = --index N | --archive NAME | --type TYPE | --region RX_RY | --all-regions | --group G
IDS     = --id N | --range A-B | --all
OUTPUT  = --meta | --meta-only, --xref, --jsonl
CAMERA  = --size N|WxH, --yaw A, --pitch A, --distance D, --background RRGGBB
SERVER  = --enable, --backend ollama|openai, --endpoint URL, --model NAME, --api-key-env VAR, --timeout SECS, --frames N, --prompt TEXT
```

`TYPE` is one of the 17 record type names: `model`, `anim`, `map`, `loc`, `npc`, `obj`, `idk`, `seq`, `spotanim`, `flo`, `varp`, `varbit`, `texture`, `sprite`, `font`, `interface`, `midi` (case doesn't matter). See [Records](../records/README.md) for what each one is.

`rs2ool --help`, and `--help` after any subcommand, lists the flags with a line each.

## Pointing at a cache

No cache is included in the repository; you point the tools at your own copy of one. Pass the directory that holds `main_file_cache.dat` and `main_file_cache.idx0`–`idx4` as `--cache DIR`, or set `RS2_CACHE`:

```text
export RS2_CACHE=/absolute/path/to/cache
rs2ool cache verify
```

`--cache` is global, so it can go before or after the subcommand, and it overrides `RS2_CACHE`. Use an absolute path when you set the variable, since tests run from each crate's own directory.

Every `cache` command except `explorer` and `caption --check` needs a cache, and so does every converting `pack` command. `pack inspect` and `pack build` don't read one. The explorers open without one; `pack explorer` opens the cache too when one is given, so compare mode can pair packs with their originals.

The tools only read the cache. They write to the output directory you name and nowhere else (the explorer also remembers its output folder, see [Explorer](explorer.md#the-actions-menu), and `caption` keeps its captions in your data directory, see [Captions](captions.md#where-captions-go)).

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | success, including `--help` and `--version` |
| 1 | usage error: a bad command line, no cache directory for a command that needs one, or a contradictory or incomplete selection in a `cache` command |
| 2 | cache or I/O error: the cache won't open, a record can't be read, an output can't be written; in `pack` commands, every error after the command line has parsed, except a missing cache |
| 3 | partial failure: some records failed (listed in `manifest.json` or the `verify` report), or a converted clip is over the validation threshold |

A partial failure still writes everything that worked. Scripts that only care about "did anything go wrong" can treat any non-zero code as failure; scripts that convert many records usually want to accept 3 and read the manifest.

If whatever's reading the output stops early, as with `rs2ool cache list | head`, the command stops writing and exits 0, the same as other Unix tools.

## Logging

Command-line runs log through `tracing` to standard error. Only errors show by default; set `RUST_LOG` (for example `RUST_LOG=warn`) to see more, such as the unknown opcodes decoders skip. The explorer sets up its own logging.

## Building without the explorer

The GUI is behind the `explorer` cargo feature, which is on by default. To build the command line without the GUI dependencies:

```text
cargo build -p rs2ool --no-default-features
```

That binary has no `cache explorer` or `pack explorer` subcommand, and no `cache caption`, which is behind the `caption` feature (also on by default). Everything else is the same. `--no-default-features --features caption` builds the command line with captioning and without the GUI.

## Pages

- [Examples](examples.md): one example per subcommand.
- [Cache commands](cache-commands.md): `list`, `dump`, `decode`, `info`, `verify` and `render`, every selector and flag.
- [Dump output](dump-output.md): where `dump` writes things, `manifest.json`, sidecars, `--xref`, groups.
- [Pack commands](pack-commands.md): every `rs2ool pack` subcommand, what it writes, and the validation reports.
- [Captions](captions.md): `cache caption`, captioning sequences with a vision model you run.
- [Explorer](explorer.md): the GUI.

## See also

- [Getting started](../getting-started.md): building and first commands.
- [rs2ool crate](../crates/rs2ool.md): the binary's library and its features.
- [Packs](../pack/README.md): what the `pack` commands write.
