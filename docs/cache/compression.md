[Home](../README.md) › [Cache](README.md) › Compression

# Compression

The 317 cache uses two codecs: gzip for the files of indices 1–4, and bzip2 with its stream header stripped inside jag archives. This page says how to recognise each and how the reader undoes them.

Integers in the archive header mentioned below are big-endian. Implemented in [`rs2cache::compress`](../../crates/rs2cache/src/compress.rs).

## gzip files (indices 1–4)

Model, frame pack, MIDI and map files are normally stored as a standard gzip stream followed by the file's 2-byte version (see [Data and index files](data-and-index-files.md#the-version-trailer)).

The reader decides by looking at the first two bytes:

```text
1f 8b    gzip magic: gunzip the stream
other    not compressed: use the bytes as stored
```

Gunzipping decodes the gzip stream and stops at its end, so the version trailer after it doesn't get in the way. A file without the magic is passed through unchanged, trailer and all. A stream that starts with the magic but fails to decode is an error (`CompressError::Gzip`).

Index 0 files are never put through this step. They go straight to the archive decoder.

## Headerless bzip2 (archives)

A normal bzip2 stream starts with a 4-byte header: `BZh` and a block-size digit. Jag archives store their bzip2 data **without** that header. The original client always compresses with the smallest block size (100 kB, digit `1`), so the reader puts `BZh1` back in front and hands the result to an ordinary bzip2 decoder.

```text
stored      bzip2 stream minus its first 4 bytes
decoded as  "BZh1" + stored
```

There is no magic to test for here. Whether an archive, or an entry within it, is bzip2'd is decided by the archive's size fields, never by peeking at the data. See [Archives](archives.md) for the two arrangements:

- **Whole-archive**: the archive's two header sizes differ, and everything after the header is one bzip2 stream.
- **Per-entry**: the sizes match, and each entry's data is its own bzip2 stream.

A corrupt stream is `CompressError::Bzip2`. In both arrangements the archive decoder then checks that the stream inflated to exactly the size its table promised; see [Archives](archives.md#size-checks).

If you're writing your own reader and get "not a bzip2 stream" errors on perfectly good archives, the missing header is almost always why.

## Errors

| Error | Raised when |
| --- | --- |
| `CompressError::Gzip` | A file with the gzip magic fails to gunzip |
| `CompressError::Bzip2` | A headerless bzip2 stream fails to decode |

Either one aborts the read of that file or archive. `rs2ool cache verify` counts such files in its *inflate-failed* column (see [Cache commands](../tools/cache-commands.md)).

## See also

- [Archives](archives.md): where bzip2 appears and how sizes are checked.
- [Data and index files](data-and-index-files.md): how the stored bytes are read before decompression.
- [Cache overview](README.md): which index holds what.
- [rs2cache](../crates/rs2cache.md): the error types around these codecs.
