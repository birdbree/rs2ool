[Home](../README.md) › [Pack format](README.md) › Strings

# Strings

A `strings` record (kind 11) is one table of UTF-8 strings for one domain and locale. Other records refer to a string by its index in the table. This page gives the layout, the decoder's checks and the domains in use.

Implemented in [`contentpack::records::strings`](../../crates/contentpack/src/records/strings.rs). All integers are little-endian.

## Layout

```text
count     u32
offsets   (count + 1) × u32   byte offsets into data
data      offsets[count] bytes of UTF-8
```

String `i` is `data[offsets[i] .. offsets[i + 1]]`. The last offset is the length of `data`. Strings aren't terminated; an empty string has two equal offsets. The encoder writes `offsets[0]` as 0 and the strings back to back in index order, so the offsets never decrease.

Note there are `count + 1` offsets, not `count`: the leading 0 is stored too. Read `count` offsets and you'll be one short and four bytes out from then on.

## Decoder checks

- `count`, all `count + 1` offsets, and `offsets[count]` bytes of data must be present (`Truncated` otherwise).
- No bytes may follow the data (`Trailing`).
- Each string's range `offsets[i] .. offsets[i + 1]` must lie within the data and not run backwards (`Invalid`, "string offsets").
- Each string must be valid UTF-8 (`Invalid`, "string is not UTF-8").

The decoder doesn't require `offsets[0]` to be 0.

## Ids, domains and locales

The record id is `domain << 8 | locale`.

| Domain | Contents | Indexed by |
| --- | --- | --- |
| 1 | Floor names | the `name` field of the `floors` manifest table ([Record ids](record-ids.md#manifest-extras)) |
| 2 | Object names and option strings | `nameRef` and `optionRefs` in [object records](objects.md) |

Locale 0 is English, and it's the only locale the tools write, so the two tables are records `0x100` and `0x200`. Each comes in its own `strings` pack: `rs2ool pack floors` writes the floor table, `rs2ool pack objects` the object table.

The floor table has one entry per floor definition, in id order, with an empty string for a floor that has no name. The object table holds each distinct name and option string once, in the order the objects first use them; objects that share a string share its index.

## See also

- [Objects](objects.md): `nameRef` and `optionRefs`, and `NONE` for a missing string.
- [Floors](../conversion/floors.md): the floor name table and the `floors` manifest table.
- [Objects conversion](../conversion/objects.md): how the object table is built.
- [Record ids](record-ids.md): the full id scheme.
