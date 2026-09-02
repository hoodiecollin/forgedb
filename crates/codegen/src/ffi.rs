use crate::RustGenerator;
use crate::{GeneratedCode, Result};
use forgedb_parser::Schema;
use quote::{format_ident, quote};

fn fingerprint_block(pfx: &str, fingerprint: &str) -> String {
    format!(
        r#"
#define FORGEDB_FINGERPRINT "{fingerprint}"

const char* {pfx}fingerprint(void);

static inline int forgedb_fingerprint_ok(void) {{
  const char* built = {pfx}fingerprint();
  const char* want = FORGEDB_FINGERPRINT;
  while (*built && *want && *built == *want) {{ built++; want++; }}
  return *built == *want;
}}
"#
    )
}

pub struct FfiGenerator;

impl FfiGenerator {
    pub fn generate(schema: &Schema, symbol_prefix: &str) -> Result<GeneratedCode> {
        let p = symbol_prefix;
        let model_ops = Self::generate_model_ops(schema, p);
        let async_ops = Self::generate_async_ops(schema, p);
        let relation_ops = Self::generate_relation_ops(schema, p);
        let arrow_ops = Self::generate_arrow_ops(schema, p);

        let sym_version = format_ident!("{}version", p);
        let sym_fingerprint = format_ident!("{}fingerprint", p);
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
            #![allow(warnings)]

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
            use forgedb_core::forgedb_types::Uuid;

            pub struct Db {
                inner: Database,
            }

            pub struct ForgeError {
                code: i32,
                message: CString,
            }

            pub struct Snapshot {
                inner: database::DatabaseSnapshot,
            }

            const FORGEDB_ERR_INVALID_ARG: i32 = 1;
            const FORGEDB_ERR_IO: i32 = 2;
            const FORGEDB_ERR_PANIC: i32 = 3;
            const FORGEDB_ERR_VALIDATION: i32 = 4;

            unsafe fn set_error(err_out: *mut *mut ForgeError, code: i32, message: String) {
                if err_out.is_null() {
                    return;
                }
                let message = CString::new(message)
                    .unwrap_or_else(|_| CString::new("error").unwrap());
                *err_out = Box::into_raw(Box::new(ForgeError { code, message }));
            }

            fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
                if let Some(s) = payload.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                }
            }

            unsafe fn clear_err(err_out: *mut *mut ForgeError) {
                if !err_out.is_null() {
                    *err_out = ptr::null_mut();
                }
            }

            unsafe fn read_bytes<'a>(ptr: *const u8, len: usize) -> Option<&'a [u8]> {
                if ptr.is_null() {
                    None
                } else {
                    Some(std::slice::from_raw_parts(ptr, len))
                }
            }

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

            mod fingerprint;

            #[unsafe(no_mangle)]
            pub extern "C" fn #sym_fingerprint() -> *const c_char {
                static FINGERPRINT: OnceLock<CString> = OnceLock::new();
                FINGERPRINT
                    .get_or_init(|| {
                        CString::new(fingerprint::FINGERPRINT)
                            .unwrap_or_else(|_| CString::new("").unwrap())
                    })
                    .as_ptr()
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn #sym_version() -> *const c_char {
                static VERSION: OnceLock<CString> = OnceLock::new();
                VERSION
                    .get_or_init(|| {
                        CString::new(env!("CARGO_PKG_VERSION"))
                            .unwrap_or_else(|_| CString::default())
                    })
                    .as_ptr()
            }

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
                match catch_unwind(AssertUnwindSafe(|| Database::open_at(root))) {
                    Ok(db) => Box::into_raw(Box::new(Db { inner: db })),
                    Err(payload) => {
                        set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                        ptr::null_mut()
                    }
                }
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_close(db: *mut Db) {
                if db.is_null() {
                    return;
                }
                drop(Box::from_raw(db));
            }

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

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_error_code(err: *const ForgeError) -> i32 {
                match err.as_ref() {
                    Some(e) => e.code,
                    None => 0,
                }
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_error_message(err: *const ForgeError) -> *const c_char {
                match err.as_ref() {
                    Some(e) => e.message.as_ptr(),
                    None => ptr::null(),
                }
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_error_free(err: *mut ForgeError) {
                if err.is_null() {
                    return;
                }
                drop(Box::from_raw(err));
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_free_buffer(ptr: *mut u8, len: usize) {
                if ptr.is_null() {
                    return;
                }
                drop(Vec::from_raw_parts(ptr, len, len));
            }

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

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_snapshot_free(snap: *mut Snapshot) {
                if snap.is_null() {
                    return;
                }
                drop(Box::from_raw(snap));
            }

            pub type ForgeCompletion =
                extern "C" fn(token: u64, status: i32, payload: *mut u8, payload_len: usize);

            static COMPLETION_CB: AtomicUsize = AtomicUsize::new(0);

            struct SendDb(*mut Db);
            unsafe impl Send for SendDb {}

            impl SendDb {
                #[allow(clippy::mut_from_ref)]
                unsafe fn as_mut<'a>(&self) -> &'a mut Db {
                    unsafe { &mut *self.0 }
                }
            }

            const _: fn() = || {
                fn assert_send<T: Send>() {}
                assert_send::<Db>();
            };

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn #sym_set_completion_callback(cb: Option<ForgeCompletion>) {
                let addr = match cb {
                    Some(f) => f as usize,
                    None => 0,
                };
                COMPLETION_CB.store(addr, Ordering::SeqCst);
            }

            fn load_completion_cb() -> Option<ForgeCompletion> {
                let addr = COMPLETION_CB.load(Ordering::SeqCst);
                if addr == 0 {
                    None
                } else {
                    Some(unsafe { std::mem::transmute::<usize, ForgeCompletion>(addr) })
                }
            }

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

            fn spawn_async<F: FnOnce() + Send + 'static>(f: F) {
                if let Ok(tx) = async_executor().lock() {
                    let _ = tx.send(Box::new(f));
                }
            }

            #(#async_ops)*

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

            struct ArrowArrayOwner {
                _export: forgedb_core::forgedb_storage::ColumnExport,
                _buffers: Vec<*const c_void>,
            }

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

            unsafe extern "C" fn arrow_schema_release(schema: *mut ArrowSchema) {
                if schema.is_null() {
                    return;
                }
                let s = &mut *schema;
                s.release = None;
            }

            unsafe fn fill_arrow_primitive(
                out_schema: *mut ArrowSchema,
                out_array: *mut ArrowArray,
                format: *const c_char,
                export: forgedb_core::forgedb_storage::ColumnExport,
                length: usize,
            ) {
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

            #(#model_ops)*

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
                let snap_field = format_ident!("{}", snake);


                quote! {
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
                            Ok(None) => true,
                            Err(payload) => {
                                set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                                false
                            }
                        }
                    }

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
                            Ok(None) => true,
                            Err(payload) => {
                                set_error(err_out, FORGEDB_ERR_PANIC, panic_message(payload));
                                false
                            }
                        }
                    }

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


                quote! {
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

    fn generate_relation_ops(schema: &Schema, p: &str) -> Vec<proc_macro2::TokenStream> {
        use std::collections::{HashMap, HashSet};

        let mut ops = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();

        let vec_getter = |sym: &proc_macro2::Ident,
                          id_ty: &proc_macro2::TokenStream,
                          call: proc_macro2::TokenStream,
                          | {
            quote! {
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

        let snap_vec_getter = |sym: &proc_macro2::Ident,
                               id_ty: &proc_macro2::TokenStream,
                               call: proc_macro2::TokenStream,
                               | {
            quote! {
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
                if !model_has_id {
                    continue;
                }
                let method_ident = format_ident!("{}", method_name);
                let sym = format_ident!("{}{}", p, method_name);
                ops.push(quote! {
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
            let id_ty = RustGenerator::id_type_tokens(schema, parent);
            ops.push(vec_getter(&sym, &id_ty, quote! { db.inner.#method_ident(id) }));
        }

        for m in RustGenerator::valid_m2m(schema) {
            let snake1 = RustGenerator::to_snake_case(&m.model1);
            let snake2 = RustGenerator::to_snake_case(&m.model2);
            let (lk, rk) = RustGenerator::junction_key_idents(schema, &m);

            let link_name = format!("link_{snake1}_{snake2}");
            if seen.insert(link_name.clone()) {
                let link_ident = format_ident!("{}", link_name);
                let sym = format_ident!("{}{}", p, link_name);
                ops.push(quote! {
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

            let unlink_name = format!("unlink_{snake1}_{snake2}");
            if seen.insert(unlink_name.clone()) {
                let unlink_ident = format_ident!("{}", unlink_name);
                let sym = format_ident!("{}{}", p, unlink_name);
                ops.push(quote! {
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

            let fwd_name = format!("{snake1}_{}", m.field1);
            if seen.insert(fwd_name.clone()) {
                let fwd_ident = format_ident!("{}", fwd_name);
                let sym = format_ident!("{}{}", p, fwd_name);
                let id_ty = lk.clone();
                ops.push(vec_getter(&sym, &id_ty, quote! { db.inner.#fwd_ident(id) }));

                let fwd_at_name = format!("{snake1}_{}_at", m.field1);
                if seen.insert(fwd_at_name.clone()) {
                    let fwd_at_ident = format_ident!("{}", fwd_at_name);
                    let at_sym = format_ident!("{}{}", p, fwd_at_name);
                    ops.push(snap_vec_getter(
                        &at_sym,
                        &lk,
                        quote! { db.inner.#fwd_at_ident(&snap.inner, id) },
                    ));
                }
            }

            let rev_name = format!("{snake2}_{}", m.field2);
            if seen.insert(rev_name.clone()) {
                let rev_ident = format_ident!("{}", rev_name);
                let sym = format_ident!("{}{}", p, rev_name);
                let id_ty = rk.clone();
                ops.push(vec_getter(&sym, &id_ty, quote! { db.inner.#rev_ident(id) }));
            }
        }

        ops
    }

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
                let fmt_bytes = proc_macro2::Literal::byte_string(format!("{fmt}\0").as_bytes());
                ops.push(quote! {
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

    pub fn generate_header(
        schema: &Schema,
        symbol_prefix: &str,
        fingerprint: &str,
    ) -> Result<GeneratedCode> {
        use crate::go::{GoGenerator, GoRelOp, HEADER_PREAMBLE, subst};
        let pfx = symbol_prefix;
        let models = GoGenerator::crud_models(schema);
        let rel_ops = GoGenerator::relation_ops(schema);

        let mut h = String::new();
        h.push_str(&subst(HEADER_PREAMBLE, pfx));
        h.push_str(&fingerprint_block(pfx, fingerprint));

        for m in &models {
            let s = &m.snake;
            h.push_str(&format!(
                "\n\
                 bool {pfx}{s}_insert(Db* db, const uint8_t* record, size_t record_len, uint8_t** id_out, size_t* id_len_out, ForgeError** err_out);\n\
                 bool {pfx}{s}_get(Db* db, const uint8_t* id, size_t id_len, uint8_t** out, size_t* out_len, ForgeError** err_out);\n\
                 int64_t {pfx}{s}_count(Db* db, ForgeError** err_out);\n\
                 bool {pfx}{s}_all(Db* db, uint8_t** out, size_t* out_len, ForgeError** err_out);\n\
                 int32_t {pfx}{s}_update(Db* db, const uint8_t* id, size_t id_len, const uint8_t* record, size_t record_len, ForgeError** err_out);\n\
                 int32_t {pfx}{s}_delete(Db* db, const uint8_t* id, size_t id_len, ForgeError** err_out);\n\
                 bool {pfx}{s}_get_at(Db* db, const Snapshot* snap, const uint8_t* id, size_t id_len, uint8_t** out, size_t* out_len, ForgeError** err_out);\n\
                 bool {pfx}{s}_all_at(Db* db, const Snapshot* snap, uint8_t** out, size_t* out_len, ForgeError** err_out);\n\
                 void {pfx}{s}_insert_async(Db* db, const uint8_t* record, size_t record_len, uint64_t token);\n\
                 void {pfx}{s}_get_async(Db* db, const uint8_t* id, size_t id_len, uint64_t token);\n\
                 void {pfx}{s}_count_async(Db* db, uint64_t token);\n\
                 void {pfx}{s}_all_async(Db* db, uint64_t token);\n\
                 void {pfx}{s}_update_async(Db* db, const uint8_t* id, size_t id_len, const uint8_t* record, size_t record_len, uint64_t token);\n\
                 void {pfx}{s}_delete_async(Db* db, const uint8_t* id, size_t id_len, uint64_t token);\n",
                s = s,
            ));
        }

        if !rel_ops.is_empty() {
            for op in &rel_ops {
                h.push_str(&match op {
                    GoRelOp::ForwardFk { sym, .. } | GoRelOp::Vec { sym, .. } => format!(
                        "bool {pfx}{sym}(Db* db, const uint8_t* id, size_t id_len, uint8_t** out, size_t* out_len, ForgeError** err_out);\n"
                    ),
                    GoRelOp::VecAt { sym, .. } => format!(
                        "bool {pfx}{sym}(Db* db, const Snapshot* snap, const uint8_t* id, size_t id_len, uint8_t** out, size_t* out_len, ForgeError** err_out);\n"
                    ),
                    GoRelOp::Link { sym, .. } => format!(
                        "bool {pfx}{sym}(Db* db, const uint8_t* left, size_t left_len, const uint8_t* right, size_t right_len, ForgeError** err_out);\n"
                    ),
                    GoRelOp::Unlink { sym, .. } => format!(
                        "int32_t {pfx}{sym}(Db* db, const uint8_t* left, size_t left_len, const uint8_t* right, size_t right_len, ForgeError** err_out);\n"
                    ),
                });
            }
        }

        let arrow = GoGenerator::arrow_columns(schema);
        if !arrow.is_empty() {
            for c in &arrow {
                h.push_str(&format!(
                    "bool {}{}(Db* db, struct ArrowSchema* out_schema, struct ArrowArray* out_array, ForgeError** err_out);\n",
                    pfx, c.sym
                ));
            }
        }

        h.push_str("\n#endif /* FORGEDB_H */\n");
        Ok(GeneratedCode {
            description: format!("C header ({} models)", models.len()),
            code: h,
        })
    }

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
