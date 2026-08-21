//! Native FFI generator — the Layer-0 C-ABI (language-bindings #51/#52/#117).
//!
//! Emits, per schema, the fat generated C-ABI for the language bindings
//! (#51/#52/#117). Two halves land in the SAME generated `ffi/src/lib.rs`:
//!
//! * the schema-**invariant** *lifecycle + error* spine (Phase 2) every binding
//!   (PyO3 / NAPI-RS) hangs off: `open`/`close`, `commit`/`checkpoint`/
//!   `compact`, the error trio (`error_code`/`error_message`/`error_free`),
//!   `free_buffer`, `version`;
//! * the schema-**tailored** per-model OLTP row ops (Phase 3): for each identity
//!   model `M` (snake name `m`), `<m>_insert`/`_get`/`_count`/`_all`/
//!   `_update`/`_delete`, calling the generated `Database::create_<m>` /
//!   `update_<m>` / `delete_<m>` integrity wrappers + `<m>` storage reads.
//!
//! # Every exported symbol carries a per-app prefix (#335 §2, decision 9)
//!
//! The names above are **stems**, not symbols. The linkable name is
//! `<prefix><stem>`, where `<prefix>` comes from `forgedb::naming::symbol_prefix`
//! and is passed in — this file never re-derives it. The prefix used to be the
//! constant `forgedb_`, so two apps in one project that each declare a `Post`
//! exported byte-identical `forgedb_post_insert`. Cargo never sees that: it is
//! not a package-name collision, and under a `cdylib` it only bites if one
//! process loads both. **Static linking (the Go binding's `staticlib`) makes it a
//! link-time collision** in a single Go binary importing two ForgeDB packages —
//! reachable, and silent until late. `go.rs` emits the calling side and the
//! `forgedb.h` prototypes from the same prefix; all three move together or the
//! Go binding does not link.
//!
//! # This crate links `core` and pins ZERO substrate
//!
//! The generated database arrives as the cache's `core` package, renamed to
//! `forgedb_core` by the manifest so no generated `.rs` byte carries the app
//! hash. Substrate types are reached through `core`'s re-exports
//! (`forgedb_core::forgedb_storage`, `::forgedb_types`) so they **unify** with
//! `core`'s rather than merely resolving to the same version by lockfile
//! coincidence.
//!
//! **Identity (class-2 transport glue — the spirit of the old `crates/ffi`):**
//! like the wasm `Replica`, this file is *generated per schema*. The spine's
//! symbols are schema-invariant; the per-model ops reference the generated
//! structs/wrappers by name (that IS the tailoring) but still invent **no**
//! generic query surface — rows and ids cross the C-ABI as OPAQUE JSON bytes
//! (the same opaque-bytes discipline as the WAL / broker / wasm-replica paths),
//! decoded into the generated type via serde at a compile-time-known type. There
//! is deliberately **no** generic `forgedb_query(model, predicate)` symbol and no
//! `match model` runtime dispatch — that shape is the removed-`QueryBuilder` red
//! line (acceptance constraint 1). The `_async` completion bridges land here too
//! (`generate_async_ops` + the schema-invariant executor/callback spine): each
//! `forgedb_<m>_<op>_async` enqueues the blocking engine call on one background
//! worker and fires a caller-registered completion callback keyed by `token`, so
//! a foreign event loop never blocks on the fsync-ing per-row API. The per-column
//! Arrow columnar export (the zero-copy selling point) also lands here
//! (`generate_arrow_ops` + the schema-invariant Arrow C-Data-Interface spine):
//! for each Arrow-exportable non-null fixed-width column, a
//! `forgedb_<m>_<f>_export_arrow` exports exactly the live rows of that one
//! column into a caller-owned Arrow array — a **zero-copy `mmap` alias** of the
//! on-disk column when the live rows are a dense prefix `[0, n)` (no
//! updates/tombstones), a gathered copy otherwise. Both paths ride the same ABI
//! + `release` contract (`forgedb_storage::ColumnExport` behind
//! `fill_arrow_primitive`), so the consumer is alias-or-gather transparent.
//!
//! **Panic discipline.** Every entry point that can run engine code wraps it in
//! `catch_unwind` and converts a panic into a `ForgeError` — an unwind across the
//! `extern "C"` boundary into a foreign caller is UB, so the spine refuses to let
//! one escape (e.g. `open_at`'s `DirLock` contention `expect`).

use crate::RustGenerator;
use crate::{GeneratedCode, Result};
use forgedb_parser::Schema;
use quote::{format_ident, quote};

/// Generates the native FFI spine (`ffi.rs`) for a schema.
pub struct FfiGenerator;

impl FfiGenerator {
    /// Generate the Layer-0 C-ABI lifecycle/error spine.
    ///
    /// `symbol_prefix` is the app's per-app C-symbol prefix, from
    /// `forgedb::naming::symbol_prefix` — the ONE definition. Every
    /// `#[unsafe(no_mangle)]` symbol emitted here is `<symbol_prefix><stem>`;
    /// the spine's stems are schema-invariant but its *symbols* are not, which
    /// is the whole point (two apps in one project must not export the same
    /// name, and `staticlib` delivery makes a duplicate a link-time error).
    ///
    /// `crates/codegen/src/go.rs` emits the calling side (`forgedb.go`) and the
    /// prototypes (`forgedb.h`) from the *same* prefix. The three move together
    /// or the Go binding does not link.
    pub fn generate(schema: &Schema, symbol_prefix: &str) -> Result<GeneratedCode> {
        let p = symbol_prefix;
        let model_ops = Self::generate_model_ops(schema, p);
        let async_ops = Self::generate_async_ops(schema, p);
        let relation_ops = Self::generate_relation_ops(schema, p);
        let arrow_ops = Self::generate_arrow_ops(schema, p);

        // The schema-INVARIANT spine symbols. Their *names* were the last
        // constants in the emitted C-ABI (#335 §2, decision 9): two apps in one
        // project exported byte-identical `forgedb_open`, which cargo never sees
        // and which static linking turns into a link-time collision inside one
        // Go binary. `symbol_prefix` is the ONE definition of the prefix
        // (`forgedb::naming::symbol_prefix`) — it is never re-derived here.
        let sym_version = format_ident!("{}version", p);
        let sym_open = format_ident!("{}open", p);
        let sym_close = format_ident!("{}close", p);
        let sym_commit = format_ident!("{}commit", p);
        let sym_checkpoint = format_ident!("{}checkpoint", p);
        let sym_compact = format_ident!("{}compact", p);
        let sym_error_code = format_ident!("{}error_code", p);
        let sym_error_message = format_ident!("{}error_message", p);
        let sym_error_free = format_ident!("{}error_free", p);
        let sym_free_buffer = format_ident!("{}free_buffer", p);
        let sym_snapshot = format_ident!("{}snapshot", p);
        let sym_snapshot_free = format_ident!("{}snapshot_free", p);
        let sym_set_completion_callback = format_ident!("{}set_completion_callback", p);
        let tokens = quote! {
            //! Generated by ForgeDB — native FFI, Layer-0 C-ABI spine.
            //! DO NOT EDIT - This file is auto-generated.
            //!
            //! Class-2 transport glue for the native language bindings
            //! (#51 Python/PyO3, #52 Node + #117 Bun/NAPI-RS). Exposes the
            //! schema-invariant lifecycle + error surface over the generated
            //! `Database`. The tailored data logic lives in the generated
            //! `database.rs`; this file is the boundary the per-runtime wrappers
            //! bind. It invents no query API (the identity red line).
            //!
            //! **Every exported symbol carries this app's C-symbol prefix.** The
            //! doc comments below name each entry point by its unprefixed stem
            //! (`open`, `error_free`, `<m>_insert`, …); the linkable name is that
            //! stem behind the prefix. Two apps in one project therefore export
            //! disjoint symbol sets and can be linked into one binary.
            #![allow(warnings)]

            // The one generated database for this app, reached as a cargo
            // dependency. The MANIFEST renames it to `forgedb_core`, so no
            // generated `.rs` byte carries the per-app hash.
            use forgedb_core as database;

            use std::ffi::{CStr, CString, c_char, c_void};
            use std::panic::{AssertUnwindSafe, catch_unwind};
            use std::path::PathBuf;
            use std::ptr;
            use std::sync::{Mutex, OnceLock};
            use std::sync::atomic::{AtomicUsize, Ordering};
            use std::sync::mpsc::{self, Sender};
            use std::thread;

            use database::Database;
            // `Uuid` names the primary-key type of UUID-keyed models in the
            // per-model row ops below (integer-PK models use a primitive id type).
            use forgedb_core::forgedb_types::Uuid;

            /// Opaque database handle passed across the C-ABI as `*mut Db`. Owns
            /// the generated `Database` (including its single-writer `DirLock`);
            /// freed by `forgedb_close`.
            pub struct Db {
                inner: Database,
            }

            /// An error surfaced across the C-ABI as `*mut ForgeError`. Allocated
            /// by the engine when an entry point fails and freed by the caller via
            /// `forgedb_error_free` — never freed implicitly.
            pub struct ForgeError {
                code: i32,
                message: CString,
            }

            /// An owned point-in-time read snapshot captured by
            /// `forgedb_snapshot` — a cross-model-consistent bundle of per-
            /// collection row-count watermarks (#56, Direction A).  Passed by
            /// `*const Snapshot` to the `_at` read/traversal entry points and
            /// freed by the caller via `forgedb_snapshot_free`.  Because the
            /// capture is routed through `Database::snapshot()` (taken on the
            /// single writer between mutations), the watermarks are atomic as of
            /// one commit boundary: rows appended after capture are invisible and
            /// no reader can observe a torn row.
            pub struct Snapshot {
                inner: database::DatabaseSnapshot,
            }

            /// A null or non-UTF-8 argument reached an entry point.
            const FORGEDB_ERR_INVALID_ARG: i32 = 1;
            /// The engine returned an `io::Error` (e.g. a failed `commit` fsync).
            const FORGEDB_ERR_IO: i32 = 2;
            /// Engine code panicked; the unwind was caught at the boundary and
            /// converted here rather than crossing into the foreign caller (UB).
            const FORGEDB_ERR_PANIC: i32 = 3;
            /// A write was refused by the generated data-integrity gate (#91):
            /// a field constraint (422), a `&unique` clash or a dangling/blocked
            /// foreign key (409).  The message carries the specifics.
            const FORGEDB_ERR_VALIDATION: i32 = 4;

            /// Write a fresh boxed `ForgeError` through `err_out` when it is
            /// non-null. A nul byte in `message` is impossible from our sources,
            /// but is handled without panicking for total safety.
            unsafe fn set_error(err_out: *mut *mut ForgeError, code: i32, message: String) {
                if err_out.is_null() {
                    return;
                }
                let message = CString::new(message)
                    .unwrap_or_else(|_| CString::new("error").unwrap());
                *err_out = Box::into_raw(Box::new(ForgeError { code, message }));
            }

            /// Extract a human-readable message from a caught panic payload.
            fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
                if let Some(s) = payload.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                }
            }

