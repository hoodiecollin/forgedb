A `Copy`, fixed-capacity UTF-8 string holding at most `BYTES` bytes.

# Why it exists

The generated code passes a model's identity **by value**, repeatedly —
`get(id)`, `delete(id)`, relation resolution, index-key construction, the
live-query delta enum. That is sound for `Uuid`/`u32`/`u64`/[`Timestamp`]
because they are `Copy`; `String` is not, so `id: string` produced 31
move/borrow errors across a single model. Making the key type `Copy` is far
smaller than making every id-consuming path stop copying, and it is what
`string(N)` / `string(N!)` identities are built on (#252).

# Class-1 substrate

It knows nothing about schemas, identities or URLs. In particular the
URL-path-segment alphabet a *string identity's value* must obey (#252 res 4)
is **not** enforced here — that rule applies because a field is an identity,
which is schema knowledge, so the check is emitted into the generated write
path (res 7). What this type enforces is only the capacity bound.

# Parameterized by bytes, declared in characters

`BYTES` is a byte capacity, not a character count: the substrate cannot
compute `[u8; CHARS * 4]` without the unstable `generic_const_exprs`, so the
generator emits the constant. For an identity that mapping is the identity
function — `string(26)` and `string(26!)` both give `InlineStr<26>`, because
the column is one byte per character (#238) and `@utf8` on an identity is a
validation error (#252 res 3). The `4N` form survives for non-identity
columns, which is why the parameter stays in bytes.

# Nothing here is derived except `Clone, Copy`

| impl | why it is hand-written |
|---|---|
| `PartialEq`/`Eq`/`PartialOrd`/`Ord`/`Hash` | defined on [`Self::as_str`]; the bytes past `len` are not part of the value (#252 res 8) |
| `Serialize`/`Deserialize` | a JSON **string**. serde's *derive* has per-length array impls that stop at 32 — the #243 defect (`char(N)` above 32 could not be indexed). Deriving here would compile for `InlineStr<26>` and fail for `InlineStr<64>` |
| `Debug` | a derive would print `BYTES` integers into every log line and panic message |

# Examples

```rust
use forgedb_types::InlineStr;

let ulid: InlineStr<26> = "01JQZ8Y7X6W5V4T3S2R1Q0P9NM".try_into().unwrap();
assert_eq!(ulid.len(), 26);
assert_eq!(ulid.to_string(), "01JQZ8Y7X6W5V4T3S2R1Q0P9NM");

// The capacity is a bound, never a truncation.
assert!(InlineStr::<4>::try_from("hello").is_err());
```
