use forgedb_source_guard::RustSource;

const TWO_MODELS: &str = r#"
pub struct UserStore { rows: Vec<u8> }
pub struct PostStore { rows: Vec<u8> }

impl UserStore {
    pub fn insert(&mut self, r: User) -> usize {
        let idx = self.rows.len();
        self.wal.write(&WalEntry::raw("user", payload));
        idx
    }
    pub fn update(&mut self, id: usize) {
        self.wal.write(&WalEntry::raw("user", payload));
    }
}

impl PostStore {
    pub fn insert(&mut self, r: Post) -> usize {
        self.rows.len()
    }
}

pub fn free_helper() {
    let m = manifest.auto_sequences;
    let _ = m;
}

pub struct NapiUser {
    pub id: String,
    pub count: u32,
}
"#;

fn src() -> RustSource {
    RustSource::generated("two_models.rs", TWO_MODELS)
}

#[test]
fn a_missing_method_is_an_error_not_the_whole_file() {
    let s = src();
    let err = s
        .method_named("nonexistent_method")
        .expect_err("a missing scope MUST NOT resolve");

    let msg = err.to_string();
    assert!(msg.contains("nonexistent_method"), "names what was sought: {msg}");
    assert!(
        msg.contains("UserStore::insert"),
        "a miss must list what is actually present: {msg}"
    );
}

#[test]
fn a_missing_free_fn_is_an_error() {
    let s = src();
    let err = s.fn_named("not_here").expect_err("must not resolve");
    assert!(err.to_string().contains("free_helper"), "lists available fns");
}

#[test]
fn a_missing_struct_is_an_error() {
    let s = src();
    let err = s.struct_named("NoSuchStruct").expect_err("must not resolve");
    assert!(err.to_string().contains("NapiUser"), "lists available structs");
}

#[test]
fn an_ambiguous_method_is_an_error_not_first_wins() {
    let s = src();
    let err = s
        .method_named("insert")
        .expect_err("two `insert` methods must be AMBIGUOUS, not first-wins");

    let msg = err.to_string();
    assert!(msg.contains("AMBIGUOUS"), "must say so plainly: {msg}");
    assert!(msg.contains("UserStore") && msg.contains("PostStore"), "names both: {msg}");
}

#[test]
fn a_scope_does_not_leak_into_the_next_method() {
    let s = src();

    assert_eq!(
        s.method_in("UserStore", "insert").unwrap().call_count("write"),
        1,
        "UserStore::insert writes WAL exactly once"
    );
    assert_eq!(
        s.method_in("PostStore", "insert").unwrap().call_count("write"),
        0,
        "PostStore::insert writes no WAL — a leaking scope would find UserStore's"
    );
}

#[test]
fn a_call_in_a_comment_or_string_is_not_a_call() {
    let s = RustSource::generated(
        "prose.rs",
        r#"
pub fn only_prose() {
    let doc = "call write() here";
    let _ = doc;
}
"#,
    );
    assert_eq!(
        s.fn_named("only_prose").unwrap().call_count("write"),
        0,
        "a call named in a comment or inside a string literal is not a call"
    );
}

#[test]
fn a_longer_identifier_is_not_a_match() {
    let s = RustSource::generated(
        "prefix.rs",
        "pub fn f() { write_batched(); }",
    );
    assert_eq!(
        s.fn_named("f").unwrap().call_count("write"),
        0,
        "`write_batched` must not match `write` — substring matching cannot make this \
         distinction and `contains(\"write\")` gets it wrong"
    );
    assert_eq!(s.fn_named("f").unwrap().call_count("write_batched"), 1);
}

#[test]
fn a_field_read_is_distinct_from_a_declaration_and_an_initializer() {
    let s = RustSource::generated(
        "kinds.rs",
        r#"
pub struct M { pub auto_sequences: u64 }

pub fn declares_only() -> M { M { auto_sequences: 0 } }

pub fn reads_it(m: &M) -> u64 { m.auto_sequences }
"#,
    );

    assert_eq!(
        s.fn_named("declares_only").unwrap().field_read_count("auto_sequences"),
        0,
        "a struct-literal initializer is NOT a read"
    );
    assert_eq!(
        s.fn_named("reads_it").unwrap().field_read_count("auto_sequences"),
        1,
        "`m.auto_sequences` IS a read"
    );
}

#[test]
fn field_type_is_exact_not_a_prefix_match() {
    let s = src();
    assert_eq!(s.field_type("NapiUser", "id").unwrap(), "String");
    assert_eq!(s.field_type("NapiUser", "count").unwrap(), "u32");
}

#[test]
fn field_type_does_not_match_a_similarly_named_type() {
    let s = RustSource::generated("s.rs", "pub struct T { pub id: Stringify }");
    assert_eq!(
        s.field_type("T", "id").unwrap(),
        "Stringify",
        "the type is reported exactly, so `String` cannot be claimed of `Stringify`"
    );
    assert_ne!(s.field_type("T", "id").unwrap(), "String");
}

#[test]
fn a_missing_field_is_an_error_listing_the_real_fields() {
    let s = src();
    let err = s.field_type("NapiUser", "nope").expect_err("must not resolve");
    let msg = err.to_string();
    assert!(msg.contains("id") && msg.contains("count"), "lists real fields: {msg}");
}

#[test]
fn statement_order_is_structural_not_positional() {
    let s = RustSource::generated(
        "order.rs",
        r#"
pub fn f() {
    let keep_all = compute();
    let rows = rows_by_views();
    use_them(keep_all, rows);
}
"#,
    );
    let f = s.fn_named("f").unwrap();

    let idx_of = |needle: &str| {
        f.stmt_index_of(|st| {
            use quote::ToTokens;
            st.to_token_stream().to_string().contains(needle)
        })
        .unwrap_or_else(|| panic!("no statement mentions {needle}"))
    };

    assert!(
        idx_of("keep_all") < idx_of("rows_by_views"),
        "keep_all must be bound before rows_by_views is called"
    );
}
