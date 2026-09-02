use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use forgedb_benchmarks::forgedb_generated::{
    Database, Doc, DocPageRef, DocScanRef, Post, PostPageRef, PostScanRef, User,
};
use uuid::Uuid;

const BODY_LEN: usize = 200;

const ROWS: [usize; 2] = [1_000, 10_000];

const POINTS: [(usize, usize); 3] = [(0, 50), (0, 1_000), (10, 5)];

fn id_of(tag: u8, i: usize) -> Uuid {
    let mut bytes = [0u8; 16];
    bytes[0] = tag;
    bytes[8..16].copy_from_slice(&(i as u64).to_be_bytes());
    Uuid::from_bytes(bytes)
}

fn body_of(i: usize, tag: char) -> String {
    let mut s = String::with_capacity(BODY_LEN);
    s.push(tag);
    while s.len() < BODY_LEN {
        s.push((b'a' + ((i + s.len()) % 26) as u8) as char);
    }
    s
}

fn doc_of(i: usize) -> Doc {
    Doc {
        id: id_of(9, i),
        seq: i as u64,
        kind: (i % 7) as u32,
        body_a: body_of(i, 'a'),
        body_b: body_of(i, 'b'),
        body_c: body_of(i, 'c'),
        body_d: body_of(i, 'd'),
    }
}

fn populated_docs(n: usize) -> (Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut db = Database::open_at(dir.path().to_path_buf());
    for i in 0..n {
        db.doc.insert(doc_of(i)).expect("insert doc");
    }
    (db, dir)
}

const AUTHOR: u8 = 7;

const BASE_SECS: i64 = 1_700_000_000;

fn populated_posts(n: usize) -> (Database, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut db = Database::open_at(dir.path().to_path_buf());
    let author = id_of(AUTHOR, 0);
    db.user
        .insert(User {
            id: author,
            name: "bench".to_string(),
            email: "bench@example.com".to_string(),
            created_at: forgedb_benchmarks::ts_from_seconds(BASE_SECS),
            posts: (),
        })
        .expect("insert user");
    for i in 0..n {
        db.post
            .insert(Post {
                id: id_of(8, i),
                title: body_of(i, 't'),
                views: i as u64,
                published: i % 2 == 0,
                author,
                created_at: forgedb_benchmarks::ts_from_seconds(BASE_SECS + i as i64),
                tags: (),
            })
            .expect("insert post");
    }
    (db, dir)
}

fn doc_phase_a(db: &Database, offset: usize, limit: usize) -> (usize, Vec<Uuid>) {
    db.doc.__with_scan(
        None,
        |_: &DocScanRef<'_>| true,
        |scan: &mut Vec<DocScanRef<'_>>| {
            let total = scan.len();
            let start = offset.min(scan.len());
            let end = offset.saturating_add(limit).min(scan.len());
            let ids: Vec<Uuid> = scan[start..end].iter().map(|r| r.id).collect();
            (total, ids)
        },
    )
}

fn doc_phase_b(db: &Database, ids: &[Uuid]) -> Vec<Doc> {
    ids.iter().filter_map(|id| db.doc.get(*id)).collect()
}

