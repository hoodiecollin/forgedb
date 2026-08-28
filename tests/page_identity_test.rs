mod common;

const SCHEMA: &str = r#"
enum Tier { Free, Pro, Enterprise }

struct Dims {
  w: u32
  h: u32
}

// Every field class the page view can hold, so the two construction sites are
// compared over all of them at once: a nullable variable string, a decimal, an enum,
// a timestamp, fixed bytes, a fixed array, an inline struct, a string-keyed FK, an
// unstored `[Model]` relation, json, and two indexed scalars.
Widget {
  id: +uuid
  label: string?
  price: decimal
  tier: ^Tier
  made_at: timestamp(ms)
  checksum: bytes(4)
  scores: [i32; 3]
  dims: Dims
  owner: *Org
  parts: [Part]
  payload: json
  serial: ^u32
}

// A string identity, so `Widget.owner` is an `InlineStr<6>` rather than a `Uuid` —
// the non-scan, inline-string field whose `borrowed` flag flips.
Org {
  id: string(6!)
  name: string
  widgets: [Widget]
}

Part {
  id: +uuid
  name: string
  widget: *Widget
}

Doc {
  id: +uuid
  title: string
  links: [Link]
}

Tag {
  id: +uuid
  name: string
  links: [Link]
}

// ZERO filterable fields: the identity is a required FK (a relation is never
// filterable) and `meta` is json (no total order). Its scan set is a single
// NON-filterable identity, which is the shape the emitter must not assume away.
Link {
  id: *Doc
  other: *Tag
  meta: json
}
"#;

const DRIVER: &str = r##"
mod database;
use database::*;

use forgedb_types::{InlineStr, Timestamp, Uuid};

/// Deterministic uuids: the corpus must be identical on every run so a failure is
/// reproducible from the printed `(model, offset, limit)` alone.
fn uuid_of(kind: u8, n: usize) -> Uuid {
    let mut b = [0u8; 16];
    b[0] = kind;
    b[8..16].copy_from_slice(&(n as u64).to_be_bytes());
    Uuid::from_bytes(b)
}

fn org_id(n: usize) -> InlineStr<6> {
    InlineStr::try_from(format!("o{n:05}").as_str()).expect("org key fits 6 chars")
}

// Sized so that EVERY model is still above 1,000 live rows AFTER the churn below
// removes `n/DELETE_EVERY` of them — the floor is asserted at the end rather than
// trusted, because it is the property that keeps the `limit = 1000` column
// discriminating.
const ORGS: usize = 1_050;
const WIDGETS: usize = 1_100;
const OTHERS: usize = 1_100;

/// Rows updated (a dead version stays in the span, the live row moves to the tail)
/// and rows deleted (a hole in the selection). Chosen so the live count stays above
/// 1,000 and the live set is nowhere near the dense prefix `[0, n)`.
const UPDATE_EVERY: usize = 11;
const DELETE_EVERY: usize = 19;

fn widget_of(n: usize, rev: u32) -> Widget {
    Widget {
        id: uuid_of(1, n),
        // Nullable, and BOTH inhabitants appear — a byte-exact comparison cannot
        // discriminate a silently-defaulted field whose fixture value IS the default.
        label: if n % 3 == 0 {
            None
        } else {
            Some(format!("label-{n:05}-r{rev}"))
        },
        price: format!("{}.{:02}", n % 977, (n * 7 + rev as usize) % 100)
            .parse()
            .expect("decimal"),
        tier: match n % 3 {
            0 => Tier::Free,
            1 => Tier::Pro,
            _ => Tier::Enterprise,
        },
        made_at: Timestamp::from_micros(1_700_000_000_000_000 + (n as i64) * 1_000),
        checksum: [(n & 0xff) as u8, (n >> 8) as u8, rev as u8, 0xA5],
        scores: [n as i32, -(n as i32), (n as i32) * 3 + rev as i32],
        dims: Dims {
            w: (n % 640) as u32 + 1,
            h: (n % 480) as u32 + 1,
        },
        owner: org_id(n % 50),
        parts: (),
        payload: serde_json::json!({ "n": n, "rev": rev, "z": [1, 2, 3] }),
        serial: (n as u32) * 10 + rev,
    }
}

/// The windows. `limit = 0` and an `offset` past the end are legitimate requests and
/// both paths must agree on them too — that is where the clamping arithmetic lives.
const OFFSETS: [usize; 4] = [0, 1, 5, 50];
const LIMITS: [usize; 6] = [0, 1, 2, 5, 50, 1000];

static mut FAILURES: u32 = 0;

fn check(model: &str, offset: usize, limit: usize, fast: (usize, String), slow: (usize, String)) {
    let (t_fast, s_fast) = fast;
    let (t_slow, s_slow) = slow;
    if t_fast != t_slow {
        println!("  FAIL {model} off={offset} lim={limit} — total {t_fast} != {t_slow}");
        unsafe { FAILURES += 1 };
        return;
    }
    if s_fast != s_slow {
        println!("  FAIL {model} off={offset} lim={limit} — page bytes differ");
        println!("    fast: {}", &s_fast[..s_fast.len().min(400)]);
        println!("    slow: {}", &s_slow[..s_slow.len().min(400)]);
        unsafe { FAILURES += 1 };
    }
}

