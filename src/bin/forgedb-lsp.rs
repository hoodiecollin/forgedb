// The ForgeDB language server binary.
//
// Compiled only when the root crate's non-default `lsp` feature is enabled
// (`required-features = ["lsp"]`), so a lean `cargo install forgedb` never pulls
// in the async LSP stack (`tokio` / `tower-lsp`). Release archives build with
// `--features lsp`, shipping this binary next to `forgedb`; `forgedb lsp`
// resolves and hands off to it (see `src/commands/lsp.rs`).
//
// Stays a plain synchronous `fn main` — `forgedb_lsp_server::run()` owns the
// Tokio runtime internally.
fn main() {
    forgedb_lsp_server::run();
}
