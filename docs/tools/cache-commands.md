[Home](../README.md) › [Tools](README.md) › Cache commands

# Cache commands

Every `rs2ool cache` subcommand with its selectors and flags, and the rules for combining them. The files `dump` writes are on [Dump output](dump-output.md).

The definitions are in [`cli/cache.rs`](../../crates/rs2ool/src/cli/cache.rs) and the commands in [`cache/`](../../crates/rs2ool/src/cache/). The examples assume `RS2_CACHE` is set (see [Tools](README.md#pointing-at-a-cache)).

## list

Prints what the cache holds. With no flag it prints a table of the five indices (`index`, `files`, `present`, `absent`), a blank line, then every archive of index 0 with its entry count and entry names.

```text
rs2ool cache list
rs2ool cache list --index 4
rs2ool cache list --archive config
rs2ool cache list --type npc
```

| Flag | Prints |
| --- | --- |
| `--index N` | `index N: K ids`, then `id` and size in bytes for every present file |
| `--archive NAME` | `archive N name: K entries` (plus `, compressed whole` when it is), then per entry its name, hash, unpacked size and stored size |
| `--type TYPE` | `TYPE: K records`, then each id, with a name for `loc`, `npc` and `obj` (the definition's name), `sprite` (its archive entry), `font` (its name) and `map` (`RX_RY`) |

Give at most one of the three. Archives can be named or numbered: `title` (1), `config` (2), `interface` (3), `media` (4), `versionlist` (5), `textures` (6), `wordenc` (7), `sounds` (8). Entries whose names aren't in the built-in table of known names show as `#hash`, the signed decimal name hash.

## dump

Extracts records to files, raw or decoded. A dump takes one **source**, usually some **ids**, an output directory, and optional flags.

### Source (exactly one)

| Flag | Takes |
| --- | --- |
| `--index N` | files of index 0–4, inflated (or decoded with `--decode`) |
| `--archive NAME` | an archive's entries, or its decoded record types with `--decode` |
| `--type TYPE` | decoded records of one type |
| `--region RX_RY` | one map region's tile and location files, e.g. `50_50` |
| `--all-regions` | every region in `map_index` |
| `--group G` | a record and what it depends on: `npc:<id>`, `item:<id>` or `player-kit` |

### Ids (at most one)

| Flag | Takes |
| --- | --- |
| `--id N` | one id |
| `--range A-B` | every id from A to B inclusive that the source has |
| `--all` | every id |

`--index` and `--type` need one of these. `--archive`, `--region`, `--all-regions` and `--group` choose their own records and refuse them. An id that turns out to be absent isn't an error: it's counted in the manifest's `absent` total. Indices have empty slots, and an empty slot is just that, not damage.

### Flags

| Flag | Effect |
| --- | --- |
| `--entry NAME` | with `--archive`, only this entry (a name, or `#hash`) |
| `--raw` | write stored bytes, still compressed, instead of inflated ones; for index 1–4 files that includes the 2-byte version trailer, and for an archive it writes the whole archive file |
| `--decode` | decode instead of writing bytes: `--index 1`, `2` and `3` decode as `model`, `anim` and `midi`; `--archive` decodes the record types the archive holds; `--region` and `--all-regions` decode the region |
| `-o, --out DIR` | output directory (required) |
| `--meta` | a `.meta.json` sidecar beside every output |
| `--meta-only` | only the sidecars |
| `--xref` | reverse references in sidecars, cached in `DIR/xref.json` |
| `--jsonl` | one `TYPE.jsonl` file per record type instead of a file per record |

`--type` and `--group` always decode, so `--decode` changes nothing there. With `--archive --decode`, the config archive decodes into `loc`, `npc`, `obj`, `idk`, `seq`, `spotanim`, `flo`, `varp` and `varbit`; `title` into fonts and title sprites; `interface`, `media` and `textures` into their types; and `versionlist` into one `versionlist.json`.

### Examples

```text
rs2ool cache dump --index 1 --all -o out/
rs2ool cache dump --index 2 --range 0-99 --decode -o out/
rs2ool cache dump --archive config --entry loc.dat -o out/
rs2ool cache dump --archive media --decode --meta -o out/
rs2ool cache dump --type model --range 0-99 --meta --xref -o out/
rs2ool cache dump --type sprite --all -o out/
rs2ool cache dump --type npc --all --jsonl -o out/
rs2ool cache dump --region 50_50 --decode -o out/
rs2ool cache dump --all-regions --raw -o out/
rs2ool cache dump --group npc:1 -o out/
```

### Contradictory selectors

The command line refuses combinations that can't mean one thing. All of these exit with code 1 without writing any records:

- no source, or more than one source;
- more than one of `--id`, `--range`, `--all`;
- `--index` or `--type` without any of them;
- `--id`, `--range` or `--all` with `--archive`, `--region`, `--all-regions` or `--group`;
- `--range A-B` with A greater than B, or not in `A-B` form;
- `--entry` without `--archive`;
- `--raw` with `--type`, `--group` or `--decode`;
- `--meta` together with `--meta-only`;
- `--entry` with `--archive … --decode` (decoding works on a whole archive);
- `--index 0 --decode` (index 0 holds archives: use `--archive NAME --decode`) and `--index 4 --decode` (index 4 files are per region: use `--region` or `--all-regions`);
- `--archive --decode` on an archive with no decoder (`wordenc`, `sounds`);
- an archive name or number that doesn't exist, an index outside 0–4, or an unknown group.

`--region` takes `RX_RY` with both parts from 0 to 255. Asking for an entry the archive doesn't have isn't a usage error; it's recorded as a failure in the manifest, and the run exits 3.

## decode

Shorthand for `dump --type TYPE`, with the type as a positional argument:

```text
rs2ool cache decode npc --id 1 -o out/
rs2ool cache decode model --all --meta -o out/
```

It takes the same ids (one of `--id`, `--range`, `--all`, required) and the same output flags (`-o`, `--meta`, `--meta-only`, `--xref`, `--jsonl`).

## info

Prints one record's metadata sidecar to standard output as pretty JSON, without writing files:

```text
rs2ool cache info --type model --id 0
```

Both `--type` and `--id` are required. The sidecar format is on [Dump output](dump-output.md#sidecars); `info` doesn't add cross-references.

## verify

Reads every file of every index and checks it:

```text
rs2ool cache verify
```

For each file it follows the sector chain and checks every header, inflates index 1–4 files, and decodes index 0 files as archives. Where the versionlist can be read, it compares each index 1–4 file's version trailer and CRC with the versionlist's tables (see [Versionlist](../cache/versionlist.md)).

The report is a table, one row per index:

```text
index	present	absent	truncated	bad-sector	inflate-failed	modified
```

Then, per index, the ids in each damaged class (the first 20, then a count), and the ids that differ from the versionlist. A file is *truncated* when its chain runs out of range or a sector is short, and *bad-sector* for any other chain fault. *Modified* files have a different version or CRC from the versionlist; that isn't damage, and it doesn't change the exit code. The last line is either `no damaged files` (exit 0) or `N damaged files` (exit 3). If the versionlist can't be read, the report says so and skips the comparison.

## explorer

`rs2ool cache explorer` opens the GUI on the cache. Its startup options are on [Explorer](explorer.md#command-line-startup).

## See also

- [Dump output](dump-output.md): the files, the manifest, sidecars, xref and groups.
- [Data and index files](../cache/data-and-index-files.md): the sector chains `verify` walks.
- [Archives](../cache/archives.md): entry names, hashes and compression.
- [Records](../records/README.md): what each record type decodes to.