/// One grid sweep for one model. A macro rather than a function because the two
/// methods are inherent to each `*Storage` type and the view types differ per model —
/// there is no trait to be generic over, and inventing one for a test would be a
/// different shape than the code under test.
macro_rules! sweep {
    ($db:expr, $field:ident, $name:literal) => {{
        let mut cells = 0usize;
        for offset in OFFSETS {
            for limit in LIMITS {
                let fast = $db.$field.__with_fast_page(offset, limit, |t, p| {
                    (t, serde_json::to_string(p).expect("serialize fast page"))
                });
                let slow = $db.$field.__with_page(
                    None,
                    |_| true,
                    |_| {},
                    offset,
                    limit,
                    |t, p| (t, serde_json::to_string(p).expect("serialize page")),
                );
                check($name, offset, limit, fast, slow);
                cells += 1;
            }
        }
        let total = $db.$field.__with_fast_page(0, 0, |t, _| t);
        println!("  {:<7} {cells} windows over {total} live rows", $name);
        total
    }};
}

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("data dir arg"));
    let _ = std::fs::remove_dir_all(&dir);
    let mut db = Database::open_at(dir.clone());

    for n in 0..ORGS {
        db.org
            .insert(Org {
                id: org_id(n),
                name: format!("org-{n:05}"),
                widgets: (),
            })
            .expect("insert org");
    }
    for n in 0..WIDGETS {
        db.widget.insert(widget_of(n, 0)).expect("insert widget");
    }
    for n in 0..OTHERS {
        db.doc
            .insert(Doc {
                id: uuid_of(2, n),
                title: format!("doc-{n:05}"),
                links: (),
            })
            .expect("insert doc");
        db.tag
            .insert(Tag {
                id: uuid_of(3, n),
                name: format!("tag-{n:05}"),
                links: (),
            })
            .expect("insert tag");
        db.part
            .insert(Part {
                id: uuid_of(4, n),
                name: format!("part-{n:05}"),
                widget: uuid_of(1, n % WIDGETS),
            })
            .expect("insert part");
        db.link
            .insert(Link {
                id: uuid_of(2, n),
                other: uuid_of(3, n),
                meta: serde_json::json!({ "n": n }),
            })
            .expect("insert link");
    }

    // Churn. An update appends a new version and repoints `id_to_row` at it, so the
    // live row moves to the tail and a dead version stays inside the mapped span; a
    // delete appends a tombstone and punches a hole. Both are what stop the live set
    // from being the dense prefix `[0, n)` — the only condition under which slicing
    // the selection could disagree with mapping through a recorded slot.
    for n in (0..WIDGETS).step_by(UPDATE_EVERY) {
        db.widget
            .update(uuid_of(1, n), widget_of(n, 1))
            .expect("update widget");
    }
    for n in (0..WIDGETS).step_by(DELETE_EVERY) {
        assert!(db.widget.delete(uuid_of(1, n)), "delete widget {n}");
    }
    // `Part` has no dependants, so it can be churned freely. `Doc`/`Tag` are each
    // referenced by a `Link` and `@on_delete` defaults to restrict, so they are left
    // alone deliberately rather than by oversight.
    for n in (0..OTHERS).step_by(UPDATE_EVERY) {
        db.part
            .update(
                uuid_of(4, n),
                Part {
                    id: uuid_of(4, n),
                    name: format!("part-{n:05}-v2"),
                    widget: uuid_of(1, (n + 1) % WIDGETS),
                },
            )
            .expect("update part");
    }
    for n in (0..OTHERS).step_by(DELETE_EVERY) {
        assert!(db.part.delete(uuid_of(4, n)), "delete part {n}");
    }

    println!("#281 S10 — fast page vs page, per model:");
    let live_widget = sweep!(db, widget, "widget");
    let live_org = sweep!(db, org, "org");
    let live_part = sweep!(db, part, "part");
    let live_doc = sweep!(db, doc, "doc");
    let live_tag = sweep!(db, tag, "tag");
    let live_link = sweep!(db, link, "link");

    // The corpus size is part of the spec: below 1,000 the `limit = 1000` column
    // stops being a near-full page and the grid quietly loses its discriminating
    // cells. Assert it rather than trusting the constants above to stay in step.
    for (name, live) in [
        ("widget", live_widget),
        ("org", live_org),
        ("part", live_part),
        ("doc", live_doc),
        ("tag", live_tag),
        ("link", live_link),
    ] {
        if live < 1_000 {
            println!("  FAIL {name} has {live} live rows, below the 1,000 the grid needs");
            unsafe { FAILURES += 1 };
        }
    }
    // `limit = 1000` against ~1,000 live rows is the `p = r` regime — the page IS
    // the table — which is where the cost model predicts a zero win. It has to be a
    // real cell, not a page that happens to be the whole table on every model.
    if live_widget <= 1_000 {
        println!("  FAIL widget must exceed 1,000 live rows so `limit=1000` is a partial page");
        unsafe { FAILURES += 1 };
    }

    let failures = unsafe { FAILURES };
    if failures > 0 {
        println!("{failures} mismatch(es)");
        std::process::exit(1);
    }
    println!("ok — the two page construction sites agree on every window");
}
"##;

#[test]
#[ignore = "compiles and runs a generated crate; run with --ignored (see `make page-identity-test`)"]
fn fast_page_and_page_build_identical_pages() {
    let (out, proj) = common::generate_compile_run("pageidentity", SCHEMA, DRIVER);
    let warnings = common::build_warnings(&proj);
    common::assert_driver_ok(&out, &proj, "the fast page and the page disagree");
    for needle in ["FastPageBufs", "__with_fast_page"] {
        assert!(
            !warnings.contains(needle),
            "#281: the generated crate warns about `{needle}`:\n{warnings}"
        );
    }
}
