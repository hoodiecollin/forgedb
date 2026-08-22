//! A ratchet on the guard-scoping defect #388 exists to remove.
//!
//! # What is being counted
//!
//! `&src[src.find(needle).unwrap_or(FALLBACK)..]` — a scoping probe whose miss-behaviour is
//! a *fallback value* rather than a failure. There is no correct fallback: the two available
//! ones are "nothing" and "everything", and both are wrong answers wearing the costume of an
//! answer.
//!
//! The "everything" direction is the dangerous one, and it is not hypothetical. Measured on
//! `codegen_snapshots.rs` before this work: the `#170` insert-fsync guard's window ran to
//! EOF, covering 237 KB of a 261 KB file and containing eight `.write(&forgedb_wal::WalEntry`
//! calls belonging to seven *other* methods — so it passed with the call it names deleted
//! outright.
//!
//! That is strictly worse than a vacuous pass. A vacuous pass stops claiming anything; a
//! widened scope keeps claiming the same thing about a bigger haystack, so it gets **easier
//! to satisfy exactly as it becomes meaningless**.
//!
//! # Why a ratchet rather than a ban
//!
//! Banning the idiom outright would mean migrating ~42 windowing sites in one change, which
//! is precisely the "rewrite 1,396 assertions" #388 rules out. A ratchet lets the migration
//! be incremental while making it impossible for the count to drift back up — and, unlike a
//! TODO, it fails.
//!
//! **When you migrate a site to `forgedb_source_guard`, lower `BUDGET`.** The test tells you
//! the number. It is not a target to sit at; it is a debt that only moves one way.
//!
//! As of #388 every in-scope Rust site is migrated and the count is at its floor — see the
//! per-site notes on `BUDGET` for why the remainder cannot widen a Rust assertion.

use std::path::{Path, PathBuf};

/// Files that still scope by byte offset, and how many such sites each may contain.
///
/// Lower these as sites migrate to `forgedb-source-guard`. Never raise one: a new guard has
/// the testkit available and has no reason to reach for a widening window.
const BUDGET: &[(&str, usize)] = &[
    // 3 remaining, and every one is deliberately out of scope for #388 rather than
    // pending. Each was inspected; none can widen a Rust assertion:
    //
    //   * the strict-mirror walk over the generated **TypeScript** client — TS is an
    //     explicit non-goal of #388, which covers Rust and Go;
    //   * an `unwrap_or(0)` inside a **failure-message argument**, not an assertion. The
    //     worst case is a confusing message on a test that is already failing;
    //   * a scope into generated **Go** whose start is `unwrap_or_else(|| panic!(…))` —
    //     already fatal on a miss, which is the property that matters.
    //
    // So this is a floor, not a debt. If it drops to 2 one of the above was addressed;
    // if it rises, a genuinely new widening window was added and the test says where.
    ("crates/codegen/tests/codegen_snapshots.rs", 3),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Count `…find(…).unwrap_or…` occurrences: a lookup whose miss produces a value.
///
/// Deliberately textual. This is a *census of a spelling*, not an assertion about program
/// behaviour, and the spelling is exactly what is being retired — so matching it literally is
/// the right tool here, unlike the guards it is counting.
fn widening_sites(src: &str) -> Vec<(usize, String)> {
    src.lines()
        .enumerate()
        .filter(|(_, l)| {
            let t = l.trim_start();
            // Skip the prose: this file's own doc comments quote the idiom, and so do the
            // explanatory comments left at migrated sites.
            !t.starts_with("//")
                && !t.starts_with("///")
                && !t.starts_with("//!")
                && l.contains("find(")
                && l.contains("unwrap_or")
        })
        .map(|(i, l)| (i + 1, l.trim().to_string()))
        .collect()
}

#[test]
fn widening_windows_only_ever_decrease() {
    let mut checked = 0;

    for (rel, budget) in BUDGET {
        let path = repo_root().join(rel);
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let sites = widening_sites(&src);
        checked += 1;

        assert!(
            sites.len() <= *budget,
            "{rel} has {} byte-offset windows whose miss WIDENS the scope, budget is {}.\n\
             \n\
             A new one was added, or a migrated one came back. Use \
             `forgedb_source_guard::RustSource` — its scoping queries return `Result` and a \
             miss is an error, never a wider scope.\n\
             \n\
             Sites:\n{}",
            sites.len(),
            budget,
            sites
                .iter()
                .map(|(n, l)| format!("  {rel}:{n}  {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        );

        // The ratchet half. Leaving the budget above the real count lets a re-introduced
        // site hide in the slack, which would make this test pass while the thing it guards
        // got worse.
        assert_eq!(
            sites.len(),
            *budget,
            "{rel} now has {} such windows but the budget still says {}. Lower BUDGET to {} \
             in tests/widening_window_ratchet_test.rs — slack in a ratchet is somewhere for a \
             regression to hide.",
            sites.len(),
            budget,
            sites.len()
        );
    }

    // Guards the guard: an emptied BUDGET would otherwise pass by iterating over nothing —
    // the never-evaluated-reads-as-passing failure this whole area exists to remove.
    assert_eq!(checked, BUDGET.len());
    assert!(checked > 0, "BUDGET must not be empty");
}
