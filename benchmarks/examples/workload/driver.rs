//! The #218 mixed-workload driver.
//!
//! Every other benchmark in this repo is ONE operation, timed in isolation, on a
//! database shaped to make that operation easy to measure. That cannot answer #167's
//! question, because **the append-only tax is history-dependent**: it depends on how
//! mutated the database already is when the next operation lands. A pristine corpus
//! cannot exhibit it by construction.
//!
//! So this is artillery's model applied to in-process calls:
//!   * a weighted mix of read/create/update/delete/scan,
//!   * phased **arrival rates** (warmup → steady → burst → recover),
//!   * **open-loop** pacing, so a stall shows up as queueing delay instead of
//!     silently throttling the offered load,
//!   * HDR histograms, because the interesting number is the tail.
//!
//! ## Two properties worth stating explicitly
//!
//! **1. The op sequence is precomputed and timing-independent.** The whole
//! `(op, key)` schedule is derived from the seed alone, before the clock starts. That
//! is what makes the cross-engine comparison fair: ForgeDB, SQLite and redb replay a
//! byte-identical workload, and a slow engine cannot "get an easier workload" by
//! falling behind. It also makes runs reproducible.
//!
//! **2. Latency is measured against INTENDED submission time.** Recording
//! `completion - start_of_call` measures service time and quietly discards queueing
//! delay — the classic coordinated-omission artifact, where a system that stalls for a
//! second looks fast because nothing was sent during the stall. Under an open loop the
//! arrival schedule is fixed in advance, so a stall pushes every subsequent op's
//! response time up, which is the honest picture. Both are recorded; `response` is the
//! one to report.

use std::time::{Duration, Instant};

use hdrhistogram::Histogram;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, Zipf};

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Op {
    Read,
    Create,
    Update,
    Delete,
    Scan,
}

impl Op {
    pub const ALL: [Op; 5] = [Op::Read, Op::Create, Op::Update, Op::Delete, Op::Scan];
    pub fn idx(self) -> usize {
        match self {
            Op::Read => 0,
            Op::Create => 1,
            Op::Update => 2,
            Op::Delete => 3,
            Op::Scan => 4,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Op::Read => "read",
            Op::Create => "create",
            Op::Update => "update",
            Op::Delete => "delete",
            Op::Scan => "scan",
        }
    }
}

/// What a single operation did. `rows` lets a scan report how much it materialized,
/// which is how the amplification tax becomes visible as work rather than just time.
#[derive(Copy, Clone, Debug)]
pub struct OpOutcome {
    pub ok: bool,
    pub rows: u64,
}

impl OpOutcome {
    pub fn ok() -> Self {
        Self { ok: true, rows: 1 }
    }
    pub fn miss() -> Self {
        Self { ok: false, rows: 0 }
    }
    pub fn rows(n: u64) -> Self {
        Self { ok: true, rows: n }
    }
}

/// How much of a row an update rewrites.
///
/// ForgeDB's `update` writes the FULL row whatever changed — the whole record as JSON
/// to the WAL, then an append to every column — so the per-update cost scales with the
/// row's total width. An in-place engine would write only the changed column. On a
/// 6-field model that difference is nearly unmeasurable, which is why `bench.forge`
/// carries the 22-column `Metric`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum UpdateWidth {
    /// Mutate a single field.
    OneField,
    /// Mutate every field.
    AllFields,
}

/// Which generated read path a scan exercises. These are NOT interchangeable — they
/// hit the two different storage calls under suspicion, so keeping them separate is
/// what lets a cliff be attributed instead of just observed:
///
/// * [`ScanKind::Projection`] → the declared `@projection` scan → `FixedColumn::export`,
///   whose zero-copy `mmap` requires the requested indices to be the dense prefix
///   `[0, n)`. One update anywhere moves a row to the tail, the live set stops being
///   dense, and every fixed column silently falls back to a per-index gather copy.
/// * [`ScanKind::Narrow`] → the internal narrow scan → `VariableColumn::gather_buffered`,
///   which ignores the requested indices for the read and pulls the entire offsets
///   index and data region, dead versions included.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ScanKind {
    Projection,
    Narrow,
}

// ---------------------------------------------------------------------------
// The target abstraction
// ---------------------------------------------------------------------------

