//! **#282 BDD-9.** ForgeDB's benchmark arms cover every page method the generated
//! list handler actually calls.
//!
//! # The hazard, and why no other guard in this repo can see it
//!
//! `benchmarks/benches/list_rest_bench.rs` measures the generated REST list path at
//! four boundaries, and the two engine-side rungs have to *call something*. The
//! fairness contract deliberately names a **representation** — the borrowed page view
//! `&[<Model>PageRef]` — and not a method, because a contract states where the
//! measurement stops, not which function gets it there. That is the right contract and
//! it is **not self-enforcing**:
//!
//! > Two arms holding the same `&[<Model>PageRef]` from two *different* generated page
//! > methods produce byte-identical output.
//!
//! So the S2↔S3 byte-equality guard (BDD-1) is blind to this by construction. A bench
//! arm can go on calling a page method the handler has stopped calling, every assertion
//! stays green, and the published numbers describe a **retired path**. That is not
//! hypothetical: #281 added `__with_fast_page` for the unfiltered, unsorted request —
//! the exact shape both of #282's sweeps measure — and computing the routing tax against
//! the stale arm yields **−173.96 µs**, a negative cost, because the router had become
//! cheaper than the harness mirroring it.
//!
//! # What is asserted
//!
//! **(a) subset.** Every *terminal* page callee the emitter produces is a member of
//! `KNOWN_PAGE_METHODS`. A **bounded whitelist, not a prohibition** — #281's fast path is
//! deliberately *admitted*; the point is that a new one cannot appear unnoticed.
//!
//! **(b) coverage.** For every member actually emitted, the bench file contains a
//! matching call **on a non-comment line**. This is the executable form of the
//! cross-issue promise: the change that invalidates the measurement is blocked until it
//! fixes the measurement.
//!
//! ## "Terminal" is the discriminator, and `return` is what identifies it
//!
//! A page call is the one `db.<field>.` call the handler *returns from*; everything else
//! the arm emits **binds**. The owned `?projection=` block binds `__with_scan`, the
//! no-identity arm binds `all()`, and — the case that makes this necessary — the #160
//! index-pushdown chain binds one `__rows_by_<field>(..)` call per pushdown field
//! *immediately above* the page call. `Post`, the subject of every #282 cross-engine
//! arm, carries `^views`, so "the handler's one `db.<field>.` call" was never true on
//! this schema. Keying on `return` stays correct however many pushdown fields a schema
//! has.
//!
//! ## Whitespace: demonstrated, not predicted
//!
//! `prettyplease` breaks lines by length, so the same call renders differently per
//! schema. The emitter's actual output here is
//!
//! ```text
//! return db .post .__with_fast_page(
//! ```
//!
//! — a space *before* each `.`, the method split from `db` across what was a line break.
//! A pre-committed literal `return db.post.__with_page(` matches **nothing**, half (a)
//! collects an empty set, and the subset check passes **vacuously**. Hence the `\s*`
//! regexes below, and hence `assert_terminal_set_is_not_vacuous`.
//!
//! A hand-rolled matcher was tried first and produced a **false RED** on its first run:
//! a guard intended to stop `.__with_page(` matching inside `.__with_fast_page(` rejected
//! every genuine match, because the character before the needle's leading `.` is
//! legitimately an identifier character (`db.post.`) — and the two needles cannot collide
//! in the first place. That is the argument for the dependency rather than the clever
//! substring scan.
//!
//! ## What this file CANNOT see — established by mutation, not assumed
//!
//! (b) detects an **omitted** arm and nothing else. It cannot tell that the arm measures
//! the unfiltered shape, that it is registered *adjacently* to its control as the in-run
//! paired method requires, or that it is reached at all — a call in an unused helper, or
//! in a function never named in `criterion_group!`, satisfies it. Those are *shape*
//! obligations; they live in the plan and in the bench's own module docs. (b) is the
//! tripwire that the arm exists.
//!
//! **And every assertion here reads emitted *source*, so none of them can see
//! reachability.** Mutating the emitter's branch condition to `if false && __keep_all
//! && …` leaves all four tests **green**, because the `return … __with_fast_page(…)` is
//! still emitted — merely dead. That is not a hole to plug here; it is a different
//! question, and it is already answered by #281's `tests/api_wire_test.rs` W1, whose
//! reachability mutation (`f(__total + 1000)`) turns 8 URIs red. Recording it because a
//! guard whose blind spots are unstated gets trusted for things it never checked.

// Verified by mutation (`128-bdd9-mutate.ts`), each confirming the mutation applied and
// the suite ran all its tests:
//
//   bench arm call renamed away        -> (b) red alone
//   emitter's callee renamed           -> (a) red
//   the whitespace-insensitive regex
//     replaced with a literal          -> anti-vacuity red
//   the emitted fast-page block deleted -> both-paths red ALONE, (a) and (b) green
//
// The last one is the argument for the fourth test: nothing else in the file notices.

