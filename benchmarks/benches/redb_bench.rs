use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use forgedb_benchmarks::{
    dataset, id_for, ts_from_seconds, uuid_of, Dataset, PostJson, LIST_CORE_LIMIT,
    LIST_CORE_ROWS, LIST_LIMITS, LIST_SIZES,
};
use redb::{Database, Durability, MultimapTableDefinition, ReadableTable, TableDefinition};

const READ_USERS: usize = 1_000;
const READ_POSTS: usize = 10_000;

const USER: TableDefinition<&[u8], &[u8]> = TableDefinition::new("user");
const POST: TableDefinition<&[u8], &[u8]> = TableDefinition::new("post");
const TAG: TableDefinition<&[u8], &[u8]> = TableDefinition::new("tag");
const EMAIL_IDX: TableDefinition<&str, &[u8]> = TableDefinition::new("email_idx");
const AUTHOR_IDX: MultimapTableDefinition<&[u8], &[u8]> = MultimapTableDefinition::new("author_idx");
const POST_TAG: MultimapTableDefinition<&[u8], &[u8]> = MultimapTableDefinition::new("post_tag");

fn pack_user(name: &str, email: &str, created_at: i64) -> Vec<u8> {
    let mut v = Vec::with_capacity(12 + name.len() + email.len());
    v.extend_from_slice(&created_at.to_le_bytes());
    v.extend_from_slice(&(name.len() as u32).to_le_bytes());
    v.extend_from_slice(name.as_bytes());
    v.extend_from_slice(email.as_bytes());
    v
}
fn unpack_user(b: &[u8]) -> (i64, String, String) {
    let created_at = i64::from_le_bytes(b[0..8].try_into().unwrap());
    let nlen = u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize;
    let name = String::from_utf8_lossy(&b[12..12 + nlen]).into_owned();
    let email = String::from_utf8_lossy(&b[12 + nlen..]).into_owned();
    (created_at, name, email)
}

fn pack_post(title: &str, views: u64, published: bool, author: &[u8; 16], created_at: i64) -> Vec<u8> {
    let mut v = Vec::with_capacity(33 + title.len());
    v.extend_from_slice(&views.to_le_bytes());
    v.extend_from_slice(&created_at.to_le_bytes());
    v.push(published as u8);
    v.extend_from_slice(author);
    v.extend_from_slice(&(title.len() as u32).to_le_bytes());
    v.extend_from_slice(title.as_bytes());
    v
}
fn unpack_post(b: &[u8]) -> (u64, i64, bool, [u8; 16], String) {
    let views = u64::from_le_bytes(b[0..8].try_into().unwrap());
    let created_at = i64::from_le_bytes(b[8..16].try_into().unwrap());
    let published = b[16] != 0;
    let mut author = [0u8; 16];
    author.copy_from_slice(&b[17..33]);
    let tlen = u32::from_le_bytes(b[33..37].try_into().unwrap()) as usize;
    let title = String::from_utf8_lossy(&b[37..37 + tlen]).into_owned();
    (views, created_at, published, author, title)
}

fn fresh_db(path: &std::path::Path) -> Database {
    Database::create(path).expect("create redb")
}

fn load(db: &Database, data: &Dataset) {
    let mut tx = db.begin_write().unwrap();
    tx.set_durability(Durability::Immediate);
    {
        let mut users = tx.open_table(USER).unwrap();
        let mut emails = tx.open_table(EMAIL_IDX).unwrap();
        for u in &data.users {
            users.insert(&u.id[..], pack_user(&u.name, &u.email, u.created_at).as_slice()).unwrap();
            emails.insert(u.email.as_str(), &u.id[..]).unwrap();
        }
        let mut posts = tx.open_table(POST).unwrap();
        let mut authors = tx.open_multimap_table(AUTHOR_IDX).unwrap();
        for p in &data.posts {
            posts
                .insert(&p.id[..], pack_post(&p.title, p.views, p.published, &p.author, p.created_at).as_slice())
                .unwrap();
            authors.insert(&p.author[..], &p.id[..]).unwrap();
        }
        let mut tags = tx.open_table(TAG).unwrap();
        for t in &data.tags {
            tags.insert(&t.id[..], t.name.as_bytes()).unwrap();
        }
        let mut post_tags = tx.open_multimap_table(POST_TAG).unwrap();
        for &(p, t) in &data.links {
            post_tags.insert(&data.posts[p].id[..], &data.tags[t].id[..]).unwrap();
        }
    }
    tx.commit().unwrap();
}

