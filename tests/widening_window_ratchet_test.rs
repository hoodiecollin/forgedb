use std::path::{Path, PathBuf};

const BUDGET: &[(&str, usize)] = &[
    ("crates/codegen/tests/codegen_snapshots.rs", 3),
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn widening_sites(src: &str) -> Vec<(usize, String)> {
    src.lines()
        .enumerate()
        .filter(|(_, l)| {
            let t = l.trim_start();
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

    assert_eq!(checked, BUDGET.len());
    assert!(checked > 0, "BUDGET must not be empty");
}
