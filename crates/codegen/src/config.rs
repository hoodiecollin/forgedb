//! Generate-time runtime-behavior configuration (epic #126).
//!
//! `GenConfig` carries the schema-blind, generate-time knobs that tailor the
//! emitted Rust database (`RustGenerator`) — the *binding-time* model
//! (config epic #126):
//!
//! - **Tier A** — code *specialization* (emit/omit): `replication` gates whether
//!   `open_at` attaches the durable broker at all; `compaction` gates whether the
//!   auto-compaction trigger is emitted. The compiler optimizes around code that
//!   isn't there.
//! - **Tier B** — a baked `const` (same code, tailored number): the WAL checkpoint
//!   interval, compaction dead-row threshold, changefeed capacity, cascade depth,
//!   and the WAL fsync *policy value*.
//!
//! Every field is **schema-blind** (litmus: two apps with entirely different
//! schemas could share the value verbatim with identical effect). Nothing here is
//! per-model or field-aware — that would be a `.forge` directive, not a config
//! knob (guardrail G1). The *generator* consuming these is schema-aware; the
//! *knob meanings* are not.
//!
//! **Default = today's emitted code, byte-identical** (guardrail G6) — with the
//! ONE sanctioned exception that `replication` defaults to **off**: an unused
//! broker's second `F_FULLFSYNC` per write is pure waste (nothing is subscribed),
//! so removing it loses no data (#130). All other defaults reproduce the prior
//! output exactly, which the insta snapshot baseline enforces for free.

use std::fmt;

/// WAL fsync policy the generated durable write path binds (#129).
///
/// Currently **Tier B**: the value is baked into the `FsyncPolicy` passed to
/// `WalManager::open`, but `FsyncPolicy` is a runtime enum matched per-write in
/// the substrate (`crates/wal/src/writer.rs`), so `Never` still leaves a (dead,
/// value-known) branch in the substrate rather than removing the `sync_all` from
/// the binary. True Tier A (`sync_all` *gone*, compiler-optimized) needs a
/// const-generic policy or conditional emission at the generated call site — a
/// documented follow-up on #129. Default `Always` (byte-identical).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsyncMode {
    /// fsync every commit (an `F_FULLFSYNC` barrier on macOS). The v1 default —
    /// crash-safe with zero data loss for acked writes.
    Always,
    /// Never fsync: the OS flushes on its own schedule. **Durability-weakening**
    /// (guardrail G7) — an explicit operator opt-in with a real data-loss window
    /// on power loss; never the default.
    Never,
}

impl FsyncMode {
    /// The `forgedb_wal::FsyncPolicy` variant identifier this mode binds.
    pub fn wal_policy_variant(self) -> &'static str {
        match self {
            FsyncMode::Always => "Always",
            FsyncMode::Never => "Never",
        }
    }
}

impl fmt::Display for FsyncMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            FsyncMode::Always => "always",
            FsyncMode::Never => "never",
        })
    }
}