fn doc_page_buffered(db: &Database, offset: usize, limit: usize) -> (usize, usize) {
    db.doc.__with_page(
        None,
        |_: &DocScanRef<'_>| true,
        |_: &mut Vec<DocScanRef<'_>>| {},
        offset,
        limit,
        |total: usize, page: &[DocPageRef<'_>]| (total, doc_fold(page)),
    )
}

fn doc_fold(page: &[DocPageRef<'_>]) -> usize {
    let mut sum = 0usize;
    for r in page {
        sum ^= r.id.as_u128() as usize;
        sum ^= r.seq as usize;
        sum ^= r.kind as usize;
        sum ^= r.body_a.len();
        sum ^= r.body_b.len();
        sum ^= r.body_c.len();
        sum ^= r.body_d.len();
    }
    sum
}

fn doc_full_buffered(db: &Database, offset: usize, limit: usize) -> (usize, String) {
    db.doc.__with_page(
        None,
        |_: &DocScanRef<'_>| true,
        |_: &mut Vec<DocScanRef<'_>>| {},
        offset,
        limit,
        |total: usize, page: &[DocPageRef<'_>]| {
            (total, serde_json::to_string(page).expect("serialize"))
        },
    )
}

fn doc_select_only(db: &Database) -> usize {
    db.doc.__with_fast_page(0, 0, |total: usize, _| total)
}

fn doc_fast_page(db: &Database, offset: usize, limit: usize) -> (usize, usize) {
    db.doc
        .__with_fast_page(offset, limit, |total: usize, page: &[DocPageRef<'_>]| {
            (total, doc_fold(page))
        })
}

fn doc_fast_buffered(db: &Database, offset: usize, limit: usize) -> (usize, String) {
    db.doc
        .__with_fast_page(offset, limit, |total: usize, page: &[DocPageRef<'_>]| {
            (total, serde_json::to_string(page).expect("serialize"))
        })
}

fn post_phase_a(db: &Database, offset: usize, limit: usize) -> (usize, Vec<Uuid>) {
    db.post.__with_scan(
        None,
        |_: &PostScanRef<'_>| true,
        |scan: &mut Vec<PostScanRef<'_>>| {
            let total = scan.len();
            let start = offset.min(scan.len());
            let end = offset.saturating_add(limit).min(scan.len());
            let ids: Vec<Uuid> = scan[start..end].iter().map(|r| r.id).collect();
            (total, ids)
        },
    )
}

fn post_phase_b(db: &Database, ids: &[Uuid]) -> Vec<Post> {
    ids.iter().filter_map(|id| db.post.get(*id)).collect()
}

fn post_page_buffered(db: &Database, offset: usize, limit: usize) -> (usize, usize) {
    db.post.__with_page(
        None,
        |_: &PostScanRef<'_>| true,
        |_: &mut Vec<PostScanRef<'_>>| {},
        offset,
        limit,
        |total: usize, page: &[PostPageRef<'_>]| (total, post_fold(page)),
    )
}

fn post_fold(page: &[PostPageRef<'_>]) -> usize {
    let mut sum = 0usize;
    for r in page {
        sum ^= r.id.as_u128() as usize;
        sum ^= r.title.len();
        sum ^= r.views as usize;
        sum ^= r.published as usize;
        sum ^= r.author.as_u128() as usize;
        sum ^= r.created_at.as_micros() as usize;
    }
    sum
}

fn post_full_buffered(db: &Database, offset: usize, limit: usize) -> (usize, String) {
    db.post.__with_page(
        None,
        |_: &PostScanRef<'_>| true,
        |_: &mut Vec<PostScanRef<'_>>| {},
        offset,
        limit,
        |total: usize, page: &[PostPageRef<'_>]| {
            (total, serde_json::to_string(page).expect("serialize"))
        },
    )
}

fn post_select_only(db: &Database) -> usize {
    db.post.__with_fast_page(0, 0, |total: usize, _| total)
}

fn post_fast_page(db: &Database, offset: usize, limit: usize) -> (usize, usize) {
    db.post
        .__with_fast_page(offset, limit, |total: usize, page: &[PostPageRef<'_>]| {
            (total, post_fold(page))
        })
}

fn post_fast_buffered(db: &Database, offset: usize, limit: usize) -> (usize, String) {
    db.post
        .__with_fast_page(offset, limit, |total: usize, page: &[PostPageRef<'_>]| {
            (total, serde_json::to_string(page).expect("serialize"))
        })
}

fn bench_doc(c: &mut Criterion) {
    for rows in ROWS {
        let (db, _dir) = populated_docs(rows);

        for (offset, limit) in POINTS {
            let label = format!("rows={rows}/off={offset}/limit={limit}");

            let (_, ids) = doc_phase_a(&db, offset, limit);
            let page = doc_phase_b(&db, &ids);

            let (_, buffered_body) = doc_full_buffered(&db, offset, limit);
            let reference = serde_json::to_string(&page).expect("serialize");
            assert_eq!(
                buffered_body, reference,
                "post-#226 page bytes diverged from the pre-#226 page at {label}"
            );
            let (fast_total, fast_body) = doc_fast_buffered(&db, offset, limit);
            assert_eq!(
                fast_body, reference,
                "#281 fast page bytes diverged from the pre-#226 page at {label}"
            );
            assert_eq!(
                fast_total, rows,
                "#281 `total` must be the live row count, not the page length, at {label}"
            );

            let mut g = c.benchmark_group("forgedb/list_page");

            g.bench_with_input(BenchmarkId::new("full_path", &label), &limit, |b, &limit| {
                b.iter(|| {
                    let (total, ids) = doc_phase_a(&db, offset, limit);
                    let page = doc_phase_b(&db, &ids);
                    let body = serde_json::to_string(&page).expect("serialize");
                    std::hint::black_box((total, body))
                });
            });

            g.bench_with_input(
                BenchmarkId::new("full_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(doc_full_buffered(&db, offset, limit)));
                },
            );

            g.bench_with_input(
                BenchmarkId::new("fast_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(doc_fast_buffered(&db, offset, limit)));
                },
            );

            g.bench_with_input(BenchmarkId::new("scan_only", &label), &limit, |b, &limit| {
                b.iter(|| std::hint::black_box(doc_phase_a(&db, offset, limit)));
            });

            g.bench_with_input(
                BenchmarkId::new("page_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(doc_page_buffered(&db, offset, limit)));
                },
            );

            g.bench_with_input(BenchmarkId::new("fast_page", &label), &limit, |b, &limit| {
                b.iter(|| std::hint::black_box(doc_fast_page(&db, offset, limit)));
            });

            g.bench_with_input(BenchmarkId::new("select_only", &label), &limit, |b, _| {
                b.iter(|| std::hint::black_box(doc_select_only(&db)));
            });

            g.bench_with_input(BenchmarkId::new("page_get", &label), &ids, |b, ids| {
                b.iter(|| std::hint::black_box(doc_phase_b(&db, ids)));
            });

            g.bench_with_input(BenchmarkId::new("serialize", &label), &page, |b, page| {
                b.iter(|| std::hint::black_box(serde_json::to_string(page).expect("serialize")));
            });

            g.finish();
        }
    }
}

fn bench_post_fk(c: &mut Criterion) {
    for rows in ROWS {
        let (db, _dir) = populated_posts(rows);

        for (offset, limit) in POINTS {
            let label = format!("rows={rows}/off={offset}/limit={limit}");

            let (_, ids) = post_phase_a(&db, offset, limit);
            let page = post_phase_b(&db, &ids);

            let (_, buffered_body) = post_full_buffered(&db, offset, limit);
            let reference = serde_json::to_string(&page).expect("serialize");
            assert_eq!(
                buffered_body, reference,
                "post-#226 page bytes diverged from the pre-#226 page at {label}"
            );
            let (fast_total, fast_body) = post_fast_buffered(&db, offset, limit);
            assert_eq!(
                fast_body, reference,
                "#281 fast page bytes diverged from the pre-#226 page at {label}"
            );
            assert_eq!(
                fast_total, rows,
                "#281 `total` must be the live row count, not the page length, at {label}"
            );

            let mut g = c.benchmark_group("forgedb/list_page_fk");

            g.bench_with_input(BenchmarkId::new("full_path", &label), &limit, |b, &limit| {
                b.iter(|| {
                    let (total, ids) = post_phase_a(&db, offset, limit);
                    let page = post_phase_b(&db, &ids);
                    let body = serde_json::to_string(&page).expect("serialize");
                    std::hint::black_box((total, body))
                });
            });

            g.bench_with_input(
                BenchmarkId::new("full_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(post_full_buffered(&db, offset, limit)));
                },
            );

            g.bench_with_input(
                BenchmarkId::new("fast_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(post_fast_buffered(&db, offset, limit)));
                },
            );

            g.bench_with_input(BenchmarkId::new("scan_only", &label), &limit, |b, &limit| {
                b.iter(|| std::hint::black_box(post_phase_a(&db, offset, limit)));
            });

            g.bench_with_input(
                BenchmarkId::new("page_buffered", &label),
                &limit,
                |b, &limit| {
                    b.iter(|| std::hint::black_box(post_page_buffered(&db, offset, limit)));
                },
            );

            g.bench_with_input(BenchmarkId::new("fast_page", &label), &limit, |b, &limit| {
                b.iter(|| std::hint::black_box(post_fast_page(&db, offset, limit)));
            });

            g.bench_with_input(BenchmarkId::new("select_only", &label), &limit, |b, _| {
                b.iter(|| std::hint::black_box(post_select_only(&db)));
            });

            g.bench_with_input(BenchmarkId::new("page_get", &label), &ids, |b, ids| {
                b.iter(|| std::hint::black_box(post_phase_b(&db, ids)));
            });

            g.bench_with_input(BenchmarkId::new("serialize", &label), &page, |b, page| {
                b.iter(|| std::hint::black_box(serde_json::to_string(page).expect("serialize")));
            });

            g.finish();
        }
    }
}

criterion_group!(benches, bench_doc, bench_post_fk);
criterion_main!(benches);