            /// Clear a caller's error out-param at entry (so a success leaves it
            /// null).  A null `err_out` is a no-op.
            unsafe fn clear_err(err_out: *mut *mut ForgeError) {
                if !err_out.is_null() {
                    *err_out = ptr::null_mut();
                }
            }

            /// View a caller-owned `(ptr, len)` as a borrowed byte slice, or `None`
            /// for a null pointer.  The bytes are read (decoded) before the call
            /// returns, so the borrow never outlives the caller's buffer.
            ///
            /// # Safety
            /// `ptr`/`len`, if `ptr` is non-null, must describe a readable buffer.
            unsafe fn read_bytes<'a>(ptr: *const u8, len: usize) -> Option<&'a [u8]> {
                if ptr.is_null() {
                    None
                } else {
                    Some(std::slice::from_raw_parts(ptr, len))
                }
            }

            /// Hand an engine-owned byte buffer out through `(out, out_len)`,
            /// transferring ownership to the caller — who frees it with
            /// `forgedb_free_buffer` (capacity == length, so the boxed slice
            /// reconstructs exactly).  If either out-param is null the buffer is
            /// dropped rather than leaked.
            ///
            /// # Safety
            /// `out`/`out_len`, when non-null, must point to writable storage.
            unsafe fn emit_bytes(bytes: Vec<u8>, out: *mut *mut u8, out_len: *mut usize) {
                if out.is_null() || out_len.is_null() {
                    drop(bytes);
                    return;
                }
                let boxed = bytes.into_boxed_slice();
                let len = boxed.len();
                let ptr = Box::into_raw(boxed) as *mut u8;
                *out = ptr;
                *out_len = len;
            }

