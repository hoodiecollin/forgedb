use forgedb_types::{InlineStr, InlineStrError};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::str::FromStr;

fn hash_of<T: Hash>(v: &T) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

#[test]
fn capacity_is_a_bound_not_a_truncation() {
    let ulid = "01JQZ8Y7X6W5V4T3S2R1Q0P9NM";
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

#[test]
fn equal_text_hashes_equal_whatever_built_it() {
    let via_try = InlineStr::<32>::try_from("cus_N8s7Ld2mQ").unwrap();
    let via_from_str = InlineStr::<32>::from_str("cus_N8s7Ld2mQ").unwrap();

    assert_eq!(via_try, via_from_str);
    assert_eq!(hash_of(&via_try), hash_of(&via_from_str));

    let json = serde_json::to_string(&via_try).unwrap();
    let via_serde: InlineStr<32> = serde_json::from_str(&json).unwrap();
    assert_eq!(via_try, via_serde);
    assert_eq!(hash_of(&via_try), hash_of(&via_serde));
}

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

    assert_eq!(json, format!("\"{long}\""));
    assert!(!json.starts_with('['));

    let back: InlineStr<64> = serde_json::from_str(&json).unwrap();
    assert_eq!(back, v);
    assert_eq!(back.as_str(), long);

    let too_long = serde_json::to_string(&"x".repeat(65)).unwrap();
    assert!(serde_json::from_str::<InlineStr<64>>(&too_long).is_err());
}

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

    assert!(InlineStr::<8>::try_from("ab").unwrap() < InlineStr::<8>::try_from("abc").unwrap());
}

#[test]
fn display_and_debug_render_text() {
    let v = InlineStr::<40>::try_from("urn:isbn:0451450523").unwrap();

    assert_eq!(v.to_string(), "urn:isbn:0451450523");
    assert_eq!(format!("{v:?}"), "\"urn:isbn:0451450523\"");
    assert!(!format!("{v:?}").contains("117"), "must not render raw bytes");
}

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

#[test]
fn deref_gives_the_whole_str_surface() {
    let v = InlineStr::<32>::try_from("user@example.com").unwrap();

    assert!(v.contains('@'));
    assert!(v.starts_with("user"));
    assert_eq!(v.chars().count(), 16);
    assert_eq!(v.as_bytes(), b"user@example.com");
    assert_eq!(&v[..4], "user");
}