/// Generate-time runtime-behavior knobs baked into `database.rs` (#126).
///
/// Constructed by the CLI from `forgedb.toml`'s schema-blind `[runtime]` /
/// `[storage]` sections and threaded into `RustGenerator::generate_with_config`.
/// `Default` reproduces today's emitted code byte-for-byte except `replication`
/// (see the module docs / guardrail G6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenConfig {
    /// **Tier A, default OFF (#130).** Attach the durable replication broker in
    /// `open_at`. When off, no `DurableBroker::open` and no second per-write
    /// `F_FULLFSYNC` barrier are emitted — the write path's `if let Some(broker)`
    /// guard is statically dead. Turn on for apps that consume `/replicate` or
    /// run a browser read-replica follower (#82/#110).
    pub replication: bool,

    /// **Tier B, default `Always` (#129).** WAL fsync policy for the durable
    /// write path. `Never` is a durability-weakening opt-in (guardrail G7).
    pub fsync: FsyncMode,

    /// **Tier B, default 1000 (#131).** Mutations per collection between WAL
    /// checkpoints (`const WAL_CHECKPOINT_INTERVAL`). Bounds WAL growth /
    /// reopen-replay cost.
    pub wal_checkpoint_interval: u64,

    /// **Tier A, default ON (#134).** Emit the auto-compaction trigger. When off,
    /// the `dead_since_compaction >= COMPACTION_DEAD_THRESHOLD` check is omitted
    /// (storage still compacts via the explicit `Database::compact()`).
    pub compaction: bool,

    /// **Tier B, default 1000 (#133).** Dead row versions per model before an
    /// in-process compaction fires (`const COMPACTION_DEAD_THRESHOLD`).
    pub compaction_threshold: u64,

    /// **Tier B, default 1024 (#135).** In-process changefeed broadcast channel
    /// capacity (`ChangeFeed::new(cap)`) and the durable broker's in-memory
    /// buffer.
    pub changefeed_capacity: usize,

    /// **Tier B, default 64 (#150).** Maximum recursive `@on_delete(cascade)`
    /// depth (`const MAX_CASCADE_DEPTH`), a structural safety bound against a
    /// pathological FK cycle.
    pub max_cascade_depth: u32,

    /// **Tier B, default 3 (#146).** Default retry count for
    /// `transaction_optimistic` (`const DEFAULT_TXN_RETRIES`) — a conflict-losing
    /// Tier-2 optimistic transaction is re-run up to `retries + 1` times.
    /// Per-call control stays available via `transaction_retrying(retries, f)`.
    pub txn_max_retries: u32,

    /// **Tier B, default 50 (#141).** List-endpoint page size when the client
    /// omits `?limit` (`const PAGE_DEFAULT_LIMIT`). A clamp default, not a
    /// per-model query knob — schema-blind.
    pub page_default_limit: usize,

    /// **Tier B, default 1000 (#141).** Maximum list-endpoint page size; a client
    /// `?limit` above this is clamped (`const PAGE_MAX_LIMIT`). The generated
    /// handler clamps against this baked ceiling rather than the substrate's
    /// fixed `MAX_LIMIT`, so an app can tailor it without a runtime schema.
    pub page_max_limit: usize,

    /// **Tier A, default ON (#151).** Emit the unauthenticated `/metrics`
    /// endpoint (per-model live row counts). When off, neither the `__metrics`
    /// handler nor its route is emitted — the compiler optimizes around code that
    /// isn't there. `/health`, `/ready`, and `/snapshot` are unaffected.
    pub metrics: bool,

    /// **Tier B, default 250 (#148).** Browser read-replica auto-commit debounce
    /// in milliseconds (`COMMIT_DEBOUNCE_MS` in the static Worker bootstrap).
    /// Trades persist-write frequency vs the tab-close loss window. Substituted
    /// into the schema-agnostic bootstrap — the pipe stays schema-blind (#110).
    pub wasm_commit_debounce_ms: u64,

    /// **Tier B, default 100 (#148).** Browser read-replica auto-commit frame
    /// ceiling (`COMMIT_MAX_FRAMES`): commit immediately once this many
    /// replication frames are buffered, regardless of the debounce timer.
    pub wasm_commit_max_frames: u64,

    /// **Tier B, default 0 = disabled (#137).** Durable replication-log retention:
    /// when `> 0` (and `replication` is on), `Database::maintain()` prunes the
    /// broker log to the last N offsets (`prune_through(watermark − N)`), bounding
    /// `_replication.log` growth. 0 keeps the full log (byte-identical; today's
    /// behavior). A follower resuming from an offset older than the retained
    /// window must re-seed from a fresh backup — the operator's size/resume-window
    /// tradeoff.
    pub replication_log_retention: u64,

    /// **Tier A, default ON (#335 §10, absorbing #336).** Emit the `utoipa`
    /// `ToSchema` import and per-model derives.
    ///
    /// This is a *package-shape* decision rather than a runtime one, and it has
    /// to be made at generate time because **the orphan rule forbids making it
    /// later**: once `database.rs` and `api.rs` are separate crates, the derive
    /// lives in `core` and its `#[openapi(components(schemas(...)))]` consumer
    /// lives in `server`, so `server` cannot supply the impl — both the trait and
    /// the type are foreign to it. Proved by compile:
    /// `error[E0277]: the trait bound 'oc::Post: ToSchema' is not satisfied`.
    ///
    /// Decided from the app's **declared** target set, never from what a single
    /// invocation happened to emit: otherwise `generate rust` and `generate all`
    /// would produce different `database.rs` for the same project.
    ///
    /// Explicitly **not** a cargo feature — C11 forbids a generate-time knob
    /// becoming a feature of a shared member, and a feature is what an
    /// implementer reaches for here by reflex.
    pub web: bool,
}