fn bench_insert(c: &mut Criterion) {
    let mut group = c.benchmark_group("redb/insert_user");
    group.throughput(Throughput::Elements(1));
    for &(dur, label) in &[(Durability::Immediate, "immediate"), (Durability::Eventual, "eventual")] {
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(&dir.path().join("bench.redb"));
        let mut i = 0usize;
        group.bench_function(label, |b| {
            b.iter_batched(
                || {
                    let n = i;
                    i += 1;
                    n
                },
                |n| {
                    let id = uuid::Uuid::from_u128(
                        0xF000_0000_0000_0000_0000_0000_0000_0000 + n as u128,
                    )
                    .into_bytes();
                    let mut tx = db.begin_write().unwrap();
                    tx.set_durability(dur);
                    {
                        let mut users = tx.open_table(USER).unwrap();
                        users
                            .insert(&id[..], pack_user("bulk", &format!("insert{n}@example.com"), 1_700_000_000).as_slice())
                            .unwrap();
                    }
                    tx.commit().unwrap();
                },
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_bulk_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("redb/bulk_load_posts");
    group.sample_size(10);
    for &n in &[1_000usize, 10_000] {
        let data = dataset(n.min(2_000).max(1), n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::new("eventual", n), &data, |b, data| {
            b.iter_batched(
                || tempfile::tempdir().unwrap(),
                |dir| {
                    let db = fresh_db(&dir.path().join("bench.redb"));
                    for u in &data.users {
                        let mut tx = db.begin_write().unwrap();
                        tx.set_durability(Durability::Eventual);
                        {
                            let mut users = tx.open_table(USER).unwrap();
                            users.insert(&u.id[..], pack_user(&u.name, &u.email, u.created_at).as_slice()).unwrap();
                        }
                        tx.commit().unwrap();
                    }
                    for p in &data.posts {
                        let mut tx = db.begin_write().unwrap();
                        tx.set_durability(Durability::Eventual);
                        {
                            let mut posts = tx.open_table(POST).unwrap();
                            posts
                                .insert(&p.id[..], pack_post(&p.title, p.views, p.published, &p.author, p.created_at).as_slice())
                                .unwrap();
                        }
                        tx.commit().unwrap();
                    }
                    dir
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

fn bench_reads(c: &mut Criterion) {
    let data = dataset(READ_USERS, READ_POSTS);
    let dir = tempfile::tempdir().unwrap();
    let db = fresh_db(&dir.path().join("bench.redb"));
    load(&db, &data);

    c.benchmark_group("redb/point_lookup")
        .throughput(Throughput::Elements(1))
        .bench_function("get_post_by_id", |b| {
            let mut i = 0usize;
            b.iter(|| {
                let id = id_for(2, i % READ_POSTS);
                i += 1;
                let rtx = db.begin_read().unwrap();
                let posts = rtx.open_table(POST).unwrap();
                let row = posts.get(&id[..]).unwrap().map(|g| unpack_post(g.value()));
                std::hint::black_box(row)
            });
        });

    c.benchmark_group("redb/index_probe")
        .throughput(Throughput::Elements(1))
        .bench_function("get_user_by_email", |b| {
            let mut i = 0usize;
            b.iter(|| {
                let email = format!("user{}@example.com", i % READ_USERS);
                i += 1;
                let rtx = db.begin_read().unwrap();
                let emails = rtx.open_table(EMAIL_IDX).unwrap();
                let users = rtx.open_table(USER).unwrap();
                let row = emails.get(email.as_str()).unwrap().and_then(|g| {
                    let uid = g.value().to_vec();
                    users.get(&uid[..]).unwrap().map(|u| unpack_user(u.value()))
                });
                std::hint::black_box(row)
            });
        });

    c.benchmark_group("redb/reverse_fk")
        .throughput(Throughput::Elements(1))
        .bench_function("user_posts", |b| {
            let mut i = 0usize;
            b.iter(|| {
                let id = id_for(1, i % READ_USERS);
                i += 1;
                let rtx = db.begin_read().unwrap();
                let authors = rtx.open_multimap_table(AUTHOR_IDX).unwrap();
                let posts = rtx.open_table(POST).unwrap();
                let mut out = Vec::new();
                for pid in authors.get(&id[..]).unwrap() {
                    let pid = pid.unwrap();
                    if let Some(p) = posts.get(pid.value()).unwrap() {
                        out.push(unpack_post(p.value()));
                    }
                }
                std::hint::black_box(out)
            });
        });

    c.benchmark_group("redb/m2m")
        .throughput(Throughput::Elements(1))
        .bench_function("post_tags", |b| {
            let mut i = 0usize;
            b.iter(|| {
                let id = id_for(2, i % READ_POSTS);
                i += 1;
                let rtx = db.begin_read().unwrap();
                let post_tags = rtx.open_multimap_table(POST_TAG).unwrap();
                let tags = rtx.open_table(TAG).unwrap();
                let mut out: Vec<(Vec<u8>, String)> = Vec::new();
                for tid in post_tags.get(&id[..]).unwrap() {
                    let tid = tid.unwrap();
                    if let Some(t) = tags.get(tid.value()).unwrap() {
                        out.push((tid.value().to_vec(), String::from_utf8_lossy(t.value()).into_owned()));
                    }
                }
                std::hint::black_box(out)
            });
        });
}

fn bench_scan(c: &mut Criterion) {
    let data = dataset(READ_USERS, READ_POSTS);
    let dir = tempfile::tempdir().unwrap();
    let db = fresh_db(&dir.path().join("bench.redb"));
    load(&db, &data);

    c.benchmark_group("redb/scan_aggregate")
        .throughput(Throughput::Elements(READ_POSTS as u64))
        .bench_function("sum_views_where_published", |b| {
            b.iter(|| {
                let rtx = db.begin_read().unwrap();
                let posts = rtx.open_table(POST).unwrap();
                let mut count = 0u64;
                let mut sum = 0u128;
                for row in posts.iter().unwrap() {
                    let (_k, v) = row.unwrap();
                    let (views, _ca, published, _a, _t) = unpack_post(v.value());
                    if published {
                        count += 1;
                        sum += views as u128;
                    }
                }
                std::hint::black_box((count, sum))
            });
        });

    type PostRowOut = (u64, i64, bool, [u8; 16], String);
    c.benchmark_group("redb/scan_sort_top10")
        .throughput(Throughput::Elements(READ_POSTS as u64))
        .bench_function("top10_by_views", |b| {
            b.iter(|| {
                let rtx = db.begin_read().unwrap();
                let posts = rtx.open_table(POST).unwrap();
                let mut rows: Vec<PostRowOut> = Vec::new();
                for row in posts.iter().unwrap() {
                    let (_k, v) = row.unwrap();
                    let rec = unpack_post(v.value());
                    if rec.0 >= 50_000 {
                        rows.push(rec);
                    }
                }
                rows.sort_unstable_by(|a, b| b.0.cmp(&a.0));
                rows.truncate(10);
                std::hint::black_box(rows)
            });
        });
}

const REDB_LIST_SHAPES: [&str; 3] = ["unfiltered", "filtered_unindexed", "sorted"];

type PostRec = (u64, i64, bool, [u8; 16], String);

fn to_json_row(key: &[u8], rec: PostRec) -> PostJson {
    let (views, created_at, published, author, title) = rec;
    PostJson {
        id: uuid_of(key.to_vec()),
        title,
        views,
        published,
        author: uuid::Uuid::from_bytes(author),
        created_at: ts_from_seconds(created_at),
        tags: (),
    }
}

fn redb_page(db: &Database, shape: &str, limit: usize) -> Vec<PostJson> {
    let rtx = db.begin_read().unwrap();
    let posts = rtx.open_table(POST).unwrap();

    if shape == "sorted" {
        let mut all: Vec<PostJson> = posts
            .iter()
            .unwrap()
            .map(|row| {
                let (k, v) = row.unwrap();
                to_json_row(k.value(), unpack_post(v.value()))
            })
            .collect();
        all.sort_unstable_by(|a, b| b.views.cmp(&a.views));
        all.truncate(limit);
        return all;
    }

    let want_published = shape == "filtered_unindexed";
    let mut out = Vec::with_capacity(limit);
    for row in posts.iter().unwrap() {
        let (k, v) = row.unwrap();
        let rec = unpack_post(v.value());
        if want_published && !rec.2 {
            continue;
        }
        out.push(to_json_row(k.value(), rec));
        if out.len() == limit {
            break;
        }
    }
    out
}

fn bench_list(c: &mut Criterion) {
    {
        let data = dataset(READ_USERS, LIST_CORE_ROWS);
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(&dir.path().join("bench.redb"));
        load(&db, &data);
        let mut g = c.benchmark_group("redb/list_core");
        for name in REDB_LIST_SHAPES {
            let limit = LIST_CORE_LIMIT;

            let label = format!("{name}/rows={LIST_CORE_ROWS}/limit={limit}");

            g.bench_function(BenchmarkId::new("s1_rows", &label), |b| {
                b.iter(|| {
                    std::hint::black_box(redb_page(&db, name, limit).len())
                });
            });

            g.bench_function(BenchmarkId::new("s2_json", &label), |b| {
                b.iter(|| {
                    let page = redb_page(&db, name, limit);
                std::hint::black_box(serde_json::to_string(&page).unwrap().len())
                });
            });
        }
        g.finish();
    }

    {
        let mut g = c.benchmark_group("redb/list_unfiltered");
        for rows in LIST_SIZES {
            let data = dataset(READ_USERS, rows);
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(&dir.path().join("bench.redb"));
        load(&db, &data);
            let name = "unfiltered";
            let limit = LIST_CORE_LIMIT;

            let label = format!("{name}/rows={rows}/limit={limit}");

            {

                let n = redb_page(&db, name, limit).len();
                assert_eq!(n, limit.min(rows), "BDD-3: redb page held {n} for limit={limit} over {rows}");
            }

            g.bench_function(BenchmarkId::new("s1_rows", &label), |b| {
                b.iter(|| {
                    std::hint::black_box(redb_page(&db, name, limit).len())
                });
            });

            g.bench_function(BenchmarkId::new("s2_json", &label), |b| {
                b.iter(|| {
                    let page = redb_page(&db, name, limit);
                std::hint::black_box(serde_json::to_string(&page).unwrap().len())
                });
            });
        }
        g.finish();
    }

    {
        let data = dataset(READ_USERS, LIST_CORE_ROWS);
        let dir = tempfile::tempdir().unwrap();
        let db = fresh_db(&dir.path().join("bench.redb"));
        load(&db, &data);
        let mut g = c.benchmark_group("redb/list_unfiltered_limits");
        for limit in LIST_LIMITS {
            let rows = LIST_CORE_ROWS;
            let name = "unfiltered";

            let label = format!("{name}/rows={rows}/limit={limit}");

            g.bench_function(BenchmarkId::new("s1_rows", &label), |b| {
                b.iter(|| {
                    std::hint::black_box(redb_page(&db, name, limit).len())
                });
            });

            g.bench_function(BenchmarkId::new("s2_json", &label), |b| {
                b.iter(|| {
                    let page = redb_page(&db, name, limit);
                std::hint::black_box(serde_json::to_string(&page).unwrap().len())
                });
            });
        }
        g.finish();
    }
}

criterion_group!(benches, bench_insert, bench_bulk_load, bench_reads, bench_scan, bench_list);
criterion_main!(benches);
