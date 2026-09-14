use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use forgedb_source_guard::{module_prefix_of, DocAttr, RustSource};

const CRATES: &[(&str, &str, usize)] = &[
    ("forgedb-types", "types", 25),
    ("forgedb-storage", "storage", 1),
    ("forgedb-storage-native", "storage-native", 110),
    ("forgedb-wal", "wal", 35),
    ("forgedb-changefeed", "changefeed", 30),
    ("forgedb-auth", "auth", 30),
    ("forgedb-query-params", "query-params", 35),
    ("forgedb-compaction", "compaction", 70),
    ("forgedb-txn", "txn", 15),
    ("forgedb-coordinator", "coordinator", 50),
];

const TOTAL_FLOOR: usize = 450;

const NON_EXECUTING_FENCES: &[&str] = &["text", "toml", "json", "sh", "console", "forge", "http", "yaml"];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

struct Site {
    key: String,
    attr: Option<DocAttr>,
    source: String,
    depth: usize,
}

fn sites_of(dir: &str) -> Vec<Site> {
    let crate_dir = repo_root().join("crates").join(dir);
    let mut out = Vec::new();
    for src in RustSource::walk(crate_dir.join("src"), &[]) {
        let rel = Path::new(src.origin())
            .strip_prefix(&crate_dir)
            .unwrap_or_else(|_| panic!("{} is not under {}", src.origin(), crate_dir.display()))
            .to_path_buf();
        let prefix = module_prefix_of(&rel);
        let depth = rel.components().count().saturating_sub(1);
        for s in src.doc_sites(&prefix) {
            out.push(Site {
                key: s.key,
                attr: s.attr,
                source: rel.display().to_string(),
                depth,
            });
        }
    }
    assert!(
        !out.is_empty(),
        "the walk of crates/{dir}/src found no attachment points — every assertion keyed \
         on it would pass vacuously"
    );
    out
}

fn sidecar_files(dir: &str) -> BTreeSet<String> {
    let docs = repo_root().join("crates").join(dir).join("docs");
    let Ok(entries) = std::fs::read_dir(&docs) else {
        return BTreeSet::new();
    };
    entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".md"))
        .collect()
}

#[test]
fn no_scoped_crate_carries_a_doc_literal() {
    for (name, dir, _) in CRATES {
        let literals: Vec<String> = sites_of(dir)
            .into_iter()
            .filter(|s| matches!(s.attr, Some(DocAttr::Literal(_))))
            .map(|s| format!("{} ({})", s.key, s.source))
            .collect();
        assert!(
            literals.is_empty(),
            "{name} carries prose in source as a doc attribute on: {literals:?}. The rule \
             (#488, restated by #490) bans the sentence, not the attribute — point it at a \
             sidecar with #[doc = include_str!(\"../docs/<key>.md\")]"
        );
    }
}

#[test]
fn every_include_names_its_own_sidecar_and_the_file_exists() {
    for (name, dir, _) in CRATES {
        for site in sites_of(dir) {
            let Some(DocAttr::IncludeStr(path)) = &site.attr else {
                continue;
            };
            let expected = format!("{}docs/{}.md", "../".repeat(site.depth), site.key);
            assert_eq!(
                path, &expected,
                "{name}: the doc attribute on `{}` ({}) names `{path}`, not its own sidecar \
                 `{expected}`. A pasted attribute documents one item with another's prose and \
                 nothing else would notice",
                site.key, site.source
            );
            let file = repo_root().join("crates").join(dir).join("docs").join(format!("{}.md", site.key));
            assert!(
                file.is_file(),
                "{name}: `{}` points at {} which does not exist",
                site.key,
                file.display()
            );
        }
    }
}

#[test]
fn every_sidecar_is_claimed_by_exactly_one_attachment_point() {
    for (name, dir, _) in CRATES {
        let mut claims: BTreeMap<String, usize> = BTreeMap::new();
        for site in sites_of(dir) {
            if matches!(site.attr, Some(DocAttr::IncludeStr(_))) {
                *claims.entry(format!("{}.md", site.key)).or_default() += 1;
            }
        }
        let files = sidecar_files(dir);
        let orphans: Vec<&String> = files.iter().filter(|f| !claims.contains_key(*f)).collect();
        let duplicates: Vec<(&String, &usize)> = claims.iter().filter(|(_, n)| **n > 1).collect();
        assert!(
            orphans.is_empty(),
            "{name}: crates/{dir}/docs/ holds sidecars no public item names: {orphans:?}. \
             The item was renamed or deleted and its prose kept lying; rename or delete the \
             file with it (#490)"
        );
        assert!(
            duplicates.is_empty(),
            "{name}: one sidecar is named by more than one attachment point: {duplicates:?}"
        );
    }
}

