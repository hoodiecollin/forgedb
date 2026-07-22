// ForgeDB Language Server — reusable library surface.
//
// The binary (`main.rs`) is a thin tower-lsp wrapper. The pieces that carry real
// behavior — the compiler-diagnostic → LSP mapper and the grammar-driven
// completion/hover helpers — live here so they can be exercised outside the LSP
// event loop. In particular, the CLI↔LSP diagnostic-parity fixture (epic #173
// WS3, `tests/lsp_cli_parity.rs` in the root crate) imports `to_lsp_diagnostics`
// and asserts it stays in lockstep with `forgedb validate` over `examples/*`.
//
// There is no private grammar here: everything is driven by `forgedb_parser`.

pub mod completion;
pub mod diagnostics;
pub mod hover;

pub use diagnostics::to_lsp_diagnostics;
