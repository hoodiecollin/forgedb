//! #252 — `InlineStr<BYTES>`, the `Copy` fixed-capacity string key.
//!
//! Gate 3 scenarios 1–7: the *substrate* half. `InlineStr` is class-1 substrate —
//! it knows nothing about schemas, identities or URLs, so the URL-path-segment
//! alphabet (res 4) and the non-empty rule (res 5) are **not** here. Those apply
//! because a field is an identity, which is schema knowledge, and they are
//! generated (res 7); they are guarded in `crates/codegen/tests` and driven for
//! real by the round-trip harness.
//!
//! What *is* here is everything the generated code assumes about the type:
//! it is `Copy`, it compares and hashes on its text rather than on its buffer
//! (res 8), it has a `Default` (Gate 2 finding 2 — the generated id field carries
//! `#[serde(default)]`), and its serde form is a JSON string at **every** width
//! (Gate 2 finding 1 — serde has no array derive past 32, the #243 failure class).

use forgedb_types::{InlineStr, InlineStrError};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

fn hash_of<T: Hash>(v: &T) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

/// Scenario 1 — the capacity bound is a **rejection**, never a truncation.
///
/// A silently truncated key is the worst outcome available here: it writes a row
/// under an id the caller never asked for, and the caller's own copy of the id no
/// longer resolves. The error carries both numbers so the generated 422 can name
/// the bound without re-deriving it.
#[test]
fn capacity_is_a_bound_not_a_truncation() {
    let ulid = "01JQZ8Y7X6W5V4T3S2R1Q0P9NM"; // 26 characters
    assert_eq!(ulid.len(), 26);

    let ok = InlineStr::<26>::try_from(ulid).expect("26 bytes fits a 26-byte capacity");
    assert_eq!(ok.as_str(), ulid);
    assert_eq!(ok.len(), 26);

    let err = InlineStr::<26>::try_from("01JQZ8Y7X6W5V4T3S2R1Q0P9NMX").unwrap_err();
    assert_eq!(
        err,
        InlineStrError {
            got_bytes: 27,
            capacity: 26
        }
    );
}

/// Scenario 2 — two values built from the same text through different
/// constructors are equal **and hash equal**.
///
/// Res 8. `InlineStr` is a `HashMap` key type (`id_to_row`, `id_versions`, and
/// #266's junction traversal indexes), so an `Eq`/`Hash` pair that disagreed
/// would produce lookup *misses*, not compile errors.
#[test]
fn equal_text_hashes_equal_whatever_built_it() {
    let via_try = InlineStr::<32>::try_from("cus_N8s7Ld2mQ").unwrap();
    let via_from_str = InlineStr::<32>::from_str("cus_N8s7Ld2mQ").unwrap();

    assert_eq!(via_try, via_from_str);
    assert_eq!(hash_of(&via_try), hash_of(&via_from_str));

    // And a round-trip through serde is the same value again — the third path
    // the generated code actually uses (a create body deserializes the key).
    let json = serde_json::to_string(&via_try).unwrap();
    let via_serde: InlineStr<32> = serde_json::from_str(&json).unwrap();
    assert_eq!(via_try, via_serde);
    assert_eq!(hash_of(&via_try), hash_of(&via_serde));
}

/// Scenario 3 — the capacity is not part of the value's *identity*, and a
/// shorter value is never padded into equality with a longer one.
///
/// The tail-bytes half of res 8 cannot be written here: the public API has no way
/// to produce a dirty tail (every constructor copies a whole `&str` into a zeroed
/// buffer), which is by design. It is guarded inside the crate instead — see
/// `inline_str_tail_is_not_part_of_the_value` in `crates/types/src/lib.rs`, which
/// hand-builds a dirty buffer through the private fields.
#[test]
fn a_short_value_is_not_padded_into_its_neighbours() {
    let ab = InlineStr::<32>::try_from("ab").unwrap();
    let ab_again = InlineStr::<32>::try_from("ab").unwrap();
    let abc = InlineStr::<32>::try_from("abc").unwrap();

    assert_eq!(ab, ab_again);
    assert_ne!(ab, abc);
    assert_eq!(ab.len(), 2);
    assert_eq!(abc.len(), 3);
}

