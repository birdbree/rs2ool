[Home](../README.md) › [Crates](README.md) › contentpack

# contentpack

`contentpack` writes and reads packs: the container, the encoder and decoder for every record kind, the HSL colour helpers, and loose record directories. This page covers the API; the byte layouts are in the [pack section](../pack/README.md).

Entry point: [`crates/contentpack/src/lib.rs`](../../crates/contentpack/src/lib.rs). Everything in a pack is little-endian. The crate has no dependency on `rs2cache`.

## Module map

| Module | What it holds |
| --- | --- |
| [`pack`](../../crates/contentpack/src/pack.rs) | `PackWriter`, `PackReader`, `Kind`, `Codec`, `PackManifest`, `PackError`. Re-exported at the crate root. |
| [`records`](../../crates/contentpack/src/records/mod.rs) | One module per record kind, each with `encode`/`decode`, and the shared `RecordError`. |
| [`palette`](../../crates/contentpack/src/palette.rs) | 16-bit HSL to RGB, brightness, colour distance, nearest-colour lookup and weighted k-means. |
| [`loose`](../../crates/contentpack/src/loose.rs) | Loose record directories and baking them into a pack. |
| [`bytes`](../../crates/contentpack/src/bytes.rs) | The little-endian `Writer` and `Reader` every record uses. |

## `PackWriter`

`PackWriter::new(kind, source_cache_crc)` starts an empty pack of one `Kind`. Records go in with `insert(id, bytes)`, which replaces any record already at that id. `set_extra(key, value)` attaches a kind-specific JSON table to the manifest. `with_codec(codec)` picks the per-record codec; the default is `Codec::Zstd` (level 9).

- `bake()` returns `(name, bytes)`: the pack's name, which is the lowercase hex SHA-256 of the whole file, and the file's bytes.
- `write_to(dir)` bakes and writes `<dir>/<kind>/<name>.pack`, creating directories as needed, and returns the path.
- `kind()`, `len()` and `is_empty()` do what they say.

Records live in a `BTreeMap` and so do the manifest extras, so baking is deterministic: the same records and extras give the same bytes and name, whatever order you inserted them in. Nothing reads the clock.

## `PackReader`

`PackReader::open(path)` or `PackReader::from_bytes(bytes)` parses a pack. It checks the `RS2P` magic, the container version (1), the kind number and the codec, then parses the JSON manifest and the index, and recomputes the SHA-256 name into `hash`.

| Item | What it gives you |
| --- | --- |
| `manifest` | The `PackManifest`: `kind`, `record_count`, `source_cache_crc`, `codec`, and `extra`. |
| `index` | The index entries (`id`, `offset`, `length`, `crc`), sorted by id. |
| `hash` | The recomputed name. Compare it with the file name if you want to know the file wasn't altered. |
| `ids()` | Record ids in ascending order. |
| `contains(id)` | Whether a record exists, without reading it. |
| `get(id)` | `None` if absent, otherwise the decompressed record, checked against the index's CRC32. |

A record whose stored bytes run past the blob, fail to decompress, or fail the CRC check is a `PackError` from `get`, not a panic.

## `Kind` and `Codec`

`Kind` is the pack kind, stored as a `u8` and serialised in lowercase: `Models` = 1, `Materials` = 2, `Textures` = 3, `Sprites` = 4, `Objects` = 5, `Kits` = 6, `Rigs` = 7, `Items` = 8, `Anims` = 9, `Fluids` = 10, `Strings` = 11. `Kind::ALL` lists them and `name()` gives the directory name `write_to` uses. Kinds 8 and 10 are reserved numbers; no record module exists for them.

`Codec` is `None` = 0 or `Zstd` = 1, and applies to each record separately.

## Records

