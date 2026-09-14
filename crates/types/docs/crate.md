Core value types shared by ForgeDB's generated code and its substrate crates.

The crate knows nothing about any schema. It provides the Rust representations of the schema
language's primitive types that need more than a `std` type, plus a tagged [`Value`] enum for
code that must carry a primitive of unknown type.

| Schema type | Rust type |
|---|---|
| `u32`, `u64`, `i32`, `i64`, `f64`, `bool` | the same `std` primitives |
| `string` | `String` |
| `string(N)` / `string(N!)` | `String` in a column; [`InlineStr`] when the field is a key |
| `uuid` | [`Uuid`], re-exported from the `uuid` crate |
| `timestamp`, `timestamp(s\|ms\|us)` | [`Timestamp`], microseconds since the Unix epoch |

[`Timestamp`] renders as an RFC 3339 string on every serde surface and parses leniently
([`Timestamp::from_rfc3339`]). [`InlineStr`] is the `Copy`, fixed-capacity string that a
`string(N)` identity, and any foreign key to one, is passed around as; [`InlineStrError`] is why
a `&str` did not fit. [`TimestampParseError`] is the opaque failure of an RFC 3339 parse.