/// Scenario 4 — the serde form is a JSON **string**, at a width above 32.
///
/// Gate 2 finding 1 / the clarification comment: serde's *derive* has per-length
/// array impls that stop at 32, which is exactly the #243 defect (`char(N)` with
/// N > 32 could not be indexed). `InlineStr` is const-generic, so ONE hand-written
/// impl covers every width — but only if it is hand-written. Instantiating at 64
/// is what makes a later "simplification" to `#[derive(Serialize)]` fail to
/// **compile** instead of silently changing the wire form.
#[test]
fn serde_is_a_json_string_above_the_derive_ceiling() {
    let long = "a-vendor-account-number-past-the-derive-ceiling";
    assert!(
        long.len() > 32 && long.len() <= 64,
        "the width is the point of this scenario: {}",
        long.len()
    );

    let v = InlineStr::<64>::try_from(long).unwrap();
    let json = serde_json::to_string(&v).unwrap();

    // A JSON string, not an array of integers.
    assert_eq!(json, format!("\"{long}\""));
    assert!(!json.starts_with('['));

    let back: InlineStr<64> = serde_json::from_str(&json).unwrap();
    assert_eq!(back, v);
    assert_eq!(back.as_str(), long);

    // Over-capacity input is a deserialize *error*, not a truncation — the same
    // rule as `try_from`, reached through the create-body path.
    let too_long = serde_json::to_string(&"x".repeat(65)).unwrap();
    assert!(serde_json::from_str::<InlineStr<64>>(&too_long).is_err());
}

/// Scenario 5 — `Default` is the empty string, and an absent field deserializes
/// to it.
///
/// Gate 2 finding 2: every generated model struct carries `#[serde(default)]` on
/// its id field so a create body may omit it. That is safe against res 5 (the
/// empty key is rejected) because the rejection is at *write*, not at
/// construction — the default exists so the deserialize-then-allocate path has
/// something to land in.
#[test]
fn default_is_empty_and_fills_an_absent_field() {
    let d = InlineStr::<26>::default();
    assert!(d.is_empty());
    assert_eq!(d.len(), 0);
    assert_eq!(d.as_str(), "");

    #[derive(serde::Deserialize)]
    struct Row {
        #[serde(default)]
        id: InlineStr<26>,
        name: String,
    }

    let row: Row = serde_json::from_str(r#"{"name":"ada"}"#).unwrap();
    assert!(row.id.is_empty());
    assert_eq!(row.name, "ada");
}

/// Scenario 6 — `Ord` agrees with `str`'s ordering, including across a prefix
/// pair.
///
/// #256 wants `InlineStr` as an allocation-free *index key*, and the generated
/// index writes a lexicographic key. An ordering that disagreed with `str` would
/// put a range scan's results in the wrong order with nothing failing.
#[test]
fn ord_agrees_with_str() {
    let words = ["abc", "ab", "", "b", "aB", "abcd", "~", "0"];
    let mut inline: Vec<InlineStr<8>> =
        words.iter().map(|w| InlineStr::try_from(*w).unwrap()).collect();
    let mut plain: Vec<&str> = words.to_vec();

    inline.sort();
    plain.sort();

    let inline_text: Vec<&str> = inline.iter().map(|v| v.as_str()).collect();
    assert_eq!(inline_text, plain);

    // The prefix pair specifically: "ab" < "abc" must not depend on the buffer's
    // zero padding happening to sort below 'c'.
    assert!(InlineStr::<8>::try_from("ab").unwrap() < InlineStr::<8>::try_from("abc").unwrap());
}

/// Scenario 7 — `Display` and `Debug` both render the text.
///
/// `Display` is what the generated create handler's `id.to_string()` and the
/// index-key `write!` produce, and what makes `/docs/{id}` resolvable. A derived
/// `Debug` would print 254 integers into every log line and every panic message.
#[test]
fn display_and_debug_render_text() {
    let v = InlineStr::<40>::try_from("urn:isbn:0451450523").unwrap();

    assert_eq!(v.to_string(), "urn:isbn:0451450523");
    assert_eq!(format!("{v:?}"), "\"urn:isbn:0451450523\"");
    assert!(!format!("{v:?}").contains("117"), "must not render raw bytes");
}

/// `Copy` is the entire point (Gate 1's impl table): the generated code passes
/// the key by value repeatedly — `get(id)`, `delete(id)`, relation resolution,
/// the live-query delta enum — which is why `String` cannot be an identity at
/// all. Asserted by *using* the value after a move, which only compiles if the
/// type is `Copy`.
#[test]
fn the_key_is_copy() {
    let v = InlineStr::<26>::try_from("01JQZ8Y7X6W5V4T3S2R1Q0P9NM").unwrap();
    let moved = v;
    assert_eq!(v, moved);

    fn by_value(k: InlineStr<26>) -> usize {
        k.len()
    }
    assert_eq!(by_value(v), 26);
    assert_eq!(by_value(v), 26);
}

/// `Deref<Target = str>` is what lets the generated validation, filter and sort
/// code keep comparing against `&str` unchanged (Gate 1's impl table).
#[test]
fn deref_gives_the_whole_str_surface() {
    let v = InlineStr::<32>::try_from("user@example.com").unwrap();

    assert!(v.contains('@'));
    assert!(v.starts_with("user"));
    assert_eq!(v.chars().count(), 16);
    assert_eq!(v.as_bytes(), b"user@example.com");
    assert_eq!(&v[..4], "user");
}
