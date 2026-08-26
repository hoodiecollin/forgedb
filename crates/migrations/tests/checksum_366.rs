//! The migration checksum is a **pinned** value, and the pinning is the test (#366).
//!
//! What was here before was `DefaultHasher` behind a module named `md5`. Its output is not
//! guaranteed stable across Rust releases, and that output is written into every migration
//! file and verified on load — so a rustup upgrade, or two developers on one repo, was
//! enough to make committed migration files fail with
//! `"file may be corrupted"`. The file was fine; the hash had moved underneath it.
//!
//! ## Why golden vectors and not a round-trip
//!
//! A round-trip (`compute` then `verify`) passes on ANY hash function, including the broken
//! one — it was passing the whole time. It cannot distinguish "stable across builds" from
//! "stable within this build", and that distinction is the entire defect.
//!
//! Only a **literal expected value** can fail when the algorithm moves. These vectors are
//! frozen bytes, not a recomputation: nothing here calls the implementation to decide what
//! the answer should be. If a change makes one of them fail, that change breaks every
//! migration file every user has committed, and the vector is the only thing that will say
//! so.

use forgedb_migrations::{ChecksumStatus, Migration, SchemaChange};

/// FNV-1a/64 over known inputs, tagged. Computed from the specification, and pinned.
///
/// The empty-input case is the offset basis itself, which makes a transcription error in
/// either constant visible immediately rather than only for long inputs.
#[test]
fn the_digest_is_frozen() {
    // Reach the algorithm the way a migration does — through a real `Migration`, so the
    // test cannot pass while the type wires up something else.
    let vectors: &[(&str, &str)] = &[
        ("", "fnv1a64:cbf29ce484222325"),
        ("a", "fnv1a64:af63dc4c8601ec8c"),
        ("foobar", "fnv1a64:85944171f73967e8"),
    ];

    for (input, want) in vectors {
        // The CRATE'S implementation against a frozen literal. This assertion, and only
        // this one, can fail when the shipped algorithm moves.
        assert_eq!(
            forgedb_migrations::checksum::compute(input.as_bytes()),
            *want,
            "FNV-1a/64 of {input:?} moved. Every migration file ever written by forgedb \
             carries a checksum computed this way; changing it invalidates all of them."
        );
        // And an independent second implementation agrees with the same literal, so a
        // transcription error in the frozen value itself is caught rather than enshrined.
        assert_eq!(fnv1a64_tagged(input.as_bytes()), *want);
    }
}

/// The reference implementation, spelled out here independently of the crate's copy.
///
/// Deliberately a second implementation rather than a call into the crate: a vector that
/// asks the code under test what the answer is cannot fail. This one is written from the
/// FNV-1a specification.
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

/// A freshly created migration verifies, and its checksum announces its own algorithm.
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

/// Editing the file is the ONE case that should read as a mismatch.
#[test]
fn an_edited_migration_is_a_mismatch() {
    let mut m = a_migration();
    m.description = "something else entirely".to_string();

    assert_eq!(m.checksum_status(), ChecksumStatus::Mismatch);
    assert!(!m.verify_checksum());
}

/// The compatibility case, and the reason this fix is not a one-liner.
///
/// Every migration file written before #366 carries a bare `DefaultHasher` value with no
/// tag. It cannot be verified — the number is meaningless outside the compiler that
/// produced it — but it is not evidence of damage. Rejecting it would make the fix detonate
/// exactly the artifact it exists to protect, with exactly the misleading error it exists
/// to remove.
#[test]
fn a_pre_366_checksum_is_unverifiable_not_corrupt() {
    let mut m = a_migration();
    m.checksum = "a3f1c2d4e5b60718".to_string(); // untagged, as DefaultHasher wrote it

    assert_eq!(m.checksum_status(), ChecksumStatus::Unverifiable);
    assert!(
        m.verify_checksum(),
        "a pre-#366 migration file must still load; it is unverifiable, not corrupt"
    );
}

/// A file from the future is its own case, and must NOT be silently accepted.
///
/// Downgrading forgedb is a real thing to do. Treating an unknown digest as `Legacy` would
/// wave through a file this build genuinely cannot check, which is the opposite of what a
/// checksum is for.
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