use forgedb_codegen::ApiGenerator;
use forgedb_parser::Parser;
use regex::Regex;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// The page methods the generated list handler is allowed to return from.
///
/// `__with_page` is the filtered/sorted path (#226); `__with_fast_page` is the
/// unfiltered, unsorted one (#281). Adding a third is a legitimate change — and the
/// point of (a) is that it must be a *deliberate* one, made here as well as in the
/// emitter, which is also where the bench arm obligation gets noticed.
const KNOWN_PAGE_METHODS: [&str; 2] = ["__with_page", "__with_fast_page"];

/// The benchmark whose arms must mirror whatever the emitter returns from.
const BENCH: &str = "benchmarks/benches/list_rest_bench.rs";

/// The schema that benchmark is generated from.
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

/// Every `return db.<field>.<method>(` in the emitted source, as the set of `<method>`.
///
/// `\s*` at every join is what makes this survive `prettyplease`'s line breaking; `\b`
/// keeps `return` a whole word. Only `return`-prefixed calls are collected, for the
/// reason in the module docs: everything non-terminal *binds*, and the pushdown chain
/// guarantees non-terminal `db.<field>.` calls exist on this very schema.
fn terminal_page_callees(emitted: &str) -> BTreeSet<String> {
    let re = Regex::new(r"\breturn\s+db\s*\.\s*\w+\s*\.\s*(\w+)\s*\(").expect("valid regex");
    re.captures_iter(emitted)
        .map(|c| c[1].to_string())
        .collect()
}

/// Strip Rust comments, leaving code. String, char and **raw** string literals are
/// tracked so that a `//` or `/*` inside one is not mistaken for a comment — the bench
/// file contains `r#"{"data":["#`, which a naive `"`-toggling stripper mis-parses.
fn strip_comments(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        // Raw string: r, then n hashes, then a quote. Ends at quote + n hashes.
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
                // Keep the newline so line structure survives.
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

/// Does `code` contain a `.<method>(` call? Whitespace-insensitive; comments are already
/// stripped by the caller.
///
/// `\s*\(` after the name is what makes the method boundary exact, so no
/// `__with_page` / `__with_page_v2` confusion is possible and no lookbehind is needed.
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

/// The comment stripper is the one hand-rolled scanner left, and half (b) is only as
/// trustworthy as it is: strip too little and a doc comment satisfies the guard forever;
/// strip too much and the guard fails on working code. Both directions are checked here,
/// including the raw-string case the bench file actually contains (`r#"{"data":["#`),
/// which a naive `"`-toggling stripper mis-parses.
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

    // A raw string holding a quote, then real code after it. If the stripper loses its
    // place inside the raw string it will swallow the call that follows.
    let raw = strip_comments("let s = r#\"{\"data\":[\"#; db.post.__with_fast_page(0);");
    assert!(
        calls_method(&raw, "__with_fast_page"),
        "code after a raw string containing a quote must survive: {raw:?}"
    );

    // A `//` inside a string literal is not a comment.
    let in_str = strip_comments("let u = \"http://x\"; db.post.__with_page(0);");
    assert!(
        calls_method(&in_str, "__with_page"),
        "`//` inside a string literal must not start a comment: {in_str:?}"
    );

    // Lifetimes are not char literals -- the bench is full of `<'_>`.
    let lifetime = strip_comments("|p: &[PostPageRef<'_>]| db.post.__with_fast_page(0);");
    assert!(
        calls_method(&lifetime, "__with_fast_page"),
        "a lifetime tick must not be read as a char literal: {lifetime:?}"
    );
}

/// The anti-vacuity assertion. A scan that matches nothing satisfies "is a subset of"
/// and "every member is covered" *trivially*, so both halves below are meaningless
/// unless this holds. It is the assertion a whitespace mistake trips.
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

/// **(a) subset.** No page method reaches the list path without being declared here.
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

/// **(b) coverage.** Every emitted page method has a bench arm calling it.
///
/// The non-comment scoping is load-bearing rather than defensive: the bench's module
/// docs and both arms' doc comments name **both** methods in prose, so a raw substring
/// search over the file would be satisfied by comments alone and this guard would be
/// permanently, silently green.
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

/// The benchmarked schema specifically exercises **both** page paths.
///
/// Separate from (a)/(b) on purpose. (a) is a bounded whitelist and (b) is keyed on what
/// was *emitted*, so if the emitter ever stopped producing `__with_fast_page` **for
/// `bench.forge`'s models** — a per-model capability hole rather than a removal — the
/// emitted set would shrink to one, (a) and (b) would both stay green, and #282's
/// sweeps would silently stop measuring the fast path. #281's own guards use their own
/// fixtures and would not notice. This pins the dispatch shape on the schema whose
/// numbers get published.
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