/// One benchmarked engine. Implementations must apply the driver's operations
/// literally — no batching a paced workload into fewer transactions, since per-op
/// durability is exactly what is being compared.
pub trait WorkloadTarget {
    fn name(&self) -> &'static str;

    fn create(&mut self, key: u64) -> OpOutcome;
    fn read(&mut self, key: u64) -> OpOutcome;
    fn update(&mut self, key: u64, width: UpdateWidth) -> OpOutcome;
    fn delete(&mut self, key: u64) -> OpOutcome;
    fn scan(&mut self, kind: ScanKind, limit: usize) -> OpOutcome;

    /// Compaction / checkpoint / vacuum — whatever the engine calls reclaiming space.
    /// Timed separately and reported as a pause, because for an append-only engine
    /// this is the mechanism that BOUNDS the churn tax, and its cost is part of the
    /// honest price of the model.
    fn maintain(&mut self);

    /// On-disk bytes for the whole data directory.
    fn footprint(&self) -> u64;

    /// Rows an engine would return from a full scan.
    fn live_rows(&mut self) -> usize;

    /// Physically stored rows including superseded versions and tombstones.
    /// `None` where the concept genuinely does not exist (redb rewrites in place and
    /// has no version chain) — never fabricate an amplification number for such an
    /// engine, it would read as though it had one.
    fn physical_rows(&mut self) -> Option<usize> {
        None
    }

