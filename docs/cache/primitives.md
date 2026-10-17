[Home](../README.md) › [Cache](README.md) › Primitives

# Primitives

Every decoder in the cache reader is built from a handful of reads: fixed-width integers, two variable-width "smart" forms, and newline-terminated strings. This page gives their exact encodings, plus the 16-bit HSL colour that most colour fields use.

All cache integers are **big-endian**. Implemented in [`rs2cache::io`](../../crates/rs2cache/src/io.rs) as the single `Cursor` type; no decoder reads bytes any other way.

## Fixed-width integers

| Read | Bytes | Range |
| --- | --- | --- |
| `u8` | 1 | 0 to 255 |
| `i8` | 1 | −128 to 127 (two's complement) |
| `u16` | 2 | 0 to 65535 |
| `i16` | 2 | −32768 to 32767 |
| `u24` | 3 | 0 to 16 777 215 |
| `u32` | 4 | 0 to 2³² − 1 |
| `i32` | 4 | −2³¹ to 2³¹ − 1 |

There is no signed 24-bit read. A `u24` is returned in a 32-bit value with the top byte zero.

## Smarts

A smart is a value stored in one byte when it is small and two bytes when it isn't. The reader peeks at the first byte: below 128 means the one-byte form, 128 or above means the two-byte form, read as a `u16` that includes the peeked byte.

### Unsigned smart

```text
first byte b < 128:   value = b                  1 byte, 0..=127
first byte b >= 128:  value = u16 - 32768        2 bytes, 0..=32767
```

The two-byte form can also encode 0 to 127 (`80 00` reads as 0). An encoder normally uses one byte whenever the value fits; the reader accepts either.

### Signed smart

```text
first byte b < 128:   value = b - 64             1 byte, -64..=63
first byte b >= 128:  value = u16 - 49152        2 bytes, -16384..=16383
```

Worked values: `40` is 0, `00` is −64, `7f` is 63, `80 00` is −16384, `c0 40` is 64, `ff ff` is 16383.

### Which form goes where

| Field | Form |
| --- | --- |
| Model vertex coordinate deltas | signed smart |
| Model face index deltas | signed smart |
| Frame pack transform deltas | signed smart |
| Map location id and position deltas | unsigned smart |

Two smart forms share one name in a lot of code out there. Go by the formula, not the name. See [Model](../records/model.md), [Frame pack](../records/frame-pack.md) and [Map](../records/map.md) for the fields themselves.

## Strings

A string is a run of bytes terminated by `\n` (byte 10). Each byte is one Latin-1 character. The terminator is consumed and not part of the string. There is no length prefix and no length limit.

```text
bytes      ... 0x0a
```

A string with no terminator before the end of the buffer is an error (`UnterminatedString`, with the offset where the string began), and the cursor doesn't move.

## Errors

Every read is bounds-checked. A read that needs more bytes than remain returns `Eof { offset, needed, remaining }` and leaves the position unchanged; seeking past the end returns `Seek { offset, len }`. Nothing in the cursor panics, which is what lets every decoder promise the same.

## 16-bit HSL colours

Model face colours, recolour pairs in config records and similar fields are 16-bit HSL values packed into a `u16`:

```text
bits 15..10   hue          6 bits, 0..=63
bits  9..7    saturation   3 bits, 0..=7
bits  6..0    lightness    7 bits, 0..=127
```

```text
hue = (c >> 10) & 63
sat = (c >> 7) & 7
lum = c & 127
```

How the original client turns these into RGB, including its brightness curve and why 0 is special, is on [Colour](../client/colour.md). Recolours, which swap one HSL value for another, are covered on [Config](config.md#recolours).

Not every colour in the cache is HSL. Floor definitions store their colours as 24-bit RGB `u24` values, so check the record page before assuming. See [Floor](../records/flo.md).

## See also

- [Data and index files](data-and-index-files.md): the 24-bit sizes and sector numbers in index records.
- [Config](config.md): how opcode streams combine these reads.
- [Glossary](../glossary.md): smart, HSL colour.
- [Testing](../development/testing.md): the property tests that round-trip both smart forms.
