use forgedb_codegen::ApiGenerator;
use forgedb_parser::Parser;
use regex::Regex;
use std::collections::BTreeSet;
use std::path::PathBuf;

const KNOWN_PAGE_METHODS: [&str; 2] = ["__with_page", "__with_fast_page"];

const BENCH: &str = "benchmarks/benches/list_rest_bench.rs";

const SCHEMA: &str = "benchmarks/bench.forge";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn is_ident_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

fn terminal_page_callees(emitted: &str) -> BTreeSet<String> {
    let re = Regex::new(r"\breturn\s+db\s*\.\s*\w+\s*\.\s*(\w+)\s*\(").expect("valid regex");
    re.captures_iter(emitted)
        .map(|c| c[1].to_string())
        .collect()
}

fn strip_comments(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == 'r' && !(i > 0 && is_ident_char(b[i - 1])) {
            let mut j = i + 1;
            let mut hashes = 0;
            while j < b.len() && b[j] == '#' {
                hashes += 1;
                j += 1;
            }
            if j < b.len() && b[j] == '"' {
                let close: String =
                    std::iter::once('"').chain(std::iter::repeat_n('#', hashes)).collect();
                let tail: String = b[j + 1..].iter().collect();
                let end = tail.find(&close).map(|k| j + 1 + k + close.len()).unwrap_or(b.len());
                out.extend(&b[i..end]);
                i = end;
                continue;
            }
        }
        match (b[i], b.get(i + 1)) {
            ('/', Some('/')) => {
                while i < b.len() && b[i] != '\n' {
                    i += 1;
                }
            }
            ('/', Some('*')) => {
                i += 2;
                let mut depth = 1;
                while i < b.len() && depth > 0 {
                    if b[i] == '/' && b.get(i + 1) == Some(&'*') {
                        depth += 1;
                        i += 2;
                    } else if b[i] == '*' && b.get(i + 1) == Some(&'/') {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            ('"', _) | ('\'', _) => {
                let quote = b[i];
                out.push(b[i]);
                i += 1;
                while i < b.len() {
                    if b[i] == '\\' {
                        out.push(b[i]);
                        if i + 1 < b.len() {
                            out.push(b[i + 1]);
                        }
                        i += 2;
                        continue;
                    }
                    out.push(b[i]);
                    i += 1;
                    if b[i - 1] == quote {
                        break;
                    }
                }
            }
            _ => {
                out.push(b[i]);
                i += 1;
            }
        }
    }
    out
}

fn calls_method(code: &str, method: &str) -> bool {
    let re = Regex::new(&format!(r"\.\s*{}\s*\(", regex::escape(method))).expect("valid regex");
    re.is_match(code)
}

fn emitted_api() -> String {
    let src = read(SCHEMA);
    let mut parser = Parser::new(&src).expect("lex benchmarks/bench.forge");
    let schema = parser.parse().expect("parse benchmarks/bench.forge");
    ApiGenerator::generate(&schema)
        .expect("generate the REST API for benchmarks/bench.forge")
        .code
}

#[test]
fn strip_comments_keeps_code_and_drops_prose() {
    let kept = strip_comments("let x = db.post.__with_page(1);");
    assert!(calls_method(&kept, "__with_page"), "plain code must survive");

    let line = strip_comments("// db.post.__with_page(1);\nlet y = 2;");
    assert!(
        !calls_method(&line, "__with_page"),
        "a line comment must not satisfy the guard: {line:?}"
    );

    let block = strip_comments("/* db.post.__with_page(1); */ let y = 2;");
    assert!(
        !calls_method(&block, "__with_page"),
        "a block comment must not satisfy the guard: {block:?}"
    );

    let doc = strip_comments("//! calls `db.post.__with_fast_page(..)` in prose\nlet y = 2;");
    assert!(
        !calls_method(&doc, "__with_fast_page"),
        "a module doc must not satisfy the guard: {doc:?}"
    );

    let raw = strip_comments("let s = r#\"{\"data\":[\"#; db.post.__with_fast_page(0);");
    assert!(
        calls_method(&raw, "__with_fast_page"),
        "code after a raw string containing a quote must survive: {raw:?}"
    );

    let in_str = strip_comments("let u = \"http://x\"; db.post.__with_page(0);");
    assert!(
        calls_method(&in_str, "__with_page"),
        "`//` inside a string literal must not start a comment: {in_str:?}"
    );

    let lifetime = strip_comments("|p: &[PostPageRef<'_>]| db.post.__with_fast_page(0);");
    assert!(
        calls_method(&lifetime, "__with_fast_page"),
        "a lifetime tick must not be read as a char literal: {lifetime:?}"
    );
}

#[test]
fn assert_terminal_set_is_not_vacuous() {
    let set = terminal_page_callees(&emitted_api());
    assert!(
        !set.is_empty(),
        "#282 BDD-9: found no `return db.<field>.<method>(` in the emitted api.rs at all. \
         The scan is broken (most likely whitespace — the emitter writes \
         `return db .post .__with_page(`), not the emitter. Both halves of BDD-9 pass \
         vacuously in this state."
    );
}

#[test]
fn terminal_page_callees_are_all_known() {
    let set = terminal_page_callees(&emitted_api());
    let known: BTreeSet<String> = KNOWN_PAGE_METHODS.iter().map(|s| s.to_string()).collect();
    let unknown: Vec<_> = set.difference(&known).cloned().collect();
    assert!(
        unknown.is_empty(),
        "#282 BDD-9(a): the generated list handler returns from page method(s) {unknown:?}, \
         which are not in KNOWN_PAGE_METHODS {KNOWN_PAGE_METHODS:?}. If that is a deliberate \
         new page path, add it here AND give it a bench arm in {BENCH} — the second half is \
         the one that keeps the published numbers describing the shipped path."
    );
}

#[test]
fn every_emitted_page_method_has_a_bench_arm() {
    let emitted = terminal_page_callees(&emitted_api());
    let code = strip_comments(&read(BENCH));

    let missing: Vec<_> = emitted
        .iter()
        .filter(|m| !calls_method(&code, m))
        .cloned()
        .collect();

    assert!(
        missing.is_empty(),
        "#282 BDD-9(b): the generated list handler returns from {missing:?}, but {BENCH} \
         contains no call to {missing:?} outside comments. ForgeDB's arms are measuring a \
         path the router no longer takes, and BDD-1's byte-equality guard cannot see it — \
         both methods hand the terminal closure the same `&[<Model>PageRef]`, so the \
         response bytes are identical by construction. Add the arm; retain the existing \
         one beside it as the in-run control."
    );
}

#[test]
fn the_benchmarked_schema_emits_both_page_paths() {
    let set = terminal_page_callees(&emitted_api());
    for method in KNOWN_PAGE_METHODS {
        assert!(
            set.contains(method),
            "#282 BDD-9: `bench.forge` no longer emits `{method}` on the list path \
             (emitted: {set:?}). #282 publishes numbers for this schema, so a page path \
             missing *here* means the benchmark stops covering it — even though the \
             emitter may still produce it for other schemas."
        );
    }
}
