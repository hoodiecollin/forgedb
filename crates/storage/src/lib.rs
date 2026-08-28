#[cfg(not(target_arch = "wasm32"))]
pub use forgedb_storage_native::*;

#[cfg(target_arch = "wasm32")]
pub use forgedb_storage_web::*;
