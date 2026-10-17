[Home](../README.md) › [Records](README.md) › MIDI

# MIDI

Index 3 holds music as MIDI files. The tools read and inflate them but don't decode them, so this is the shortest page in the section.

Implemented in `Cache::midi` in [`rs2cache`](../../crates/rs2cache/src/lib.rs).

## Storage and ids

Each MIDI file is one file of index 3, and its record id is its file id. The id list is every file of index 3 that isn't absent. Reading one goes through the usual sector chain ([data and index files](../cache/data-and-index-files.md)), then inflates the bytes: gzip when the gzip magic is present, otherwise as stored ([compression](../cache/compression.md)).

The versionlist archive holds `midi_version` and `midi_crc` tables for index 3, and a `midi_index` of one byte per MIDI file; see [versionlist](../cache/versionlist.md).

## No decoding

The decoded record is the inflated bytes themselves (`Record::Midi(Vec<u8>)`). Nothing checks that they're valid MIDI, and there's no struct to describe.

## Dump

```sh
rs2ool cache decode midi --id 1 -o out/
```

This writes `midi/1.mid`, the inflated bytes. With `--meta`, the sidecar `midi/1.meta.json` records the stored file (sector chain, stored and inflated sizes, CRC32, compression) and a summary with the byte size.

`rs2ool cache dump --index 3 --id 1 -o out/` writes the same bytes as `idx3/1.bin`, or the stored bytes with `--raw`. See [dump output](../tools/dump-output.md).

## See also

- [Records](README.md): every record type.
- [Cache overview](../cache/README.md): what each index holds.
- [Compression](../cache/compression.md): how index files are inflated.
- [Cache commands](../tools/cache-commands.md): `dump` and `decode` flags.
