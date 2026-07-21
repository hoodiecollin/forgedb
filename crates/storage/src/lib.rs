//! `forgedb-storage` — the columnar storage **facade**.
//!
//! This crate is a thin, target-selecting re-export. It owns no engine code; it
//! picks one at compile time and re-exports its entire surface:
//!
//! - **host targets** (`cfg(not(target_arch = "wasm32"))`) → [`forgedb-storage-native`],
//!   the positional file-I/O columnar engine (`pread`-style reads, advisory
//!   [`DirLock`], WAL re-exports). This is the historical `forgedb-storage`
//!   engine, moved out verbatim — the native public surface is unchanged.
//! - **`wasm32`** (the browser read-replica target, #110) → [`forgedb-storage-web`],
//!   an in-memory-arena backend with byte-identical positional semantics whose
//!   only async boundaries are `hydrate()` (load column blobs from IndexedDB /
//!   OPFS on open) and `commit()` (flush dirty arenas back). The per-row column
//!   API stays synchronous, so the generated data logic compiles unchanged.
//!
//! ## Why a facade and not a trait
//!
//! Generated code writes `use forgedb_storage::{FixedColumn, VariableColumn,
//! Tombstones};` and calls `FixedColumn::new(PathBuf::from(col_path), size)`
//! directly. A `StorageBackend` trait would risk *async-coloring* that per-row
//! API (`get()` becoming `async` everywhere) — the native path would pay for the
//! browser path and pressure would build toward runtime schema interpretation
//! (against the generator-identity red lines). The cfg facade keeps the
//! generated surface **byte-identical across targets** with zero codegen
//! branches: exactly one backend is linked per build, so their identically-named
//! public types never collide.
//!
//! [`forgedb-storage-native`]: forgedb_storage_native
//! [`forgedb-storage-web`]: forgedb_storage_web
//! [`DirLock`]: crate::DirLock

#[cfg(not(target_arch = "wasm32"))]
pub use forgedb_storage_native::*;

#[cfg(target_arch = "wasm32")]
pub use forgedb_storage_web::*;