impl Default for GenConfig {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl GenConfig {
    /// The default config, as a `const` so it can initialize a `thread_local!`
    /// `Cell`. Reproduces the pre-#126 emitted code byte-for-byte except the
    /// replication broker (G6 sanctioned exception, #130): default OFF.
    pub const DEFAULT: Self = Self {
        replication: false,
        fsync: FsyncMode::Always,
        wal_checkpoint_interval: 1000,
        compaction: true,
        compaction_threshold: 1000,
        changefeed_capacity: 1024,
        max_cascade_depth: 64,
        txn_max_retries: 3,
        page_default_limit: 50,
        page_max_limit: 1000,
        // ON by default: guardrail G6 — the default must reproduce today's
        // emitted code byte-for-byte, and today every database.rs derives
        // ToSchema unconditionally.
        web: true,
        metrics: true,
        wasm_commit_debounce_ms: 250,
        wasm_commit_max_frames: 100,
        replication_log_retention: 0,
    };

    /// **The one condition deciding whether `utoipa` is in play for this app**
    /// (#445).
    ///
    /// Both halves of that decision have to read *this* — the `ToSchema` derive
    /// and `use utoipa::ToSchema;` that [`crate::RustGenerator`] emits into
    /// `core/src/lib.rs`, and the `utoipa` pin that
    /// [`crate::CorePackage::cargo_toml`] writes into `core/Cargo.toml`. They are
    /// a matched pair: source naming a crate its own manifest does not pin is
    /// `error[E0432]: unresolved import 'utoipa'`.
    ///
    /// It shipped as two conditions that were merely *expected* to agree — the
    /// derive on `web` (the app's **declared** targets) and the pin on whether
    /// *this invocation* happened to emit an `api.rs`. Those differ for exactly
    /// the invocations that narrow: `generate rust` under `targets = ["all"]`,
    /// and `build --no-api`. Naming the condition once is what makes the
    /// disagreement unrepresentable rather than merely fixed.
    ///
    /// It cannot be deferred to the consumer: `ToSchema` and the generated types
    /// are both foreign to `server`, so the **orphan rule** means `server` can
    /// never supply the impl. It is a generate-time decision or it is nothing.
    ///
    /// The asymmetry is deliberate — `web` is resolved from the declared target
    /// set and falls back to ON, because a `utoipa` pin nothing derives against
    /// is inert while a missing derive is an `E0277` in `server` that no
    /// downstream change can fix.
    pub const fn needs_utoipa(&self) -> bool {
        self.web
    }

    /// The config that reproduces the *pre-#126* emitted code byte-for-byte,
    /// including the unconditional replication broker. Used by the replication
    /// guard tests (which assert the broker IS attached) and anywhere the legacy
    /// always-on-broker output must be regenerated.
    pub fn legacy_with_replication() -> Self {
        Self {
            replication: true,
            ..Self::default()
        }
    }
}