            /// The generated engine crate's version (`CARGO_PKG_VERSION`), as a
            /// stable NUL-terminated C string. Owned by the process — the caller
            /// must NOT free it.
            #[unsafe(no_mangle)]
            pub extern "C" fn #sym_version() -> *const c_char {
                static VERSION: OnceLock<CString> = OnceLock::new();
                VERSION
                    .get_or_init(|| {
                        CString::new(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| {
                            CString::new("0.0.0").unwrap()
                        })
                    })
                    .as_ptr()
            }

            /// Open (or create) a database at `root`. Returns an owned `*mut Db`,
            /// or null with `*err_out` set. `flags` is reserved (must be 0).
            ///
            /// # Safety
            /// `root` must be a valid NUL-terminated C string; `err_out`, if
            /// non-null, must point to writable storage for one `*mut ForgeError`.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_open(
                root: *const c_char,
                _flags: u32,
                err_out: *mut *mut ForgeError,
            ) -> *mut Db {
                if !err_out.is_null() {
                    *err_out = ptr::null_mut();
                }
                if root.is_null() {
                    set_error(err_out, FORGEDB_ERR_INVALID_ARG, "root path is null".to_string());
                    return ptr::null_mut();
                }
                let root = match CStr::from_ptr(root).to_str() {
                    Ok(s) => PathBuf::from(s),
                    Err(_) => {
                        set_error(
                            err_out,
                            FORGEDB_ERR_INVALID_ARG,
                            "root path is not valid UTF-8".to_string(),
                        );
                        return ptr::null_mut();
                    }
                };
                // `open_at` can panic (e.g. `DirLock` contention — single-writer,
                // #89). Catch it here so it becomes an error instead of unwinding
                // into the foreign caller.
                match catch_unwind(AssertUnwindSafe(|| Database::open_at(root))) {
                    Ok(db) => Box::into_raw(Box::new(Db { inner: db })),
                    Err(payload) => {
                        set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                        ptr::null_mut()
                    }
                }
            }

            /// Close a database opened by `forgedb_open`, releasing its lock.
            /// A null handle is a no-op. The handle is invalid afterward.
            ///
            /// # Safety
            /// `db` must be a handle from `forgedb_open` not already closed.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_close(db: *mut Db) {
                if db.is_null() {
                    return;
                }
                drop(Box::from_raw(db));
            }

            /// Flush every column to durable storage (fsync). Returns `true` on
            /// success; on failure returns `false` with `*err_out` set.
            ///
            /// # Safety
            /// `db` must be a live handle from `forgedb_open`.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_commit(
                db: *mut Db,
                err_out: *mut *mut ForgeError,
            ) -> bool {
                if !err_out.is_null() {
                    *err_out = ptr::null_mut();
                }
                let Some(db) = db.as_mut() else {
                    set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                    return false;
                };
                match catch_unwind(AssertUnwindSafe(|| db.inner.commit())) {
                    Ok(Ok(())) => true,
                    Ok(Err(e)) => {
                        set_error(err_out, FORGEDB_ERR_IO, e.to_string());
                        false
                    }
                    Err(payload) => {
                        set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                        false
                    }
                }
            }

            /// Force a WAL checkpoint (fsync columns, then truncate the WAL).
            /// Returns `true` on success, `false` with `*err_out` set on panic.
            ///
            /// # Safety
            /// `db` must be a live handle from `forgedb_open`.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_checkpoint(
                db: *mut Db,
                err_out: *mut *mut ForgeError,
            ) -> bool {
                if !err_out.is_null() {
                    *err_out = ptr::null_mut();
                }
                let Some(db) = db.as_mut() else {
                    set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                    return false;
                };
                match catch_unwind(AssertUnwindSafe(|| db.inner.checkpoint())) {
                    Ok(()) => true,
                    Err(payload) => {
                        set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                        false
                    }
                }
            }

            /// Explicitly compact the database (reclaim dead row versions).
            /// **Explicit only** — no read/convenience path reaches this
            /// (acceptance constraint 4). Returns `true` on success, `false` with
            /// `*err_out` set on panic.
            ///
            /// # Safety
            /// `db` must be a live handle from `forgedb_open`.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_compact(
                db: *mut Db,
                err_out: *mut *mut ForgeError,
            ) -> bool {
                if !err_out.is_null() {
                    *err_out = ptr::null_mut();
                }
                let Some(db) = db.as_mut() else {
                    set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                    return false;
                };
                match catch_unwind(AssertUnwindSafe(|| db.inner.compact())) {
                    Ok(()) => true,
                    Err(payload) => {
                        set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                        false
                    }
                }
            }

            /// The numeric code of an error (see the `FORGEDB_ERR_*` constants).
            /// Returns 0 for a null pointer.
            ///
            /// # Safety
            /// `err`, if non-null, must be a `ForgeError` not yet freed.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_error_code(err: *const ForgeError) -> i32 {
                match err.as_ref() {
                    Some(e) => e.code,
                    None => 0,
                }
            }

            /// The message of an error as a borrowed NUL-terminated C string valid
            /// until `forgedb_error_free`. Returns null for a null pointer.
            ///
            /// # Safety
            /// `err`, if non-null, must be a `ForgeError` not yet freed.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_error_message(err: *const ForgeError) -> *const c_char {
                match err.as_ref() {
                    Some(e) => e.message.as_ptr(),
                    None => ptr::null(),
                }
            }

            /// Free an error produced by the engine. A null pointer is a no-op.
            ///
            /// # Safety
            /// `err` must be a `ForgeError` from an engine out-param, freed once.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_error_free(err: *mut ForgeError) {
                if err.is_null() {
                    return;
                }
                drop(Box::from_raw(err));
            }

            /// Free a byte buffer the engine handed out — a JSON record/id buffer
            /// from a per-model row op, or (later) the gathered-buffer half of the
            /// Arrow C-Data-Interface `release` path (an alias buffer is a no-op
            /// release, never this). A null pointer is a no-op.
            ///
            /// # Safety
            /// `ptr`/`len` must be a buffer produced by a ForgeDB engine call
            /// whose contract names `forgedb_free_buffer` as its releaser (length
            /// == capacity), freed once.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_free_buffer(ptr: *mut u8, len: usize) {
                if ptr.is_null() {
                    return;
                }
                drop(Vec::from_raw_parts(ptr, len, len));
            }

            /// Capture a cross-model-consistent read snapshot (#56, Direction A):
            /// a row-count watermark for every model and junction, taken together
            /// on the single writer so the view is atomic as of one commit
            /// boundary.  Returns an owned `*mut Snapshot` the caller frees with
            /// `forgedb_snapshot_free`, or null with `*err_out` set on panic.
            /// Pass it to a `forgedb_<m>_get_at` / `_all_at` / M2M `_at` entry
            /// point for a point-in-time read (the wire token is opaque — the
            /// caller never sees or forges a watermark integer).
            ///
            /// # Safety
            /// `db` must be a live handle from `forgedb_open`; `err_out`, when
            /// non-null, must point to writable storage for one `*mut ForgeError`.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_snapshot(
                db: *mut Db,
                err_out: *mut *mut ForgeError,
            ) -> *mut Snapshot {
                clear_err(err_out);
                let Some(db) = db.as_ref() else {
                    set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                    return ptr::null_mut();
                };
                match catch_unwind(AssertUnwindSafe(|| db.inner.snapshot())) {
                    Ok(inner) => Box::into_raw(Box::new(Snapshot { inner })),
                    Err(payload) => {
                        set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                        ptr::null_mut()
                    }
                }
            }

            /// Free a snapshot captured by `forgedb_snapshot`.  A null pointer is
            /// a no-op; the handle is invalid afterward.
            ///
            /// # Safety
            /// `snap` must be a handle from `forgedb_snapshot`, freed once.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_snapshot_free(snap: *mut Snapshot) {
                if snap.is_null() {
                    return;
                }
                drop(Box::from_raw(snap));
            }

            // --- Async completion bridge (the Layer-0 half) --------------------
            //
            // The `_async` entry points (generated per model below) enqueue their
            // work on a single process-wide background worker thread and return
            // immediately, so a foreign event loop (Node/Bun/Python) never blocks
            // on the engine's synchronous, fsync-ing per-row API.  On completion
            // the worker invokes the caller-registered completion callback with
            // the op's `token` plus the result as OPAQUE JSON bytes (the SAME
            // marshalling as the sync out-params) — the Layer-1 per-runtime bridge
            // (NAPI-RS `ThreadsafeFunction` / PyO3 `pyo3-async-runtimes`) is what
            // turns that callback into a resolved Promise/future keyed by `token`.
            //
            // v1 executor = ONE worker thread: every async op (read AND write)
            // serializes on it.  That delivers the async value — event-loop
            // stall-avoidance — and keeps writes serialized for free, but does NOT
            // yet parallelize reads; an L0-owned reader pool + a dedicated writer
            // thread (P2 in the design note) is the scaling follow-up.  Writer-turn
            // serialization stays a property of the shared generated engine
            // (acceptance constraint 5) — this bridge only moves the blocking call
            // off the caller thread, it never conjures write concurrency.
            //
            // THREADING CONTRACT: while async ops are outstanding on a handle the
            // worker thread is its SOLE accessor.  A caller using the async surface
            // for a `Db` must not concurrently call any other (sync or async) entry
            // point on that same handle, and must quiesce its async ops before
            // `forgedb_close`.  `Db` is `Send` (statically asserted below), so
            // moving access to the worker is sound as long as it never overlaps.

            /// The async completion callback.  `status` is `0` on success (with
            /// `payload`/`payload_len` the JSON result, or null for a void/absent
            /// result) else a `FORGEDB_ERR_*` code (positive — the SAME values
            /// `forgedb_error_code` returns; `payload` = the UTF-8 error message,
            /// or null).  The callback must copy what it needs and free a non-null
            /// `payload` with `forgedb_free_buffer` — the same ownership contract
            /// as a sync out-buffer.
            pub type ForgeCompletion =
                extern "C" fn(token: u64, status: i32, payload: *mut u8, payload_len: usize);

            /// Process-wide completion callback address (0 = unregistered), set by
            /// `forgedb_set_completion_callback`.  An `AtomicUsize` so the worker
            /// loads it lock-free.
            static COMPLETION_CB: AtomicUsize = AtomicUsize::new(0);

            /// A `*mut Db` promised (by the threading contract above) safe to touch
            /// from the single async worker thread.  `Db` is `Send`, so the only
            /// requirement this wrapper adds is non-overlap, which the contract
            /// gives.
            struct SendDb(*mut Db);
            unsafe impl Send for SendDb {}

            impl SendDb {
                /// Reborrow the wrapped handle on the worker thread.  Going
                /// through a `&self` method forces the enclosing `move` closure to
                /// capture the whole (`Send`) `SendDb`, not the inner non-`Send`
                /// `*mut Db` field — 2021+ disjoint closure captures would
                /// otherwise capture just the field and make the job `!Send`.
                ///
                /// # Safety
                /// The async threading contract: while the job runs, the worker is
                /// the sole accessor of this handle (the caller does not touch it).
                #[allow(clippy::mut_from_ref)]
                unsafe fn as_mut<'a>(&self) -> &'a mut Db {
                    unsafe { &mut *self.0 }
                }
            }

            // Static proof that `Db` (hence the engine `Database`) is `Send`, so
            // handing its access to the worker thread is sound.  A non-`Send`
            // engine fails the FFI build HERE rather than shipping a data race.
            const _: fn() = || {
                fn assert_send<T: Send>() {}
                assert_send::<Db>();
            };

            /// Register (or clear, with a null pointer) the process-wide async
            /// completion callback.  The Layer-1 per-runtime bridge sets this once
            /// at startup; an op whose completion fires with no callback registered
            /// is dropped (the contract is: register before issuing async ops).
            ///
            /// # Safety
            /// `cb`, when non-null, must be a valid `extern "C"` function pointer
            /// that stays valid as long as async ops may complete.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_set_completion_callback(cb: Option<ForgeCompletion>) {
                let addr = match cb {
                    Some(f) => f as usize,
                    None => 0,
                };
                COMPLETION_CB.store(addr, Ordering::SeqCst);
            }

            /// Load the registered completion callback, if any.
            fn load_completion_cb() -> Option<ForgeCompletion> {
                let addr = COMPLETION_CB.load(Ordering::SeqCst);
                if addr == 0 {
                    None
                } else {
                    // SAFETY: `addr` is a `ForgeCompletion` pointer stored by
                    // `forgedb_set_completion_callback` (0 handled above).
                    Some(unsafe { std::mem::transmute::<usize, ForgeCompletion>(addr) })
                }
            }

            /// Deliver an async op's outcome to the registered callback: `Ok`
            /// → status 0 (payload = the JSON result, or `None` for a void/absent
            /// result), `Err` → the (positive) `FORGEDB_ERR_*` code with the
            /// message bytes.  A payload buffer is handed out cap==len (freed by
            /// the caller with `forgedb_free_buffer`), exactly like a sync
            /// out-param.
            fn fire_completion(token: u64, outcome: Result<Option<Vec<u8>>, (i32, String)>) {
                let Some(cb) = load_completion_cb() else { return };
                let (status, bytes) = match outcome {
                    Ok(payload) => (0, payload),
                    Err((code, msg)) => (code, Some(msg.into_bytes())),
                };
                match bytes {
                    Some(v) => {
                        let boxed = v.into_boxed_slice();
                        let len = boxed.len();
                        let ptr = Box::into_raw(boxed) as *mut u8;
                        cb(token, status, ptr, len);
                    }
                    None => cb(token, status, ptr::null_mut(), 0),
                }
            }

            /// The single background worker thread's job queue, lazily started on
            /// the first async op — a daemon for the life of the process.
            fn async_executor() -> &'static Mutex<Sender<Box<dyn FnOnce() + Send>>> {
                static EXECUTOR: OnceLock<Mutex<Sender<Box<dyn FnOnce() + Send>>>> = OnceLock::new();
                EXECUTOR.get_or_init(|| {
                    let (tx, rx) = mpsc::channel::<Box<dyn FnOnce() + Send>>();
                    let _ = thread::Builder::new()
                        .name("forgedb-async".to_string())
                        .spawn(move || {
                            while let Ok(job) = rx.recv() {
                                job();
                            }
                        });
                    Mutex::new(tx)
                })
            }

            /// Enqueue a job on the async worker thread.
            fn spawn_async<F: FnOnce() + Send + 'static>(f: F) {
                if let Ok(tx) = async_executor().lock() {
                    let _ = tx.send(Box::new(f));
                }
            }

            #(#async_ops)*

            // --- Arrow columnar export (schema-invariant spine) ---------------
            // The zero-copy selling point: hand a whole live column to the caller
            // as an Arrow C-Data-Interface array (importable by pyarrow / arrow-js
            // / polars with no per-row JSON).  These two structs are the Arrow ABI
            // verbatim (`arrow/c/abi.h`); the per-column ops below fill them.  The
            // buffer is a zero-copy `mmap` ALIAS of the on-disk column when the
            // live rows are a dense prefix `[0, n)`, and a gathered heap copy
            // otherwise — the `forgedb_storage::ColumnExport` owner carries either
            // behind one `release` callback (drop = free copy / `munmap` alias).

            /// Arrow C Data Interface schema struct (`struct ArrowSchema`).
            #[repr(C)]
            pub struct ArrowSchema {
                format: *const c_char,
                name: *const c_char,
                metadata: *const c_char,
                flags: i64,
                n_children: i64,
                children: *mut *mut ArrowSchema,
                dictionary: *mut ArrowSchema,
                release: Option<unsafe extern "C" fn(*mut ArrowSchema)>,
                private_data: *mut c_void,
            }

            /// Arrow C Data Interface array struct (`struct ArrowArray`).
            #[repr(C)]
            pub struct ArrowArray {
                length: i64,
                null_count: i64,
                offset: i64,
                n_buffers: i64,
                n_children: i64,
                buffers: *mut *const c_void,
                children: *mut *mut ArrowArray,
                dictionary: *mut ArrowArray,
                release: Option<unsafe extern "C" fn(*mut ArrowArray)>,
                private_data: *mut c_void,
            }

            /// Owns everything an exported `ArrowArray` points at: the column
            /// export backing `buffers[1]` (either a zero-copy `mmap` alias or a
            /// gathered `Vec` — a `forgedb_storage::ColumnExport`, transparent to
            /// this box) and the two-element buffer pointer array itself.  The
            /// `release` callback reclaims this box and drops it — dropping the
            /// `ColumnExport` frees the copy *or* `munmap`s the alias, as needed.
            struct ArrowArrayOwner {
                _export: forgedb_core::forgedb_storage::ColumnExport,
                _buffers: Vec<*const c_void>,
            }

            /// Arrow `release` for an exported array: reclaim + drop the owner box,
            /// then null `release` (the Arrow protocol's released marker).
            unsafe extern "C" fn arrow_array_release(array: *mut ArrowArray) {
                if array.is_null() {
                    return;
                }
                let a = &mut *array;
                if a.release.is_none() {
                    return;
                }
                if !a.private_data.is_null() {
                    drop(Box::from_raw(a.private_data as *mut ArrowArrayOwner));
                }
                a.private_data = ptr::null_mut();
                a.release = None;
            }

            /// Arrow `release` for the schema: `format` is a `'static` C-string
            /// literal and `name`/`metadata` are null, so there is nothing to free
            /// — just null `release`.
            unsafe extern "C" fn arrow_schema_release(schema: *mut ArrowSchema) {
                if schema.is_null() {
                    return;
                }
                let s = &mut *schema;
                s.release = None;
            }

            /// Fill `out_schema`/`out_array` for a non-null fixed-width primitive
            /// column: two buffers (validity = null since `null_count == 0`, then
            /// the exported data), `length` values, no children.  `format` is a
            /// `'static` Arrow format C-string; `export`'s buffer (mmap alias or
            /// gathered heap `Vec`) backs `buffers[1]` and outlives the call inside
            /// the owner box.
            unsafe fn fill_arrow_primitive(
                out_schema: *mut ArrowSchema,
                out_array: *mut ArrowArray,
                format: *const c_char,
                export: forgedb_core::forgedb_storage::ColumnExport,
                length: usize,
            ) {
                // The export's pointer (mmap address or `Vec` heap allocation) is
                // stable across the move into the owner box, so capturing it first
                // is sound.
                let data_ptr = export.as_ptr() as *const c_void;
                let buffers: Vec<*const c_void> = vec![ptr::null(), data_ptr];
                let buffers_ptr = buffers.as_ptr() as *mut *const c_void;
                let owner = Box::new(ArrowArrayOwner { _export: export, _buffers: buffers });

                *out_array = ArrowArray {
                    length: length as i64,
                    null_count: 0,
                    offset: 0,
                    n_buffers: 2,
                    n_children: 0,
                    buffers: buffers_ptr,
                    children: ptr::null_mut(),
                    dictionary: ptr::null_mut(),
                    release: Some(arrow_array_release),
                    private_data: Box::into_raw(owner) as *mut c_void,
                };
                *out_schema = ArrowSchema {
                    format,
                    name: ptr::null(),
                    metadata: ptr::null(),
                    flags: 0,
                    n_children: 0,
                    children: ptr::null_mut(),
                    dictionary: ptr::null_mut(),
                    release: Some(arrow_schema_release),
                    private_data: ptr::null_mut(),
                };
            }

            #(#arrow_ops)*

            // --- Per-model row ops (the schema-tailored half of the fat ABI) ---
            // These reference the generated per-model structs + integrity wrappers
            // by name — they ARE tailored per schema (that is the point) — but they
            // still invent no generic query surface: rows and ids cross the C-ABI
            // as OPAQUE JSON bytes (the same opaque-bytes discipline as the WAL /
            // broker / wasm-replica paths), decoded into the generated struct via
            // serde at a compile-time-known type.  There is no `forgedb_query`,
            // no runtime predicate, no `match model` dispatch.
            #(#model_ops)*

            // --- Relation-traversal ops (forward FK / reverse 1:M / M2M) -------
            // These mirror the generated `Database` traversal getters one-for-one
            // (same names, derived from the same schema), calling them by name and
            // returning the resolved record(s) as opaque JSON.  Still no generic
            // query surface — each getter is a fixed, generated edge walk, never a
            // runtime predicate.
            #(#relation_ops)*
        };

        let syntax_tree = syn::parse_file(&tokens.to_string()).map_err(|e| {
            crate::CodegenError::GenerationFailed(format!("Failed to parse generated ffi spine: {e}"))
        })?;
        let code = prettyplease::unparse(&syntax_tree);

        Ok(GeneratedCode {
            code,
            description: format!(
                "native FFI Layer-0 C-ABI spine ({} models)",
                schema.models.len()
            ),
        })
    }

    /// Generate the per-model row-op C-ABI functions (the schema-tailored half of
    /// the fat ABI): for each identity model `M` with snake name `m` and id type
    /// `IdT`, six OLTP entry points —
    ///   * `forgedb_<m>_insert(db, rec, rec_len, id_out, id_len_out, err)` →
    ///     decodes a full `M` record from JSON, inserts it through the generated
    ///     `Database::create_<m>` integrity wrapper (FKs must resolve, then field
    ///     constraints + `&unique`), and hands the new id back as JSON bytes;
    ///   * `forgedb_<m>_get(db, id, id_len, out, out_len, err)` → resolves the id
    ///     (JSON) and emits the record as JSON, or leaves `*out` null if absent;
    ///   * `forgedb_<m>_count(db, err)` → the live row count as `i64` (`-1` on err);
    ///   * `forgedb_<m>_all(db, out, out_len, err)` → every live record as a JSON
    ///     array (the always-correct fallback; the columnar/Arrow export is a
    ///     later phase);
    ///   * `forgedb_<m>_update(db, id, id_len, rec, rec_len, err)` → `i32`:
    ///     `1` updated / `0` absent / `-1` error;
    ///   * `forgedb_<m>_delete(db, id, id_len, err)` → `i32`: `1` deleted /
    ///     `0` absent / `-1` error, through the referential-integrity
    ///     `Database::delete_<m>` wrapper;
    ///   * `forgedb_<m>_get_at(db, snap, id, id_len, out, out_len, err)` /
    ///     `forgedb_<m>_all_at(db, snap, out, out_len, err)` → the point-in-time
    ///     (#56) reads: same as `_get`/`_all` but resolved as of a
    ///     `forgedb_snapshot`-captured watermark (`db.<m>.get_at`/`all_at`), so a
    ///     row appended after the capture is invisible.
    ///
    /// Rows/ids cross as opaque JSON bytes and are decoded into the generated
    /// struct / id type via serde — schema-tailored at a compile-time-known type,
    /// never a runtime schema read.  Every engine call is `catch_unwind`-guarded
    /// (an unwind across `extern "C"` is UB), and a rejected write becomes a
    /// `FORGEDB_ERR_VALIDATION` error rather than a panic.
    fn generate_model_ops(schema: &Schema, p: &str) -> Vec<proc_macro2::TokenStream> {
        schema
            .models
            .iter()
            .filter(|m| m.has_identity())
            .map(|model| {
                let snake = RustGenerator::to_snake_case(&model.name);
                let model_ident = format_ident!("{}", model.name);
                let storage = format_ident!("{}", snake);
                let id_ty = RustGenerator::id_type_tokens(schema, model);
                let create_fn = format_ident!("create_{}", snake);
                let update_fn = format_ident!("update_{}", snake);
                let delete_fn = format_ident!("delete_{}", snake);

                let insert_sym = format_ident!("{}{}_insert", p, snake);
                let get_sym = format_ident!("{}{}_get", p, snake);
                let count_sym = format_ident!("{}{}_count", p, snake);
                let all_sym = format_ident!("{}{}_all", p, snake);
                let update_sym = format_ident!("{}{}_update", p, snake);
                let delete_sym = format_ident!("{}{}_delete", p, snake);
                let get_at_sym = format_ident!("{}{}_get_at", p, snake);
                let all_at_sym = format_ident!("{}{}_all_at", p, snake);
                // The `DatabaseSnapshot` field for this model is named by its
                // snake name (same as the storage field), so `snap.inner.<snake>`
                // is this collection's captured watermark.
                let snap_field = format_ident!("{}", snake);

                let insert_doc =
                    format!("Insert a `{}` (JSON record → new id JSON) via the integrity wrapper.", model.name);
                let get_doc =
                    format!("Fetch a `{}` by id (JSON → record JSON; `*out` null if absent).", model.name);
                let count_doc = format!("Live `{}` row count (`-1` on error).", model.name);
                let all_doc = format!("Every live `{}` as a JSON array.", model.name);
                let update_doc =
                    format!("Update a `{}` by id (1 updated / 0 absent / -1 error).", model.name);
                let delete_doc =
                    format!("Delete a `{}` by id with referential integrity (1 / 0 / -1).", model.name);
                let get_at_doc = format!(
                    "Fetch a `{}` by id as of a snapshot (JSON → record JSON; `*out` null if absent then).",
                    model.name
                );
                let all_at_doc =
                    format!("Every live `{}` as of a snapshot, as a JSON array.", model.name);

                quote! {
                    #[doc = #insert_doc]
                    ///
                    /// # Safety
                    /// `db` is a live handle; `record`/`record_len` a readable JSON
                    /// buffer; `id_out`/`id_len_out`/`err_out`, when non-null,
                    /// writable.  On success `*id_out` is a buffer freed with
                    /// `forgedb_free_buffer`.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #insert_sym(
                        db: *mut Db,
                        record: *const u8,
                        record_len: usize,
                        id_out: *mut *mut u8,
                        id_len_out: *mut usize,
                        err_out: *mut *mut ForgeError,
                    ) -> bool {
                        clear_err(err_out);
                        if !id_out.is_null() { *id_out = ptr::null_mut(); }
                        if !id_len_out.is_null() { *id_len_out = 0; }
                        let Some(db) = db.as_mut() else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                            return false;
                        };
                        let Some(bytes) = read_bytes(record, record_len) else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "record buffer is null".to_string());
                            return false;
                        };
                        let record: database::#model_ident = match serde_json::from_slice(bytes) {
                            Ok(r) => r,
                            Err(e) => {
                                set_error(err_out, FORGEDB_ERR_INVALID_ARG, format!("invalid record JSON: {e}"));
                                return false;
                            }
                        };
                        match catch_unwind(AssertUnwindSafe(|| db.inner.#create_fn(record))) {
                            Ok(Ok(id)) => match serde_json::to_vec(&id) {
                                Ok(json) => {
                                    emit_bytes(json, id_out, id_len_out);
                                    true
                                }
                                Err(e) => {
                                    set_error(err_out, FORGEDB_ERR_IO, format!("id serialize: {e}"));
                                    false
                                }
                            },
                            Ok(Err(e)) => {
                                set_error(err_out, FORGEDB_ERR_VALIDATION, e.to_string());
                                false
                            }
                            Err(payload) => {
                                set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                                false
                            }
                        }
                    }

                    #[doc = #get_doc]
                    ///
                    /// # Safety
                    /// `db` is a live handle; `id`/`id_len` a readable JSON id
                    /// buffer; `out`/`out_len`/`err_out`, when non-null, writable.
                    /// On a hit `*out` is a buffer freed with `forgedb_free_buffer`.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #get_sym(
                        db: *mut Db,
                        id: *const u8,
                        id_len: usize,
                        out: *mut *mut u8,
                        out_len: *mut usize,
                        err_out: *mut *mut ForgeError,
                    ) -> bool {
                        clear_err(err_out);
                        if !out.is_null() { *out = ptr::null_mut(); }
                        if !out_len.is_null() { *out_len = 0; }
                        let Some(db) = db.as_ref() else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                            return false;
                        };
                        let Some(id_bytes) = read_bytes(id, id_len) else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "id buffer is null".to_string());
                            return false;
                        };
                        let id: #id_ty = match serde_json::from_slice(id_bytes) {
                            Ok(v) => v,
                            Err(e) => {
                                set_error(err_out, FORGEDB_ERR_INVALID_ARG, format!("invalid id JSON: {e}"));
                                return false;
                            }
                        };
                        match catch_unwind(AssertUnwindSafe(|| db.inner.#storage.get(id))) {
                            Ok(Some(record)) => match serde_json::to_vec(&record) {
                                Ok(json) => {
                                    emit_bytes(json, out, out_len);
                                    true
                                }
                                Err(e) => {
                                    set_error(err_out, FORGEDB_ERR_IO, format!("record serialize: {e}"));
                                    false
                                }
                            },
                            // Absent: success with `*out` left null (not an error).
                            Ok(None) => true,
                            Err(payload) => {
                                set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                                false
                            }
                        }
                    }

                    #[doc = #count_doc]
                    ///
                    /// # Safety
                    /// `db` is a live handle; `err_out`, when non-null, writable.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #count_sym(
                        db: *mut Db,
                        err_out: *mut *mut ForgeError,
                    ) -> i64 {
                        clear_err(err_out);
                        let Some(db) = db.as_ref() else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                            return -1;
                        };
                        match catch_unwind(AssertUnwindSafe(|| db.inner.#storage.row_count())) {
                            Ok(n) => n as i64,
                            Err(payload) => {
                                set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                                -1
                            }
                        }
                    }

                    #[doc = #all_doc]
                    ///
                    /// # Safety
                    /// `db` is a live handle; `out`/`out_len`/`err_out`, when
                    /// non-null, writable.  On success `*out` is a buffer freed
                    /// with `forgedb_free_buffer`.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #all_sym(
                        db: *mut Db,
                        out: *mut *mut u8,
                        out_len: *mut usize,
                        err_out: *mut *mut ForgeError,
                    ) -> bool {
                        clear_err(err_out);
                        if !out.is_null() { *out = ptr::null_mut(); }
                        if !out_len.is_null() { *out_len = 0; }
                        let Some(db) = db.as_ref() else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                            return false;
                        };
                        match catch_unwind(AssertUnwindSafe(|| db.inner.#storage.all())) {
                            Ok(records) => match serde_json::to_vec(&records) {
                                Ok(json) => {
                                    emit_bytes(json, out, out_len);
                                    true
                                }
                                Err(e) => {
                                    set_error(err_out, FORGEDB_ERR_IO, format!("records serialize: {e}"));
                                    false
                                }
                            },
                            Err(payload) => {
                                set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                                false
                            }
                        }
                    }

                    #[doc = #update_doc]
                    ///
                    /// # Safety
                    /// `db` is a live handle; `id`/`id_len` + `record`/`record_len`
                    /// readable JSON buffers; `err_out`, when non-null, writable.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #update_sym(
                        db: *mut Db,
                        id: *const u8,
                        id_len: usize,
                        record: *const u8,
                        record_len: usize,
                        err_out: *mut *mut ForgeError,
                    ) -> i32 {
                        clear_err(err_out);
                        let Some(db) = db.as_mut() else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                            return -1;
                        };
                        let Some(id_bytes) = read_bytes(id, id_len) else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "id buffer is null".to_string());
                            return -1;
                        };
                        let id: #id_ty = match serde_json::from_slice(id_bytes) {
                            Ok(v) => v,
                            Err(e) => {
                                set_error(err_out, FORGEDB_ERR_INVALID_ARG, format!("invalid id JSON: {e}"));
                                return -1;
                            }
                        };
                        let Some(rec_bytes) = read_bytes(record, record_len) else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "record buffer is null".to_string());
                            return -1;
                        };
                        let record: database::#model_ident = match serde_json::from_slice(rec_bytes) {
                            Ok(r) => r,
                            Err(e) => {
                                set_error(err_out, FORGEDB_ERR_INVALID_ARG, format!("invalid record JSON: {e}"));
                                return -1;
                            }
                        };
                        match catch_unwind(AssertUnwindSafe(|| db.inner.#update_fn(id, record))) {
                            Ok(Ok(true)) => 1,
                            Ok(Ok(false)) => 0,
                            Ok(Err(e)) => {
                                set_error(err_out, FORGEDB_ERR_VALIDATION, e.to_string());
                                -1
                            }
                            Err(payload) => {
                                set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                                -1
                            }
                        }
                    }

                    #[doc = #delete_doc]
                    ///
                    /// # Safety
                    /// `db` is a live handle; `id`/`id_len` a readable JSON id
                    /// buffer; `err_out`, when non-null, writable.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #delete_sym(
                        db: *mut Db,
                        id: *const u8,
                        id_len: usize,
                        err_out: *mut *mut ForgeError,
                    ) -> i32 {
                        clear_err(err_out);
                        let Some(db) = db.as_mut() else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                            return -1;
                        };
                        let Some(id_bytes) = read_bytes(id, id_len) else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "id buffer is null".to_string());
                            return -1;
                        };
                        let id: #id_ty = match serde_json::from_slice(id_bytes) {
                            Ok(v) => v,
                            Err(e) => {
                                set_error(err_out, FORGEDB_ERR_INVALID_ARG, format!("invalid id JSON: {e}"));
                                return -1;
                            }
                        };
                        match catch_unwind(AssertUnwindSafe(|| db.inner.#delete_fn(id))) {
                            Ok(Ok(true)) => 1,
                            Ok(Ok(false)) => 0,
                            Ok(Err(e)) => {
                                set_error(err_out, FORGEDB_ERR_VALIDATION, e.to_string());
                                -1
                            }
                            Err(payload) => {
                                set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                                -1
                            }
                        }
                    }

                    #[doc = #get_at_doc]
                    ///
                    /// # Safety
                    /// `db`/`snap` are live handles; `id`/`id_len` a readable JSON
                    /// id buffer; `out`/`out_len`/`err_out`, when non-null,
                    /// writable.  On a hit `*out` is a buffer freed with
                    /// `forgedb_free_buffer`.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #get_at_sym(
                        db: *mut Db,
                        snap: *const Snapshot,
                        id: *const u8,
                        id_len: usize,
                        out: *mut *mut u8,
                        out_len: *mut usize,
                        err_out: *mut *mut ForgeError,
                    ) -> bool {
                        clear_err(err_out);
                        if !out.is_null() { *out = ptr::null_mut(); }
                        if !out_len.is_null() { *out_len = 0; }
                        let Some(db) = db.as_ref() else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                            return false;
                        };
                        let Some(snap) = snap.as_ref() else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "snapshot handle is null".to_string());
                            return false;
                        };
                        let Some(id_bytes) = read_bytes(id, id_len) else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "id buffer is null".to_string());
                            return false;
                        };
                        let id: #id_ty = match serde_json::from_slice(id_bytes) {
                            Ok(v) => v,
                            Err(e) => {
                                set_error(err_out, FORGEDB_ERR_INVALID_ARG, format!("invalid id JSON: {e}"));
                                return false;
                            }
                        };
                        match catch_unwind(AssertUnwindSafe(|| {
                            db.inner.#storage.get_at(&snap.inner.#snap_field, id)
                        })) {
                            Ok(Some(record)) => match serde_json::to_vec(&record) {
                                Ok(json) => { emit_bytes(json, out, out_len); true }
                                Err(e) => {
                                    set_error(err_out, FORGEDB_ERR_IO, format!("record serialize: {e}"));
                                    false
                                }
                            },
                            // Absent as of the snapshot: success, `*out` left null.
                            Ok(None) => true,
                            Err(payload) => {
                                set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                                false
                            }
                        }
                    }

                    #[doc = #all_at_doc]
                    ///
                    /// # Safety
                    /// `db`/`snap` are live handles; `out`/`out_len`/`err_out`,
                    /// when non-null, writable.  On success `*out` is a buffer
                    /// freed with `forgedb_free_buffer`.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #all_at_sym(
                        db: *mut Db,
                        snap: *const Snapshot,
                        out: *mut *mut u8,
                        out_len: *mut usize,
                        err_out: *mut *mut ForgeError,
                    ) -> bool {
                        clear_err(err_out);
                        if !out.is_null() { *out = ptr::null_mut(); }
                        if !out_len.is_null() { *out_len = 0; }
                        let Some(db) = db.as_ref() else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                            return false;
                        };
                        let Some(snap) = snap.as_ref() else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "snapshot handle is null".to_string());
                            return false;
                        };
                        match catch_unwind(AssertUnwindSafe(|| {
                            db.inner.#storage.all_at(&snap.inner.#snap_field)
                        })) {
                            Ok(records) => match serde_json::to_vec(&records) {
                                Ok(json) => { emit_bytes(json, out, out_len); true }
                                Err(e) => {
                                    set_error(err_out, FORGEDB_ERR_IO, format!("records serialize: {e}"));
                                    false
                                }
                            },
                            Err(payload) => {
                                set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                                false
                            }
                        }
                    }
                }
            })
            .collect()
    }

    /// Generate the per-model async C-ABI entry points (the `_async` completion
    /// bridge — the schema-tailored half over the schema-invariant executor +
    /// callback registration emitted in the spine).  For each identity model `M`
    /// (snake name `m`, id type `IdT`), the six OLTP `_async` variants —
    ///   * `forgedb_<m>_get_async(db, id, id_len, token)`
    ///   * `forgedb_<m>_all_async(db, token)`
    ///   * `forgedb_<m>_count_async(db, token)`
    ///   * `forgedb_<m>_insert_async(db, record, record_len, token)`
    ///   * `forgedb_<m>_update_async(db, id, id_len, record, record_len, token)`
    ///   * `forgedb_<m>_delete_async(db, id, id_len, token)`
    ///
    /// Each decodes its opaque-JSON args on the CALLER thread (so the caller's
    /// buffers never outlive the call), then enqueues the blocking engine call on
    /// the single background worker (`spawn_async`) and returns immediately —
    /// `void`, per the pinned ABI.  On completion the worker fires the registered
    /// callback with the op's `token` and the result (or error) as opaque JSON
    /// bytes (`fire_completion`).  These call the SAME generated integrity
    /// wrappers / storage reads as the sync ops — never a second write/read path
    /// — and still invent no generic query surface.  A bad arg or a rejected
    /// write is delivered through the callback (a positive `FORGEDB_ERR_*`
    /// status, as `forgedb_error_code` uses), not a panic;
    /// the engine call itself stays `catch_unwind`-guarded so a panic keeps the
    /// worker alive and becomes a completion instead of killing the thread.
    fn generate_async_ops(schema: &Schema, p: &str) -> Vec<proc_macro2::TokenStream> {
        schema
            .models
            .iter()
            .filter(|m| m.has_identity())
            .map(|model| {
                let snake = RustGenerator::to_snake_case(&model.name);
                let model_ident = format_ident!("{}", model.name);
                let storage = format_ident!("{}", snake);
                let id_ty = RustGenerator::id_type_tokens(schema, model);
                let create_fn = format_ident!("create_{}", snake);
                let update_fn = format_ident!("update_{}", snake);
                let delete_fn = format_ident!("delete_{}", snake);

                let get_sym = format_ident!("{}{}_get_async", p, snake);
                let all_sym = format_ident!("{}{}_all_async", p, snake);
                let count_sym = format_ident!("{}{}_count_async", p, snake);
                let insert_sym = format_ident!("{}{}_insert_async", p, snake);
                let update_sym = format_ident!("{}{}_update_async", p, snake);
                let delete_sym = format_ident!("{}{}_delete_async", p, snake);

                let get_doc = format!("Async fetch a `{}` by id — completion payload = record JSON (null = absent).", model.name);
                let all_doc = format!("Async fetch every live `{}` — completion payload = JSON array.", model.name);
                let count_doc = format!("Async live `{}` count — completion payload = JSON number.", model.name);
                let insert_doc = format!("Async insert a `{}` via the integrity wrapper — completion payload = new id JSON.", model.name);
                let update_doc = format!("Async update a `{}` by id — completion payload = JSON bool (present/absent).", model.name);
                let delete_doc = format!("Async delete a `{}` by id (referential) — completion payload = JSON bool.", model.name);

                quote! {
                    #[doc = #get_doc]
                    ///
                    /// # Safety
                    /// `db` is a live handle; `id`/`id_len` a readable JSON id
                    /// buffer read before this returns.  See the async threading
                    /// contract above.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #get_sym(
                        db: *mut Db,
                        id: *const u8,
                        id_len: usize,
                        token: u64,
                    ) {
                        if db.is_null() {
                            fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string())));
                            return;
                        }
                        let id: #id_ty = match read_bytes(id, id_len) {
                            Some(b) => match serde_json::from_slice(b) {
                                Ok(v) => v,
                                Err(e) => {
                                    fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, format!("invalid id JSON: {e}"))));
                                    return;
                                }
                            },
                            None => {
                                fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, "id buffer is null".to_string())));
                                return;
                            }
                        };
                        let sdb = SendDb(db);
                        spawn_async(move || {
                            let outcome = (|| -> Result<Option<Vec<u8>>, (i32, String)> {
                                // SAFETY: async threading contract — the worker is
                                // the sole accessor of this handle.
                                let db = unsafe { sdb.as_mut() };
                                match catch_unwind(AssertUnwindSafe(|| db.inner.#storage.get(id))) {
                                    Ok(Some(rec)) => serde_json::to_vec(&rec)
                                        .map(Some)
                                        .map_err(|e| (FORGEDB_ERR_IO, format!("record serialize: {e}"))),
                                    Ok(None) => Ok(None),
                                    Err(p) => Err((FORGEDB_ERR_PANIC, panic_message(p))),
                                }
                            })();
                            fire_completion(token, outcome);
                        });
                    }

                    #[doc = #all_doc]
                    ///
                    /// # Safety
                    /// `db` is a live handle.  See the async threading contract.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #all_sym(db: *mut Db, token: u64) {
                        if db.is_null() {
                            fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string())));
                            return;
                        }
                        let sdb = SendDb(db);
                        spawn_async(move || {
                            let outcome = (|| -> Result<Option<Vec<u8>>, (i32, String)> {
                                let db = unsafe { sdb.as_mut() };
                                match catch_unwind(AssertUnwindSafe(|| db.inner.#storage.all())) {
                                    Ok(records) => serde_json::to_vec(&records)
                                        .map(Some)
                                        .map_err(|e| (FORGEDB_ERR_IO, format!("records serialize: {e}"))),
                                    Err(p) => Err((FORGEDB_ERR_PANIC, panic_message(p))),
                                }
                            })();
                            fire_completion(token, outcome);
                        });
                    }

                    #[doc = #count_doc]
                    ///
                    /// # Safety
                    /// `db` is a live handle.  See the async threading contract.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #count_sym(db: *mut Db, token: u64) {
                        if db.is_null() {
                            fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string())));
                            return;
                        }
                        let sdb = SendDb(db);
                        spawn_async(move || {
                            let outcome = (|| -> Result<Option<Vec<u8>>, (i32, String)> {
                                let db = unsafe { sdb.as_mut() };
                                match catch_unwind(AssertUnwindSafe(|| db.inner.#storage.row_count())) {
                                    Ok(n) => serde_json::to_vec(&(n as u64))
                                        .map(Some)
                                        .map_err(|e| (FORGEDB_ERR_IO, format!("count serialize: {e}"))),
                                    Err(p) => Err((FORGEDB_ERR_PANIC, panic_message(p))),
                                }
                            })();
                            fire_completion(token, outcome);
                        });
                    }

                    #[doc = #insert_doc]
                    ///
                    /// # Safety
                    /// `db` is a live handle; `record`/`record_len` a readable JSON
                    /// buffer read before this returns.  See the async contract.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #insert_sym(
                        db: *mut Db,
                        record: *const u8,
                        record_len: usize,
                        token: u64,
                    ) {
                        if db.is_null() {
                            fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string())));
                            return;
                        }
                        let record: database::#model_ident = match read_bytes(record, record_len) {
                            Some(b) => match serde_json::from_slice(b) {
                                Ok(r) => r,
                                Err(e) => {
                                    fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, format!("invalid record JSON: {e}"))));
                                    return;
                                }
                            },
                            None => {
                                fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, "record buffer is null".to_string())));
                                return;
                            }
                        };
                        let sdb = SendDb(db);
                        spawn_async(move || {
                            let outcome = (|| -> Result<Option<Vec<u8>>, (i32, String)> {
                                let db = unsafe { sdb.as_mut() };
                                match catch_unwind(AssertUnwindSafe(move || db.inner.#create_fn(record))) {
                                    Ok(Ok(id)) => serde_json::to_vec(&id)
                                        .map(Some)
                                        .map_err(|e| (FORGEDB_ERR_IO, format!("id serialize: {e}"))),
                                    Ok(Err(e)) => Err((FORGEDB_ERR_VALIDATION, e.to_string())),
                                    Err(p) => Err((FORGEDB_ERR_PANIC, panic_message(p))),
                                }
                            })();
                            fire_completion(token, outcome);
                        });
                    }

                    #[doc = #update_doc]
                    ///
                    /// # Safety
                    /// `db` is a live handle; `id`/`record` (+ lens) readable JSON
                    /// buffers read before this returns.  See the async contract.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #update_sym(
                        db: *mut Db,
                        id: *const u8,
                        id_len: usize,
                        record: *const u8,
                        record_len: usize,
                        token: u64,
                    ) {
                        if db.is_null() {
                            fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string())));
                            return;
                        }
                        let id: #id_ty = match read_bytes(id, id_len) {
                            Some(b) => match serde_json::from_slice(b) {
                                Ok(v) => v,
                                Err(e) => {
                                    fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, format!("invalid id JSON: {e}"))));
                                    return;
                                }
                            },
                            None => {
                                fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, "id buffer is null".to_string())));
                                return;
                            }
                        };
                        let record: database::#model_ident = match read_bytes(record, record_len) {
                            Some(b) => match serde_json::from_slice(b) {
                                Ok(r) => r,
                                Err(e) => {
                                    fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, format!("invalid record JSON: {e}"))));
                                    return;
                                }
                            },
                            None => {
                                fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, "record buffer is null".to_string())));
                                return;
                            }
                        };
                        let sdb = SendDb(db);
                        spawn_async(move || {
                            let outcome = (|| -> Result<Option<Vec<u8>>, (i32, String)> {
                                let db = unsafe { sdb.as_mut() };
                                match catch_unwind(AssertUnwindSafe(move || db.inner.#update_fn(id, record))) {
                                    Ok(Ok(updated)) => serde_json::to_vec(&updated)
                                        .map(Some)
                                        .map_err(|e| (FORGEDB_ERR_IO, format!("bool serialize: {e}"))),
                                    Ok(Err(e)) => Err((FORGEDB_ERR_VALIDATION, e.to_string())),
                                    Err(p) => Err((FORGEDB_ERR_PANIC, panic_message(p))),
                                }
                            })();
                            fire_completion(token, outcome);
                        });
                    }

                    #[doc = #delete_doc]
                    ///
                    /// # Safety
                    /// `db` is a live handle; `id`/`id_len` a readable JSON id
                    /// buffer read before this returns.  See the async contract.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #delete_sym(
                        db: *mut Db,
                        id: *const u8,
                        id_len: usize,
                        token: u64,
                    ) {
                        if db.is_null() {
                            fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string())));
                            return;
                        }
                        let id: #id_ty = match read_bytes(id, id_len) {
                            Some(b) => match serde_json::from_slice(b) {
                                Ok(v) => v,
                                Err(e) => {
                                    fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, format!("invalid id JSON: {e}"))));
                                    return;
                                }
                            },
                            None => {
                                fire_completion(token, Err((FORGEDB_ERR_INVALID_ARG, "id buffer is null".to_string())));
                                return;
                            }
                        };
                        let sdb = SendDb(db);
                        spawn_async(move || {
                            let outcome = (|| -> Result<Option<Vec<u8>>, (i32, String)> {
                                let db = unsafe { sdb.as_mut() };
                                match catch_unwind(AssertUnwindSafe(|| db.inner.#delete_fn(id))) {
                                    Ok(Ok(deleted)) => serde_json::to_vec(&deleted)
                                        .map(Some)
                                        .map_err(|e| (FORGEDB_ERR_IO, format!("bool serialize: {e}"))),
                                    Ok(Err(e)) => Err((FORGEDB_ERR_VALIDATION, e.to_string())),
                                    Err(p) => Err((FORGEDB_ERR_PANIC, panic_message(p))),
                                }
                            })();
                            fire_completion(token, outcome);
                        });
                    }
                }
            })
            .collect()
    }

    /// Generate the relation-traversal C-ABI getters.  Each one mirrors a
    /// generated `Database` traversal method **one-for-one** — same name, derived
    /// from the same schema in the same order (so the shared `seen` dedup here
    /// tracks `RustGenerator::generate_traversal_impl` exactly and we never emit a
    /// wrapper for a method that was deduped away).  Three families, all keyed on
    /// an id (never a runtime predicate — a fixed generated edge walk):
    ///   * **Forward FK** `forgedb_<model>_<field>(db, id, id_len, out, out_len, err)`
    ///     — fetch the source record by its id, resolve the `*Target`/`?Target` FK,
    ///     emit `Option<Target>` as JSON (`*out` null only if the source is absent);
    ///   * **Reverse 1:M** `forgedb_<parent>_<field>[_by_<child_field>](db, id, ...)`
    ///     — every child whose FK references the (UUID) parent id, as a JSON array;
    ///   * **M2M** `forgedb_link_<a>_<b>` / `forgedb_unlink_<a>_<b>` (both UUID ids)
    ///     + the query getters `forgedb_<a>_<field1>` / `forgedb_<b>_<field2>`
    ///     (linked records as a JSON array), plus the one snapshot-scoped
    ///     traversal `forgedb_<a>_<field1>_at(db, snap, id, ...)` mirroring
    ///     `Database::<a>_<field1>_at` (junction + target both clamped to `snap`).
    ///
    /// The eager-load bundles (`*_with_relations`) are deferred to a later phase
    /// (they land here too).
    fn generate_relation_ops(schema: &Schema, p: &str) -> Vec<proc_macro2::TokenStream> {
        use std::collections::{HashMap, HashSet};

        let mut ops = Vec::new();
        // ONE `seen` set spanning all three families, inserted in the SAME order
        // as `generate_traversal_impl`, so first-occurrence-wins agrees exactly.
        let mut seen: HashSet<String> = HashSet::new();

        // A `Vec`-returning getter over a decoded id: `forgedb_<name>(db, id, ...)`.
        let vec_getter = |sym: &proc_macro2::Ident,
                          id_ty: &proc_macro2::TokenStream,
                          call: proc_macro2::TokenStream,
                          doc: &str| {
            quote! {
                #[doc = #doc]
                ///
                /// # Safety
                /// `db` is a live handle; `id`/`id_len` a readable JSON id buffer;
                /// `out`/`out_len`/`err_out`, when non-null, writable.  On success
                /// `*out` is a JSON buffer freed with `forgedb_free_buffer`.
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #sym(
                    db: *mut Db,
                    id: *const u8,
                    id_len: usize,
                    out: *mut *mut u8,
                    out_len: *mut usize,
                    err_out: *mut *mut ForgeError,
                ) -> bool {
                    clear_err(err_out);
                    if !out.is_null() { *out = ptr::null_mut(); }
                    if !out_len.is_null() { *out_len = 0; }
                    let Some(db) = db.as_ref() else {
                        set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                        return false;
                    };
                    let Some(id_bytes) = read_bytes(id, id_len) else {
                        set_error(err_out, FORGEDB_ERR_INVALID_ARG, "id buffer is null".to_string());
                        return false;
                    };
                    let id: #id_ty = match serde_json::from_slice(id_bytes) {
                        Ok(v) => v,
                        Err(e) => {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, format!("invalid id JSON: {e}"));
                            return false;
                        }
                    };
                    match catch_unwind(AssertUnwindSafe(|| #call)) {
                        Ok(value) => match serde_json::to_vec(&value) {
                            Ok(json) => { emit_bytes(json, out, out_len); true }
                            Err(e) => {
                                set_error(err_out, FORGEDB_ERR_IO, format!("serialize: {e}"));
                                false
                            }
                        },
                        Err(payload) => {
                            set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                            false
                        }
                    }
                }
            }
        };

        // The snapshot-scoped sibling of `vec_getter`: a `Vec`-returning getter
        // over a decoded `#id_ty` id (#266 — the endpoint's own key) AND a
        // `*const Snapshot` handle:
        // `forgedb_<name>(db, snap, id, ...)`.  `call` uses the borrowed `snap`.
        let snap_vec_getter = |sym: &proc_macro2::Ident,
                               id_ty: &proc_macro2::TokenStream,
                               call: proc_macro2::TokenStream,
                               doc: &str| {
            quote! {
                #[doc = #doc]
                ///
                /// # Safety
                /// `db`/`snap` are live handles; `id`/`id_len` a readable JSON
                /// UUID buffer; `out`/`out_len`/`err_out`, when non-null, writable.
                /// On success `*out` is a JSON buffer freed with
                /// `forgedb_free_buffer`.
                #[unsafe(no_mangle)]
                pub unsafe extern "C" fn #sym(
                    db: *mut Db,
                    snap: *const Snapshot,
                    id: *const u8,
                    id_len: usize,
                    out: *mut *mut u8,
                    out_len: *mut usize,
                    err_out: *mut *mut ForgeError,
                ) -> bool {
                    clear_err(err_out);
                    if !out.is_null() { *out = ptr::null_mut(); }
                    if !out_len.is_null() { *out_len = 0; }
                    let Some(db) = db.as_ref() else {
                        set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                        return false;
                    };
                    let Some(snap) = snap.as_ref() else {
                        set_error(err_out, FORGEDB_ERR_INVALID_ARG, "snapshot handle is null".to_string());
                        return false;
                    };
                    let Some(id_bytes) = read_bytes(id, id_len) else {
                        set_error(err_out, FORGEDB_ERR_INVALID_ARG, "id buffer is null".to_string());
                        return false;
                    };
                    let id: #id_ty = match serde_json::from_slice(id_bytes) {
                        Ok(v) => v,
                        Err(e) => {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, format!("invalid id JSON: {e}"));
                            return false;
                        }
                    };
                    match catch_unwind(AssertUnwindSafe(|| #call)) {
                        Ok(value) => match serde_json::to_vec(&value) {
                            Ok(json) => { emit_bytes(json, out, out_len); true }
                            Err(e) => {
                                set_error(err_out, FORGEDB_ERR_IO, format!("serialize: {e}"));
                                false
                            }
                        },
                        Err(payload) => {
                            set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                            false
                        }
                    }
                }
            }
        };

        // --- A. Forward FK getters (`*Target` / `?Target`) --------------------
        for model in &schema.models {
            let model_snake = RustGenerator::to_snake_case(&model.name);
            let model_has_id = model.has_identity();
            let id_ty = RustGenerator::id_type_tokens(schema, model);
            let storage = format_ident!("{}", model_snake);
            for field in &model.fields {
                let target_name = match &field.field_type {
                    forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::RequiredReference(t),
                    )
                    | forgedb_parser::FieldType::Relation(
                        forgedb_parser::RelationType::OptionalReference(t),
                    ) => t,
                    _ => continue,
                };
                if schema.find_model(target_name).is_none() { continue; }
                let method_name = format!("{model_snake}_{}", field.name);
                if !seen.insert(method_name.clone()) {
                    continue;
                }
                // The FFI wrapper must fetch the source record by id, so it needs
                // the source model to be id-addressable.  A source without an
                // identity field still HAS the (record-by-ref) getter on Database,
                // but no C-ABI id path — skip its wrapper (seen already consumed,
                // matching the impl).
                if !model_has_id {
                    continue;
                }
                let method_ident = format_ident!("{}", method_name);
                let sym = format_ident!("{}{}", p, method_name);
                let doc = format!(
                    "Resolve the `{}` foreign key of a `{}` (by id) to its record (JSON `Option`).",
                    field.name, model.name
                );
                ops.push(quote! {
                    #[doc = #doc]
                    ///
                    /// # Safety
                    /// `db` is a live handle; `id`/`id_len` a readable JSON id
                    /// buffer; `out`/`out_len`/`err_out`, when non-null, writable.
                    /// `*out` is null iff the source record is absent; otherwise a
                    /// JSON buffer (`null` or the target) freed with
                    /// `forgedb_free_buffer`.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #sym(
                        db: *mut Db,
                        id: *const u8,
                        id_len: usize,
                        out: *mut *mut u8,
                        out_len: *mut usize,
                        err_out: *mut *mut ForgeError,
                    ) -> bool {
                        clear_err(err_out);
                        if !out.is_null() { *out = ptr::null_mut(); }
                        if !out_len.is_null() { *out_len = 0; }
                        let Some(db) = db.as_ref() else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                            return false;
                        };
                        let Some(id_bytes) = read_bytes(id, id_len) else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "id buffer is null".to_string());
                            return false;
                        };
                        let id: #id_ty = match serde_json::from_slice(id_bytes) {
                            Ok(v) => v,
                            Err(e) => {
                                set_error(err_out, FORGEDB_ERR_INVALID_ARG, format!("invalid id JSON: {e}"));
                                return false;
                            }
                        };
                        // Outer `Option` = source record present?  Inner (the
                        // getter's `Option<Target>`) = FK resolved?
                        match catch_unwind(AssertUnwindSafe(|| {
                            db.inner.#storage.get(id).map(|__rec| db.inner.#method_ident(&__rec))
                        })) {
                            Ok(Some(resolved)) => match serde_json::to_vec(&resolved) {
                                Ok(json) => { emit_bytes(json, out, out_len); true }
                                Err(e) => {
                                    set_error(err_out, FORGEDB_ERR_IO, format!("serialize: {e}"));
                                    false
                                }
                            },
                            // Source record absent: success with `*out` left null.
                            Ok(None) => true,
                            Err(payload) => {
                                set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                                false
                            }
                        }
                    }
                });
            }
        }

        // --- B. Reverse one-to-many collection getters ------------------------
        let pairs = schema.detect_relations();
        let mut group_counts: HashMap<(String, String), usize> = HashMap::new();
        for pair in &pairs {
            *group_counts
                .entry((pair.parent_model.clone(), pair.parent_field.clone()))
                .or_default() += 1;
        }
        for pair in &pairs {
            let Some(parent) = schema.find_model(&pair.parent_model) else { continue };
            let ambiguous = group_counts
                .get(&(pair.parent_model.clone(), pair.parent_field.clone()))
                .is_some_and(|&c| c > 1);
            let method_name = if ambiguous {
                format!(
                    "{}_{}_by_{}",
                    RustGenerator::to_snake_case(&pair.parent_model),
                    pair.parent_field,
                    pair.child_field
                )
            } else {
                format!("{}_{}", RustGenerator::to_snake_case(&pair.parent_model), pair.parent_field)
            };
            if !seen.insert(method_name.clone()) {
                continue;
            }
            let method_ident = format_ident!("{}", method_name);
            let sym = format_ident!("{}{}", p, method_name);
            // #266: the C-ABI id buffer decodes as the PARENT's own key type.
            let id_ty = RustGenerator::id_type_tokens(schema, parent);
            let doc = format!(
                "All `{}` whose `{}` references the given `{}` id (JSON array).",
                pair.child_model, pair.child_field, pair.parent_model
            );
            ops.push(vec_getter(&sym, &id_ty, quote! { db.inner.#method_ident(id) }, &doc));
        }

        // --- C. Many-to-many link / unlink + query getters --------------------
        for m in RustGenerator::valid_m2m(schema) {
            let snake1 = RustGenerator::to_snake_case(&m.model1);
            let snake2 = RustGenerator::to_snake_case(&m.model2);
            // #266: each junction endpoint decodes as its OWN identity type.
            let (lk, rk) = RustGenerator::junction_key_idents(schema, &m);

            // link_<a>_<b>
            let link_name = format!("link_{snake1}_{snake2}");
            if seen.insert(link_name.clone()) {
                let link_ident = format_ident!("{}", link_name);
                let sym = format_ident!("{}{}", p, link_name);
                let doc = format!("Link a `{}` (left) and a `{}` (right) in the junction.", m.model1, m.model2);
                ops.push(quote! {
                    #[doc = #doc]
                    ///
                    /// # Safety
                    /// `db` is a live handle; `left`/`right` (+ their lens) readable
                    /// JSON UUID buffers; `err_out`, when non-null, writable.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #sym(
                        db: *mut Db,
                        left: *const u8,
                        left_len: usize,
                        right: *const u8,
                        right_len: usize,
                        err_out: *mut *mut ForgeError,
                    ) -> bool {
                        clear_err(err_out);
                        let Some(db) = db.as_mut() else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                            return false;
                        };
                        let (Some(lb), Some(rb)) = (read_bytes(left, left_len), read_bytes(right, right_len)) else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "left/right buffer is null".to_string());
                            return false;
                        };
                        let (Ok(left), Ok(right)) = (
                            serde_json::from_slice::<#lk>(lb),
                            serde_json::from_slice::<#rk>(rb),
                        ) else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "invalid endpoint id JSON".to_string());
                            return false;
                        };
                        match catch_unwind(AssertUnwindSafe(|| db.inner.#link_ident(left, right))) {
                            Ok(()) => true,
                            Err(payload) => {
                                set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                                false
                            }
                        }
                    }
                });
            }

            // unlink_<a>_<b>
            let unlink_name = format!("unlink_{snake1}_{snake2}");
            if seen.insert(unlink_name.clone()) {
                let unlink_ident = format_ident!("{}", unlink_name);
                let sym = format_ident!("{}{}", p, unlink_name);
                let doc = format!("Unlink a `{}` (left) / `{}` (right): 1 removed / 0 no-op / -1 error.", m.model1, m.model2);
                ops.push(quote! {
                    #[doc = #doc]
                    ///
                    /// # Safety
                    /// `db` is a live handle; `left`/`right` (+ their lens) readable
                    /// JSON UUID buffers; `err_out`, when non-null, writable.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #sym(
                        db: *mut Db,
                        left: *const u8,
                        left_len: usize,
                        right: *const u8,
                        right_len: usize,
                        err_out: *mut *mut ForgeError,
                    ) -> i32 {
                        clear_err(err_out);
                        let Some(db) = db.as_mut() else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                            return -1;
                        };
                        let (Some(lb), Some(rb)) = (read_bytes(left, left_len), read_bytes(right, right_len)) else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "left/right buffer is null".to_string());
                            return -1;
                        };
                        let (Ok(left), Ok(right)) = (
                            serde_json::from_slice::<#lk>(lb),
                            serde_json::from_slice::<#rk>(rb),
                        ) else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "invalid endpoint id JSON".to_string());
                            return -1;
                        };
                        match catch_unwind(AssertUnwindSafe(|| db.inner.#unlink_ident(left, right))) {
                            Ok(true) => 1,
                            Ok(false) => 0,
                            Err(payload) => {
                                set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                                -1
                            }
                        }
                    }
                });
            }

            // model1.field1 -> Vec<model2>  (forward M2M query)
            let fwd_name = format!("{snake1}_{}", m.field1);
            if seen.insert(fwd_name.clone()) {
                let fwd_ident = format_ident!("{}", fwd_name);
                let sym = format_ident!("{}{}", p, fwd_name);
                let id_ty = lk.clone();
                let doc = format!("All linked `{}` for the given `{}` id (JSON array).", m.model2, m.model1);
                ops.push(vec_getter(&sym, &id_ty, quote! { db.inner.#fwd_ident(id) }, &doc));

                // The ONE snapshot-scoped traversal on `Database` (#56): the
                // junction and each resolved target are both clamped to the same
                // captured watermark, so a link or target row appended after the
                // snapshot is excluded on both sides of the join.  Emitted here,
                // at the exact point the impl inserts `<a>_<field1>_at` into the
                // shared `seen`, so this wrapper and the impl method agree.
                let fwd_at_name = format!("{snake1}_{}_at", m.field1);
                if seen.insert(fwd_at_name.clone()) {
                    let fwd_at_ident = format_ident!("{}", fwd_at_name);
                    let at_sym = format_ident!("{}{}", p, fwd_at_name);
                    let at_doc = format!(
                        "All linked `{}` for the given `{}` id, consistent as of `snap` (JSON array).",
                        m.model2, m.model1
                    );
                    ops.push(snap_vec_getter(
                        &at_sym,
                        &lk,
                        quote! { db.inner.#fwd_at_ident(&snap.inner, id) },
                        &at_doc,
                    ));
                }
            }

            // model2.field2 -> Vec<model1>  (reverse M2M query)
            let rev_name = format!("{snake2}_{}", m.field2);
            if seen.insert(rev_name.clone()) {
                let rev_ident = format_ident!("{}", rev_name);
                let sym = format_ident!("{}{}", p, rev_name);
                let id_ty = rk.clone();
                let doc = format!("All linked `{}` for the given `{}` id (JSON array).", m.model1, m.model2);
                ops.push(vec_getter(&sym, &id_ty, quote! { db.inner.#rev_ident(id) }, &doc));
            }
        }

        ops
    }

    /// Generate the Arrow columnar-export C-ABI functions — the zero-copy
    /// selling point (language bindings #51/#52). For each identity model and
    /// each Arrow-exportable non-null fixed-width column `f`
    /// (`RustGenerator::arrow_export_format` is the shared source of truth for
    /// the set + the Arrow format string), one entry point
    /// `forgedb_<m>_<f>_export_arrow(db, out_schema, out_array, err_out) -> bool`
    /// that: computes the live physical row indices in generated code
    /// (`export_live_indices`), exports exactly those rows of the one column
    /// (`export_col_<f>` → the class-1 `ColumnExport` — a zero-copy `mmap` alias
    /// when the live rows are a dense prefix, else a gathered copy), and fills the
    /// caller's Arrow `ArrowSchema`/`ArrowArray` (buffer freed / `munmap`ed by the
    /// Arrow `release` callback).
    ///
    /// Identity clean: the exported set + formats are fixed by the schema at
    /// codegen time (never a runtime column list), the export takes opaque row
    /// indices, and there is no `forgedb_query` / predicate / `match model`.
    /// The export is a **zero-copy `mmap` alias** of the on-disk column when the
    /// live rows are a dense prefix and a gathered copy otherwise — same ABI +
    /// release contract either way (`fill_arrow_primitive` / `ColumnExport`).
    fn generate_arrow_ops(schema: &Schema, p: &str) -> Vec<proc_macro2::TokenStream> {
        let mut ops = Vec::new();
        for model in schema
            .models
            .iter()
            .filter(|m| m.has_identity())
        {
            let snake = RustGenerator::to_snake_case(&model.name);
            let storage = format_ident!("{}", snake);
            for field in &model.fields {
                let Some(fmt) = RustGenerator::arrow_export_format(schema, &field.field_type) else {
                    continue;
                };
                let sym = format_ident!("{}{}_{}_export_arrow", p, snake, field.name);
                let export_method = format_ident!("export_col_{}", field.name);
                // A nul-terminated Arrow format C-string (`b"l\0"` → `*const c_char`).
                let fmt_bytes = proc_macro2::Literal::byte_string(format!("{fmt}\0").as_bytes());
                let doc = format!(
                    "Export the live `{}.{}` column as an Arrow C-Data-Interface array (`{}`).",
                    model.name, field.name, fmt
                );
                ops.push(quote! {
                    #[doc = #doc]
                    ///
                    /// Fills `out_schema`/`out_array` (the Arrow C Data Interface
                    /// pair) with exactly the live rows of this column — a zero-copy
                    /// `mmap` alias of the column's dense prefix when possible, a
                    /// gathered copy otherwise.  The caller owns the result and MUST
                    /// release it via the Arrow `release` callback on the array
                    /// (which frees the copy or `munmap`s the alias).  Returns
                    /// `false` with `*err_out` set on a null argument, an I/O error,
                    /// or a caught panic.
                    ///
                    /// # Safety
                    /// `db` is a live handle; `out_schema`/`out_array` point to
                    /// writable `ArrowSchema`/`ArrowArray` storage; `err_out`, when
                    /// non-null, is writable.
                    #[unsafe(no_mangle)]
                    pub unsafe extern "C" fn #sym(
                        db: *mut Db,
                        out_schema: *mut ArrowSchema,
                        out_array: *mut ArrowArray,
                        err_out: *mut *mut ForgeError,
                    ) -> bool {
                        clear_err(err_out);
                        if out_schema.is_null() || out_array.is_null() {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "output pointer is null".to_string());
                            return false;
                        }
                        let Some(db) = db.as_ref() else {
                            set_error(err_out, FORGEDB_ERR_INVALID_ARG, "db handle is null".to_string());
                            return false;
                        };
                        let outcome = catch_unwind(AssertUnwindSafe(|| {
                            let live = db.inner.#storage.export_live_indices();
                            let n = live.len();
                            db.inner.#storage.#export_method(&live).map(|bytes| (bytes, n))
                        }));
                        match outcome {
                            Ok(Ok((data, n))) => {
                                fill_arrow_primitive(
                                    out_schema,
                                    out_array,
                                    #fmt_bytes.as_ptr() as *const c_char,
                                    data,
                                    n,
                                );
                                true
                            }
                            Ok(Err(e)) => {
                                set_error(err_out, FORGEDB_ERR_IO, format!("column export failed: {e}"));
                                false
                            }
                            Err(payload) => {
                                set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                                false
                            }
                        }
                    }
                });
            }
        }
        ops
    }

    /// Render `ffi/Cargo.toml` for the cache package.
    ///
    /// `core_package` is the app's `core` package name; the dependency is
    /// **renamed** to `forgedb_core` so the generated source never carries the
    /// app hash.
    ///
    /// # Three crate types, and why `staticlib` is the load-bearing one
    ///
    /// * `cdylib` — the C-ABI shared object a Python/Node/Bun binding loads;
    /// * `rlib` — so this crate is usable as a plain Rust dependency. Its old
    ///   rationale ("so a NAPI-RS / PyO3 wrapper crate can link the engine
    ///   directly") was already false and is deleted: those wrappers link the
    ///   generated database, and as of #335 they link `core` for it, not this
    ///   crate;
    /// * `staticlib` — the `libforgedb.a` the Go binding links (#335 §6). Go
    ///   delivery is static, which is also what makes the per-app C-symbol
    ///   prefix mandatory rather than tidy: a duplicate `no_mangle` symbol is a
    ///   load-time problem for a `cdylib` only if one process loads both, but a
    ///   hard **link-time** collision for a single Go binary importing two
    ///   ForgeDB packages.
    ///
    /// # Zero substrate pins
    ///
    /// Every substrate type this crate names reaches it through `core`'s
    /// re-exports (`forgedb_core::forgedb_storage`, `::forgedb_types`), which is
    /// what makes those types **unify** with `core`'s rather than merely resolve
    /// to the same version by lockfile coincidence. A pin here would be a second
    /// place a version can drift.
    ///
    /// # No `[profile.*]` table
    ///
    /// The deleted `[profile.release] panic = "unwind"` was not tidy-up. This
    /// crate is now a workspace **member**, and a profile in a member is
    /// *silently ignored* (`warning: profiles for the non root package will be
    /// ignored`) — so a block whose comment called it load-bearing for the
    /// `catch_unwind` boundary read as applied while doing nothing. The unwind
    /// floor is applied by the build driver on the cargo invocation, where a
    /// hostile `$CARGO_HOME/config.toml` cannot beat it.
    pub fn cargo_toml(crate_name: &str, core_package: &str) -> String {
        format!(
            r#"# Generated by ForgeDB. Do not edit — rewritten in full on every generate.
[package]
name = "{crate_name}"
version = "0.1.0"
edition = "2024"

[lib]
# cdylib:    the C-ABI shared object a Python/Node/Bun binding loads.
# rlib:      this crate as a plain Rust dependency.
# staticlib: the archive the Go binding links (#335 §6) — the reason every
#            exported symbol carries this app's prefix, since a duplicate is a
#            link-time error rather than a load-time one.
crate-type = ["cdylib", "rlib", "staticlib"]

[dependencies]
# The one generated database for this app, and the ONLY dependency. Every
# substrate type this crate names is reached through `core`'s re-exports, so
# their types UNIFY with `core`'s instead of merely resolving to the same
# version by lockfile coincidence. This crate pins ZERO substrate of its own.
forgedb_core = {{ package = "{core_package}", path = "../core" }}

# NOT substrate, and therefore not routed through `core`: the C-ABI marshals
# every payload as JSON bytes, so this crate names `serde_json` in its own body
# (140 `E0433`s without it — caught by compiling the emitted cache workspace,
# never by a snapshot). "Zero substrate pins" is a statement about ForgeDB
# crates; a third-party dep the wrapper itself calls still belongs here.
serde_json = "1"
"#
        )
    }
}
