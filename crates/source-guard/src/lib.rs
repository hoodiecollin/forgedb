//! Assert properties of Rust source through its **syntax tree**, so that a guard cannot
//! pass for a reason its author never intended.
//!
//! # Why this exists
//!
//! A large part of this workspace's test suite asserts things about Rust and Go source by
//! substring match — 1,396 `contains(` calls in `crates/codegen/tests/codegen_snapshots.rs`
//! alone, 598 of them negated. A negated substring assertion passes for two unrelated
//! reasons: the banned construct is genuinely absent, or **the needle no longer spells
//! anything the emitter produces**. Those are indistinguishable at the report line.
//!
//! The sharper failure is the *scoping* one. The idiom being replaced looks like this:
//!
//! ```ignore
//! let insert_body = &code[code.find("pub fn insert(").unwrap_or(0)..];
//! assert!(insert_body.contains(".write(&forgedb_wal::WalEntry"));
//! ```
//!
//! Both ends are wrong. `unwrap_or(0)` means a stale anchor does not fail — it widens the
//! window to the whole file from byte 0. And even on a hit the window runs to EOF: measured
//! on one real schema it covered 237 KB of a 261 KB file and contained eight WAL writes
//! belonging to seven *other* methods, so the assertion passed with the guarded call
//! deleted outright.
//!
//! That is worse than a vacuous pass. A vacuous pass stops claiming anything; a widened
//! scope keeps claiming the same thing about a bigger haystack, so it gets *easier* to
//! satisfy exactly as it becomes meaningless.
//!
//! **So the rule this crate is built around: a scope that cannot be found is an error,
//! never an empty scope and never a wider one.** There is no `unwrap_or` anywhere in this
//! API's contract. The scoping queries that enforce it land with their first consumer;
//! this module is the parsing and caching substrate they sit on.
//!
//! # What it is not
//!
//! It resolves no types. `syn` cannot tell you what a receiver's type is, so a query for
//! "a read of `.auto_sequences`" matches the field name on any receiver. Every guard this
//! replaces is syntactic, which is why that limit is acceptable — but do not reach for this
//! crate to express a type-dependent invariant. It cannot, and it will look like it did.
//!
//! It also does not make a wrong guard right. An AST raises precision and lowers
//! brittleness; it does not verify intent. Mutation testing is still the thing that proves
//! a guard guards, and per this workspace's own history the mutation belongs at the **call
//! site**, not only in the function under test.

mod cache;
mod scope;
mod source;

pub use cache::{cache_stats, cached_parse, cached_source, CacheStats};
pub use scope::{FnScope, MethodScope, ScopeError};
pub use source::RustSource;