Each module under `records` has a Rust type and a pair of functions. `encode` returns the bytes (as `Result<Vec<u8>, RecordError>` where the type has invariants to check, plain `Vec<u8>` where it doesn't), and `decode` parses them back, rejecting leftover bytes.

| Module | Types and helpers | Pack kind |
| --- | --- | --- |
| `records::model` | `PackModel`, `AttachPoint`, `flags` (`UV`, `ALPHA`, `PRIORITY`, `LABELS`, `FACE_GROUPS`) | models |
| `records::rig` | `Rig`, `Bone`, `NO_PARENT`, `UNBOUND` | rigs |
| `records::anim` | `Clip`, `Key`, `Fade` (with `Fade::apply`), `NO_LOOP` | anims |
| `records::material` | `Material`, `mode`, `flags`, `PALETTE_ID`, `encode_palette`, `decode_palette` | materials |
| `records::texture` | `TextureRect`, `Atlas`, `ATLAS_BASE` | textures |
| `records::sprite` | `SpriteRecord`, `FontMetrics` | sprites |
| `records::object` | `ObjectRecord`, `State`, `Transition`, `KitRecord`, `NONE`, and flag constants | objects, kits |
| `records::item` | `ItemRecord`, `Icon`, `Wear`, `NONE`, `NO_ITEM`, and the flag, slot and wear-flag constants | items |
| `records::strings` | `encode(&[String])`, `decode` | strings |
| `records::structure` | `Blueprint`, `Site`, `Profile`, `StructureRecord` (decodes any of the three by magic), `Piece`, `Dressing`, `Spread`, `blueprint_id`, `site_id`, `profile_id` | structures |

The model, rig and anim records start with a 4-byte magic and a version byte, `records::VERSION` (2); their decoders reject any other version. Structures records have their own magics and `structure::VERSION` (1). `PackModel::encode` checks its arrays before writing: one material slot and palette index per face, optional per-face arrays sized to the face count, labels sized to the vertex count, and every face index in range.

`RecordError` covers truncation (`Truncated`, with offset and bytes needed), a bad magic, an unsupported version, an invalid record, and trailing bytes.

## Palette helpers

`palette` works on the cache's 16-bit HSL colours (`hue = (c >> 10) & 63`, `sat = (c >> 7) & 7`, `lum = c & 127`):

- `hsl_to_rgb(c)` gives 24-bit RGB as the original client computes it at `DEFAULT_BRIGHTNESS` (0.8), without the small random jitter the client adds when it builds its table. `hsl_to_rgb_with(c, brightness)` takes another brightness. A result of 0 becomes 1, since 0 is reserved for transparency.
- `gamma(rgb, brightness)` applies the brightness curve to a packed colour, and `split(rgb)` unpacks it to `[r, g, b]`.
- `rgb_distance`, `rgb_distance2` and `nearest(palette, colour)` measure and look up colours.
- `kmeans(points, k, iterations)` is a weighted k-means in RGB. It's deterministic: seeded at the heaviest point, then by greatest `distance² × √weight`, then Lloyd iterations until assignments settle.

[Colour](../client/colour.md) explains the HSL encoding, and [palette conversion](../conversion/palette.md) shows how these build a pack palette.

## Loose directories

A loose directory holds a `pack.json` (a `LooseManifest`: `kind`, `source_cache_crc`, and optional `extra`) and one `<id>.bin` file per record. `loose::read(dir)` turns it into a `PackWriter`; a `.bin` file whose stem isn't a number is a `LooseError::Name`. `loose::bake(dir, out)` writes the pack to `out` if it ends in `.pack`, or to `<out>/<name>.pack` otherwise, and returns the path. `rs2ool pack build` is a thin wrapper over it.

## The `bytes` module

`bytes::Writer` is an append-only buffer with chainable `u8`, `i8`, `u16`, `i16`, `u32` and `bytes` methods, all little-endian. `bytes::Reader` reads the same widths, plus `pos()`, `remaining()` and `bytes(n)`, and returns `Truncated` instead of panicking when the data runs out. It's the pack-side counterpart of `rs2cache::io::Cursor`, with the opposite byte order. There's no `u24` or smart here; packs don't use them.

## Example

Write one model record into a pack, then read it back:

```rust
use contentpack::records::model::PackModel;
use contentpack::{Kind, PackReader, PackWriter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let model = PackModel {
        vertices: vec![[0, 0, 0], [128, 0, 0], [0, 128, 0]],
        faces: vec![[0, 1, 2]],
        material_slots: vec![0],
        palette_indices: vec![1],
        ..Default::default()
    };

    let mut writer = PackWriter::new(Kind::Models, 0);
    writer.insert(1, model.encode()?);
    let (name, bytes) = writer.bake();

    let reader = PackReader::from_bytes(&bytes)?;
    assert_eq!(reader.hash, name);
    let record = reader.get(1).expect("record 1 is present")?;
    assert_eq!(PackModel::decode(&record)?, model);
    Ok(())
}
```

## See also

- [Pack container](../pack/container.md): header, manifest, index, blob and naming, byte by byte.
- [Record ids](../pack/record-ids.md): the id namespaces inside each kind.
- [Pack models](../pack/models.md): the model record layout `PackModel` encodes.
- [Determinism](../conversion/determinism.md): why two runs give the same pack name.
- [rs2convert](rs2convert.md): builds these records from cache data.
