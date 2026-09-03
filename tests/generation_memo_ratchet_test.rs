use std::path::{Path, PathBuf};

const SUBJECT: &str = "crates/codegen/tests/codegen_snapshots.rs";
const UNMEMOIZED_BUDGET: usize = 0;
const MIN_MEMOIZED: usize = 100;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn read_subject() -> String {
    let path = repo_root().join(SUBJECT);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn line_of(src: &str, offset: usize) -> usize {
    src[..offset].matches('\n').count() + 1
}

fn generation_sites(src: &str) -> (Vec<(usize, String)>, usize) {
    const NEEDLE: &str = "Generator::generate(&";
    let mut unmemoized = Vec::new();
    let mut memoized = 0;

    let mut from = 0;
    while let Some(rel) = src[from..].find(NEEDLE) {
        let at = from + rel;
        from = at + NEEDLE.len();

        let tail_end = (at + 200).min(src.len());
        if !src[at..tail_end].contains(").unwrap().code") {
            continue;
        }

        let mut ident_start = at;
        while ident_start > 0
            && src.as_bytes()[ident_start - 1].is_ascii_alphanumeric()
            || (ident_start > 0 && src.as_bytes()[ident_start - 1] == b'_')
        {
            ident_start -= 1;
        }

        let mut before = src[..ident_start].trim_end();
        if let Some(stripped) = before.strip_suffix('{') {
            before = stripped.trim_end();
        }
        if before.ends_with("||") {
            memoized += 1;
        } else {
            let line = line_of(src, ident_start);
            let text = src[ident_start..].lines().next().unwrap_or("").trim().to_string();
            unmemoized.push((line, text));
        }
    }
    (unmemoized, memoized)
}

#[test]
fn every_generation_routes_through_the_memo() {
    let src = read_subject();
    let (unmemoized, memoized) = generation_sites(&src);

    assert!(
        memoized >= MIN_MEMOIZED,
        "only {memoized} memoized generation sites found in {SUBJECT}, expected at least \
         {MIN_MEMOIZED}.\n\
         \n\
         This floor is not a ratchet on the exact count — that is what the \
         `unmemoized == 0` assertion below is for, and tests come and go. It exists so \
         the detector cannot report a clean zero because it matched NOTHING. It has \
         already fired once for exactly that reason: rustfmt rewrote every \
         `|| expr` into `|| {{ expr }}` and a line-based detector saw 0 of both kinds."
    );

    assert_eq!(
        unmemoized.len(),
        UNMEMOIZED_BUDGET,
        "{} generation call site(s) in {SUBJECT} bypass `memoized_code`, budget is \
         {UNMEMOIZED_BUDGET}.\n\
         \n\
         `forgedb_source_guard::cached_source` once had ZERO consumers while being fully \
         tested — the suite generated directly, so the memo was proven correct and never \
         ran, and the snapshot suite was 2.2% slower than before the AST guards landed. \
         Route the call through `memoized_code(\"<Generator>\", &schema, || ...)`.\n\
         \n\
         Sites:\n{}",
        unmemoized.len(),
        unmemoized
            .iter()
            .map(|(n, l)| format!("  {SUBJECT}:{n}  {l}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}

#[test]
fn the_detector_sees_an_unmemoized_call() {
    let planted = "    let code = RustGenerator::generate(&schema).unwrap().code;\n";
    let (unmemoized, _) = generation_sites(planted);
    assert_eq!(
        unmemoized.len(),
        1,
        "the detector must flag a bare generate-and-take-code call, or the budget above \
         is vacuous: {unmemoized:?}"
    );

    for wrapped in [
        "let c = memoized_code(\"RustGenerator\", &s, || RustGenerator::generate(&s).unwrap().code);\n",
        "let c = memoized_code(\"RustGenerator\", &s, || {\n    RustGenerator::generate(&s).unwrap().code\n});\n",
    ] {
        let (still, count) = generation_sites(wrapped);
        assert!(
            still.is_empty() && count == 1,
            "a wrapped call must count as memoized in BOTH closure spellings — rustfmt \
             rewrites `|| expr` into `|| {{ expr }}`, and a detector that only knows one \
             of them reports every site unmemoized: {still:?} / {count}\n{wrapped}"
        );
    }
}