fn fences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut open: Option<char> = None;
    for line in text.lines() {
        let t = line.trim_start();
        let marker = if t.starts_with("```") {
            Some('`')
        } else if t.starts_with("~~~") {
            Some('~')
        } else {
            None
        };
        let Some(m) = marker else { continue };
        match open {
            Some(o) if o == m => open = None,
            Some(_) => {}
            None => {
                open = Some(m);
                out.push(t.trim_start_matches(m).trim().to_string());
            }
        }
    }
    out
}

#[test]
fn sidecars_are_non_empty_and_no_fence_executes() {
    let mut checked = 0;
    for (name, dir, _) in CRATES {
        for file in sidecar_files(dir) {
            let path = repo_root().join("crates").join(dir).join("docs").join(&file);
            let text = std::fs::read_to_string(&path).unwrap();
            assert!(
                !text.trim().is_empty(),
                "{name}: {file} is empty — an empty sidecar satisfies missing_docs while \
                 documenting nothing"
            );
            for info in fences(&text) {
                let lang = info.split(|c: char| c == ',' || c.is_whitespace()).next().unwrap_or("");
                assert!(
                    NON_EXECUTING_FENCES.contains(&lang),
                    "{name}: {file} has a fence tagged `{info}`. rustdoc executes a bare, \
                     `rust`, `ignore`, `no_run`, `should_panic` or `compile_fail` fence as a \
                     doctest, and sidecars carry no runnable examples as a rule (#490). \
                     Allowed tags: {NON_EXECUTING_FENCES:?}"
                );
            }
            checked += 1;
        }
    }
    assert!(checked >= TOTAL_FLOOR, "only {checked} sidecars were read; expected at least {TOTAL_FLOOR}");
}

#[test]
fn keys_are_unique_case_insensitively_within_a_crate() {
    for (name, dir, _) in CRATES {
        let mut seen: BTreeMap<String, String> = BTreeMap::new();
        for site in sites_of(dir) {
            let folded = site.key.to_lowercase();
            if let Some(prev) = seen.insert(folded, site.key.clone()) {
                assert_eq!(
                    prev, site.key,
                    "{name}: `{prev}` and `{}` are one sidecar on a case-insensitive filesystem \
                     (macOS) and two on Linux CI; the tree would pass here and fail there",
                    site.key
                );
            }
        }
    }
}

#[test]
fn the_walk_is_not_vacuous() {
    let mut total = 0;
    for (name, dir, floor) in CRATES {
        let n = sites_of(dir)
            .iter()
            .filter(|s| matches!(s.attr, Some(DocAttr::IncludeStr(_))))
            .count();
        assert!(
            n >= *floor,
            "{name} has {n} sidecar attachment points, below its floor of {floor}. Either the \
             walker stopped seeing the crate or a large share of its docs were removed"
        );
        total += n;
    }
    assert!(total >= TOTAL_FLOOR, "{total} sidecar attachment points in all; floor is {TOTAL_FLOOR}");
}

#[test]
fn the_makefile_checks_exactly_these_crates() {
    let mk = std::fs::read_to_string(repo_root().join("Makefile")).unwrap();
    let joined = mk.replace("\\\n", " ");
    let line = joined
        .lines()
        .find(|l| l.starts_with("SUBSTRATE_DOC_CRATES"))
        .expect("Makefile declares SUBSTRATE_DOC_CRATES");
    let listed: BTreeSet<&str> = line
        .split(":=")
        .nth(1)
        .expect("SUBSTRATE_DOC_CRATES uses :=")
        .split_whitespace()
        .collect();
    let expected: BTreeSet<&str> = CRATES.iter().map(|(n, _, _)| *n).collect();
    assert_eq!(
        listed, expected,
        "`make docs-check` and this test disagree about which crates are the documented \
         substrate; a crate in one list and not the other is guarded by half the checks"
    );
}
