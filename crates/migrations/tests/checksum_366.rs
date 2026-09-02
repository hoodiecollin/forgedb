use forgedb_migrations::{ChecksumStatus, Migration, SchemaChange};

#[test]
fn the_digest_is_frozen() {
    let vectors: &[(&str, &str)] = &[
        ("", "fnv1a64:cbf29ce484222325"),
        ("a", "fnv1a64:af63dc4c8601ec8c"),
        ("foobar", "fnv1a64:85944171f73967e8"),
    ];

    for (input, want) in vectors {
        assert_eq!(
            forgedb_migrations::checksum::compute(input.as_bytes()),
            *want,
            "FNV-1a/64 of {input:?} moved. Every migration file ever written by forgedb \
             carries a checksum computed this way; changing it invalidates all of them."
        );
        assert_eq!(fnv1a64_tagged(input.as_bytes()), *want);
    }
}

fn fnv1a64_tagged(data: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

fn a_migration() -> Migration {
    Migration::new(
        "add a column".to_string(),
        vec![SchemaChange::AddModel {
            model_name: "Widget".to_string(),
        }],
    )
}

#[test]
fn a_new_migration_is_tagged_and_verifies() {
    let m = a_migration();
    assert!(
        m.checksum.starts_with("fnv1a64:"),
        "a checksum must name the algorithm that produced it, or a later build cannot \
         tell 'written by an older forgedb' from 'this file was edited'; got {:?}",
        m.checksum
    );
    assert_eq!(m.checksum_status(), ChecksumStatus::Verified);
    assert!(m.verify_checksum());
}

#[test]
fn an_edited_migration_is_a_mismatch() {
    let mut m = a_migration();
    m.description = "something else entirely".to_string();

    assert_eq!(m.checksum_status(), ChecksumStatus::Mismatch);
    assert!(!m.verify_checksum());
}

#[test]
fn a_pre_366_checksum_is_unverifiable_not_corrupt() {
    let mut m = a_migration();
    m.checksum = "a3f1c2d4e5b60718".to_string();

    assert_eq!(m.checksum_status(), ChecksumStatus::Unverifiable);
    assert!(
        m.verify_checksum(),
        "a pre-#366 migration file must still load; it is unverifiable, not corrupt"
    );
}

#[test]
fn a_newer_algorithm_is_reported_as_unknown() {
    let mut m = a_migration();
    m.checksum = "blake3:0123456789abcdef".to_string();

    assert_eq!(
        m.checksum_status(),
        ChecksumStatus::UnknownAlgorithm("blake3".to_string())
    );
    assert!(
        !m.verify_checksum(),
        "an unknown algorithm must not pass verification — this build cannot check it"
    );
}
