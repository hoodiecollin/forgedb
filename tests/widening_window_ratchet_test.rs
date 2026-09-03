use std::path::{Path, PathBuf};

const BUDGET: &[(&str, usize)] = &[("crates/codegen/tests/codegen_snapshots.rs", 3)];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn widening_sites(src: &str) -> Vec<(usize, String)> {
    const NEEDLE: &str = ".find(";
    const WINDOW: usize = 260;
    let mut sites = Vec::new();

    let mut from = 0;
    while let Some(rel) = src[from..].find(NEEDLE) {
        let at = from + rel;
        from = at + NEEDLE.len();

        let end = (at + WINDOW).min(src.len());
        let Some(rel_fallback) = src[at..end].find("unwrap_or") else {
            continue;
        };
        if src[at + rel_fallback..end].starts_with("unwrap_or_else") {
            continue;
        }

        let line = src[..at].matches('\n').count() + 1;
        let text: String = src[at..end].split_whitespace().collect::<Vec<_>>().join(" ");
        sites.push((line, text.chars().take(110).collect()));
    }
    sites
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

        let listing = sites
            .iter()
            .map(|(n, l)| format!("  {rel}:{n}  {l}"))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            sites.len() <= *budget,
            "{rel} has {} byte-offset windows whose miss WIDENS the scope, budget is {}.\n\
             \n\
             A new one was added, or a migrated one came back. Use \
             `forgedb_source_guard::RustSource` — its scoping queries return `Result` and a \
             miss is an error, never a wider scope.\n\
             \n\
             Sites:\n{listing}",
            sites.len(),
            budget,
        );

        assert_eq!(
            sites.len(),
            *budget,
            "{rel} now has {} such windows but the budget still says {}. Lower BUDGET to {} \
             in tests/widening_window_ratchet_test.rs — slack in a ratchet is somewhere for a \
             regression to hide.\n\nSites:\n{listing}",
            sites.len(),
            budget,
            sites.len(),
        );
    }

    assert_eq!(checked, BUDGET.len());
    assert!(checked > 0, "BUDGET must not be empty");
}

#[test]
fn the_detector_separates_a_widening_fallback_from_a_hard_failure() {
    let widening = r#"let s = &code[code.find("pub fn x").unwrap_or(0)..];"#;
    assert_eq!(
        widening_sites(widening).len(),
        1,
        "`unwrap_or(0)` on a byte offset is the whole point of this ratchet"
    );

    let hard = r#"let s = &code[code.find(d).unwrap_or_else(|| panic!("{d} missing"))..];"#;
    assert!(
        widening_sites(hard).is_empty(),
        "`unwrap_or_else(|| panic!(..))` FAILS rather than widening, so counting it inflates \
         the budget and leaves room for a real widening site to hide underneath"
    );

    let split = "let end = f\n    .find(\"pub fn \")\n    .map(|i| i + 1)\n    .unwrap_or(f.len());";
    assert_eq!(
        widening_sites(split).len(),
        1,
        "a widening site split across lines by rustfmt is still a widening site; a \
         line-based detector reported this one as absent"
    );
}