    /// Close and reopen the database, returning how long the reopen took.
    ///
    /// High signal for #167 and easy to overlook: ForgeDB's open path rehydrates
    /// `id_to_row` and every index by walking **physical** rows, superseded versions
    /// included. So reopen cost should scale with amplification directly, and unlike
    /// scan cost it is not something a smarter read path can avoid — resolving which
    /// version is current is inherent to having versions. If it does scale, it is a
    /// cost of the mutation model itself rather than of this implementation of it.
    fn reopen(&mut self) -> Option<Duration> {
        None
    }
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// How a phase offers load.
#[derive(Copy, Clone, Debug)]
pub enum Pace {
    /// **Open loop.** `rate` ops/sec for `duration`; arrival times are fixed in
    /// advance and the driver does not wait for the previous op before the next one
    /// comes due. This is the mode that can express a burst and can observe a stall.
    Open { rate: u32, duration: Duration },
    /// **Closed loop.** Exactly `ops` operations back to back, as fast as the target
    /// accepts them. Measures peak throughput; structurally cannot express queueing.
    Closed { ops: usize },
}

impl Pace {
    fn op_count(self) -> usize {
        match self {
            // Derived from rate × duration, never from elapsed time, so the schedule
            // stays identical across engines regardless of how fast each one runs.
            Pace::Open { rate, duration } => (rate as f64 * duration.as_secs_f64()) as usize,
            Pace::Closed { ops } => ops,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Phase {
    pub name: &'static str,
    pub pace: Pace,
}

/// Relative op weights. Not required to sum to anything in particular.
#[derive(Copy, Clone, Debug)]
pub struct Mix {
    pub read: u32,
    pub create: u32,
    pub update: u32,
    pub delete: u32,
    pub scan: u32,
}

impl Mix {
    /// Read-heavy with steady mutation — an ordinary application's shape.
    pub fn app() -> Self {
        Mix { read: 70, create: 10, update: 15, delete: 4, scan: 1 }
    }
    /// Update-dominated: the append-only model's worst case, and the shape that
    /// drives amplification up fastest.
    pub fn churn() -> Self {
        Mix { read: 20, create: 5, update: 70, delete: 4, scan: 1 }
    }
    fn total(&self) -> u32 {
        self.read + self.create + self.update + self.delete + self.scan
    }
}

#[derive(Clone, Debug)]
pub struct WorkloadConfig {
    pub seed: u64,
    pub phases: Vec<Phase>,
    pub mix: Mix,
    /// Rows loaded before measurement starts.
    pub preload: usize,
    /// Updates applied during preload to reach a target amplification, expressed as a
    /// multiple of the live row count. `1.0` means "pristine". This is the ladder
    /// dimension: the same workload measured at A = 1, 2, 4, 8, 16, 32 produces a
    /// curve rather than a point, and a curve is what distinguishes "append-only
    /// costs a constant factor" from "append-only falls off a cliff".
    pub target_amplification: f64,
    /// Zipf exponent for key selection. `0.0` = uniform. ~1.0 is a realistic hot-key
    /// distribution. Skew matters because it decides whether churn concentrates on a
    /// few rows (long version chains, small dead-byte total) or spreads across the
    /// corpus (short chains, large dead-byte total) — opposite stress on the engine.
    pub skew: f64,
    pub update_width: UpdateWidth,
    pub scan_kind: ScanKind,
    pub scan_limit: usize,
    /// Call `maintain()` every N ops, or never.
    pub maintain_every: Option<usize>,
}

impl Default for WorkloadConfig {
    fn default() -> Self {
        Self {
            seed: 0xF0_1D_DB,
            phases: vec![
                Phase { name: "warmup", pace: Pace::Open { rate: 100, duration: Duration::from_secs(5) } },
                Phase { name: "steady", pace: Pace::Open { rate: 500, duration: Duration::from_secs(20) } },
                Phase { name: "burst", pace: Pace::Open { rate: 5000, duration: Duration::from_secs(5) } },
                Phase { name: "recover", pace: Pace::Open { rate: 500, duration: Duration::from_secs(20) } },
            ],
            mix: Mix::app(),
            preload: 10_000,
            target_amplification: 1.0,
            skew: 1.0,
            update_width: UpdateWidth::OneField,
            scan_kind: ScanKind::Projection,
            scan_limit: 1_000,
            maintain_every: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Schedule
// ---------------------------------------------------------------------------

/// The precomputed `(op, key)` sequence for one phase.
///
/// Built entirely from the seed with **no reference to the clock or to any engine's
/// state**, which is what makes the cross-engine comparison an apples-to-apples replay
/// rather than three loosely-similar workloads.
pub struct Schedule {
    pub ops: Vec<(Op, u64)>,
}

/// Driver-side model of which keys are live.
///
/// The driver has to track this itself rather than asking the engine, for the same
/// reason: if key choice depended on engine state, two engines could diverge and the
/// comparison would quietly stop being fair. It also gives the correctness check —
/// after a run, `live_rows()` must equal this set's size, which is a real mixed-mutation
/// consistency test that the existing suite does not have.
struct KeySet {
    live: Vec<u64>,
    next: u64,
}

impl KeySet {
    fn new(preload: usize) -> Self {
        Self { live: (0..preload as u64).collect(), next: preload as u64 }
    }
    fn insert(&mut self) -> u64 {
        let k = self.next;
        self.next += 1;
        self.live.push(k);
        k
    }
    /// Pick a live key by rank, so skew is applied over positions rather than key
    /// values (key values are monotonic, so skew over values would just mean "old").
    fn pick(&self, rank: usize) -> Option<u64> {
        if self.live.is_empty() {
            return None;
        }
        Some(self.live[rank % self.live.len()])
    }
    fn remove(&mut self, key: u64) {
        if let Some(pos) = self.live.iter().position(|&k| k == key) {
            self.live.swap_remove(pos);
        }
    }
}

/// Sample a rank in `[0, n)` under the configured skew.
fn sample_rank(rng: &mut StdRng, zipf: Option<&Zipf<f64>>, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    match zipf {
        // Zipf yields 1-based ranks over a fixed support; fold into the live range so
        // the distribution's shape is preserved as the corpus grows and shrinks.
        Some(z) => (z.sample(rng) as usize).saturating_sub(1) % n,
        None => rng.gen_range(0..n),
    }
}

/// Build the full schedule for every phase up front.
pub fn build_schedules(cfg: &WorkloadConfig) -> Vec<Schedule> {
    let mut rng = StdRng::seed_from_u64(cfg.seed);
    let mut keys = KeySet::new(cfg.preload);
    // Support of the skew distribution. Fixed (not tied to the live count) so the
    // distribution does not silently reshape itself mid-run.
    let support = cfg.preload.max(1_000) as u64;
    let zipf = if cfg.skew > 0.0 { Zipf::new(support, cfg.skew).ok() } else { None };

    let total_weight = cfg.mix.total().max(1);
    let mut out = Vec::new();

    for phase in &cfg.phases {
        let n = phase.pace.op_count();
        let mut ops = Vec::with_capacity(n);
        for _ in 0..n {
            let roll = rng.gen_range(0..total_weight);
            let m = &cfg.mix;
            let op = if roll < m.read {
                Op::Read
            } else if roll < m.read + m.create {
                Op::Create
            } else if roll < m.read + m.create + m.update {
                Op::Update
            } else if roll < m.read + m.create + m.update + m.delete {
                Op::Delete
            } else {
                Op::Scan
            };

            // Resolve to the op that will actually run. A read/update/delete with
            // nothing live degrades to a create — that keeps every schedule the same
            // length (so configs stay comparable) and keeps the driver's live-set
            // model exactly in step with what the engines are told to do.
            let resolved = match op {
                Op::Create => (Op::Create, keys.insert()),
                Op::Scan => (Op::Scan, 0),
                Op::Read | Op::Update | Op::Delete => {
                    let rank = sample_rank(&mut rng, zipf.as_ref(), keys.live.len());
                    match keys.pick(rank) {
                        Some(k) => {
                            if op == Op::Delete {
                                keys.remove(k);
                            }
                            (op, k)
                        }
                        None => (Op::Create, keys.insert()),
                    }
                }
            };
            ops.push(resolved);
        }
        out.push(Schedule { ops });
    }
    out
}

// ---------------------------------------------------------------------------
// Reporting
// ---------------------------------------------------------------------------

pub struct OpStats {
    /// Service time: how long the call itself took.
    pub service: Histogram<u64>,
    /// Response time: completion measured from INTENDED submission. Under an open
    /// loop this includes queueing delay, and it is the number to report.
    pub response: Histogram<u64>,
    pub count: u64,
    pub misses: u64,
    pub rows: u64,
}

impl OpStats {
    fn new() -> Self {
        // 1 µs .. 5 min at 3 significant figures.
        let h = || Histogram::<u64>::new_with_bounds(1, 300_000_000, 3).unwrap();
        Self { service: h(), response: h(), count: 0, misses: 0, rows: 0 }
    }
}

pub struct PhaseReport {
    pub name: &'static str,
    pub stats: Vec<OpStats>,
    pub wall: Duration,
    pub issued: usize,
    /// Whether the engine kept up with the offered rate. If the run took materially
    /// longer than the phase's nominal duration, the engine could not absorb the load
    /// — which is a finding in itself, not a measurement error.
    pub nominal: Option<Duration>,
    pub footprint: u64,
    pub live_rows: usize,
    pub physical_rows: Option<usize>,
    pub rss: Option<u64>,
    pub maintain_pause: Duration,
    pub maintain_calls: u32,
    pub reopen: Option<Duration>,
}

impl PhaseReport {
    pub fn amplification(&self) -> Option<f64> {
        match (self.physical_rows, self.live_rows) {
            (Some(p), l) if l > 0 => Some(p as f64 / l as f64),
            _ => None,
        }
    }
    pub fn op(&self, op: Op) -> &OpStats {
        &self.stats[op.idx()]
    }
    /// Did the engine fall behind the offered load?
    pub fn kept_up(&self) -> Option<bool> {
        self.nominal.map(|n| self.wall <= n + n / 10)
    }
}

pub struct RunReport {
    pub engine: &'static str,
    pub phases: Vec<PhaseReport>,
    pub preload_amplification: Option<f64>,
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

fn dispatch<T: WorkloadTarget + ?Sized>(t: &mut T, op: Op, key: u64, cfg: &WorkloadConfig) -> OpOutcome {
    match op {
        Op::Read => t.read(key),
        Op::Create => t.create(key),
        Op::Update => t.update(key, cfg.update_width),
        Op::Delete => t.delete(key),
        Op::Scan => t.scan(cfg.scan_kind, cfg.scan_limit),
    }
}

/// Load the corpus and churn it to the configured amplification before measuring.
///
/// The churn is the whole point: measuring a mixed workload on a pristine corpus would
/// reproduce the existing suite's blind spot. Updates are spread across the corpus
/// round-robin rather than concentrated, so amplification arrives as "every row has A
/// versions" rather than "one row has A×L versions" — the former is what a real
/// application's steady state looks like.
pub fn preload<T: WorkloadTarget + ?Sized>(target: &mut T, cfg: &WorkloadConfig) -> Option<f64> {
    for k in 0..cfg.preload as u64 {
        target.create(k);
    }
    if cfg.target_amplification > 1.0 && cfg.preload > 0 {
        let extra = ((cfg.target_amplification - 1.0) * cfg.preload as f64) as usize;
        for i in 0..extra {
            target.update((i % cfg.preload) as u64, cfg.update_width);
        }
    }
    let live = target.live_rows();
    target.physical_rows().map(|p| if live > 0 { p as f64 / live as f64 } else { 0.0 })
}

pub fn run<T: WorkloadTarget + ?Sized>(target: &mut T, cfg: &WorkloadConfig) -> RunReport {
    let schedules = build_schedules(cfg);
    let preload_amplification = preload(target, cfg);

    let mut phases = Vec::new();
    let mut since_maintain = 0usize;

    for (phase, schedule) in cfg.phases.iter().zip(schedules.iter()) {
        let mut stats: Vec<OpStats> = (0..5).map(|_| OpStats::new()).collect();
        let mut maintain_pause = Duration::ZERO;
        let mut maintain_calls = 0u32;

        let phase_start = Instant::now();
        let gap = match phase.pace {
            Pace::Open { rate, .. } => Some(Duration::from_secs_f64(1.0 / rate as f64)),
            Pace::Closed { .. } => None,
        };

        for (i, &(op, key)) in schedule.ops.iter().enumerate() {
            // Open loop: this op came due at a time fixed before the run began. If the
            // engine is behind, `intended` is already in the past and the wait is
            // skipped — the lateness lands in the response histogram instead of being
            // absorbed by the driver, which is the whole anti-coordinated-omission
            // mechanism.
            let intended = gap.map(|g| phase_start + g.mul_f64(i as f64));
            if let Some(t) = intended {
                let now = Instant::now();
                if t > now {
                    std::thread::sleep(t - now);
                }
            }

            let started = Instant::now();
            let outcome = dispatch(target, op, key, cfg);
            let done = Instant::now();

            let s = &mut stats[op.idx()];
            s.count += 1;
            s.rows += outcome.rows;
            if !outcome.ok {
                s.misses += 1;
            }
            let service = done.duration_since(started).as_micros() as u64;
            let response = intended
                .map(|t| done.saturating_duration_since(t).as_micros() as u64)
                .unwrap_or(service);
            let _ = s.service.record(service.max(1));
            let _ = s.response.record(response.max(1));

            since_maintain += 1;
            if let Some(every) = cfg.maintain_every {
                if since_maintain >= every {
                    since_maintain = 0;
                    let m = Instant::now();
                    target.maintain();
                    maintain_pause += m.elapsed();
                    maintain_calls += 1;
                }
            }
        }

        let wall = phase_start.elapsed();
        let live_rows = target.live_rows();
        let physical_rows = target.physical_rows();
        // Probed only on the final phase: reopening mid-run would discard warm page
        // cache and contaminate the next phase's latencies with cold-start effects.
        let is_last = std::ptr::eq(phase, cfg.phases.last().unwrap());
        let reopen = if is_last { target.reopen() } else { None };
        phases.push(PhaseReport {
            name: phase.name,
            stats,
            wall,
            issued: schedule.ops.len(),
            nominal: match phase.pace {
                Pace::Open { duration, .. } => Some(duration),
                Pace::Closed { .. } => None,
            },
            footprint: target.footprint(),
            live_rows,
            physical_rows,
            rss: rss_bytes(),
            maintain_pause,
            maintain_calls,
            reopen,
        });
    }

    RunReport { engine: target.name(), phases, preload_amplification }
}

/// Count of live keys the schedule implies, for the post-run consistency check.
pub fn expected_live_rows(cfg: &WorkloadConfig) -> usize {
    let schedules = build_schedules(cfg);
    let mut live = cfg.preload as i64;
    for s in &schedules {
        for &(op, _) in &s.ops {
            match op {
                Op::Create => live += 1,
                Op::Delete => live -= 1,
                _ => {}
            }
        }
    }
    live.max(0) as usize
}

/// Resident set size. Sampled once per phase, so shelling out to `ps` is cheap enough
/// and avoids a platform-specific dependency for a diagnostic number.
pub fn rss_bytes() -> Option<u64> {
    let pid = std::process::id();
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let kb: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(kb * 1024)
}

/// Sum of every file under a directory (or the file itself).
pub fn dir_size(path: &std::path::Path) -> u64 {
    if path.is_file() {
        return path.metadata().map(|m| m.len()).unwrap_or(0);
    }
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for e in entries.flatten() {
            let p = e.path();
            total += if p.is_dir() { dir_size(&p) } else { p.metadata().map(|m| m.len()).unwrap_or(0) };
        }
    }
    total
}

