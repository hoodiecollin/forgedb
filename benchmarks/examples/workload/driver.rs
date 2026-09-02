use std::time::{Duration, Instant};

use hdrhistogram::Histogram;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, Zipf};

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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum UpdateWidth {
    OneField,
    AllFields,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ScanKind {
    Projection,
    Narrow,
}

pub trait WorkloadTarget {
    fn name(&self) -> &'static str;

    fn create(&mut self, key: u64) -> OpOutcome;
    fn read(&mut self, key: u64) -> OpOutcome;
    fn update(&mut self, key: u64, width: UpdateWidth) -> OpOutcome;
    fn delete(&mut self, key: u64) -> OpOutcome;
    fn scan(&mut self, kind: ScanKind, limit: usize) -> OpOutcome;

    fn maintain(&mut self);

    fn footprint(&self) -> u64;

    fn live_rows(&mut self) -> usize;

    fn physical_rows(&mut self) -> Option<usize> {
        None
    }

    fn reopen(&mut self) -> Option<Duration> {
        None
    }
}

#[derive(Copy, Clone, Debug)]
pub enum Pace {
    Open { rate: u32, duration: Duration },
    Closed { ops: usize },
}

impl Pace {
    fn op_count(self) -> usize {
        match self {
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

#[derive(Copy, Clone, Debug)]
pub struct Mix {
    pub read: u32,
    pub create: u32,
    pub update: u32,
    pub delete: u32,
    pub scan: u32,
}

impl Mix {
    pub fn app() -> Self {
        Mix { read: 70, create: 10, update: 15, delete: 4, scan: 1 }
    }
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
    pub preload: usize,
    pub target_amplification: f64,
    pub skew: f64,
    pub update_width: UpdateWidth,
    pub scan_kind: ScanKind,
    pub scan_limit: usize,
    pub maintain_every: Option<usize>,
    pub preload_churn_skew: Option<f64>,
    #[allow(dead_code)]
    pub payload_bytes: usize,
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
            preload_churn_skew: None,
            payload_bytes: 256,
        }
    }
}

pub struct Schedule {
    pub ops: Vec<(Op, u64)>,
}

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

fn sample_rank(rng: &mut StdRng, zipf: Option<&Zipf<f64>>, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    match zipf {
        Some(z) => (z.sample(rng) as usize).saturating_sub(1) % n,
        None => rng.gen_range(0..n),
    }
}

pub fn build_schedules(cfg: &WorkloadConfig) -> Vec<Schedule> {
    let mut rng = StdRng::seed_from_u64(cfg.seed);
    let mut keys = KeySet::new(cfg.preload);
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

pub struct OpStats {
    pub service: Histogram<u64>,
    pub response: Histogram<u64>,
    pub count: u64,
    pub misses: u64,
    pub rows: u64,
}

impl OpStats {
    fn new() -> Self {
        let h = || Histogram::<u64>::new_with_bounds(1, 300_000_000, 3).unwrap();
        Self { service: h(), response: h(), count: 0, misses: 0, rows: 0 }
    }
}

pub struct PhaseReport {
    pub name: &'static str,
    pub stats: Vec<OpStats>,
    pub wall: Duration,
    pub issued: usize,
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
    pub fn kept_up(&self) -> Option<bool> {
        self.nominal.map(|n| self.wall <= n + n / 10)
    }
}

pub struct RunReport {
    pub engine: &'static str,
    pub phases: Vec<PhaseReport>,
    pub preload_amplification: Option<f64>,
}

fn dispatch<T: WorkloadTarget + ?Sized>(t: &mut T, op: Op, key: u64, cfg: &WorkloadConfig) -> OpOutcome {
    match op {
        Op::Read => t.read(key),
        Op::Create => t.create(key),
        Op::Update => t.update(key, cfg.update_width),
        Op::Delete => t.delete(key),
        Op::Scan => t.scan(cfg.scan_kind, cfg.scan_limit),
    }
}

pub fn preload<T: WorkloadTarget + ?Sized>(target: &mut T, cfg: &WorkloadConfig) -> Option<f64> {
    for k in 0..cfg.preload as u64 {
        target.create(k);
    }
    if cfg.target_amplification > 1.0 && cfg.preload > 0 {
        let extra = ((cfg.target_amplification - 1.0) * cfg.preload as f64) as usize;
        match cfg.preload_churn_skew {
            None => {
                for i in 0..extra {
                    target.update((i % cfg.preload) as u64, cfg.update_width);
                }
            }
            Some(skew) => {
                let mut rng = StdRng::seed_from_u64(cfg.seed ^ 0x5EED_C4A5);
                let zipf = if skew > 0.0 {
                    Zipf::new(cfg.preload.max(1_000) as u64, skew).ok()
                } else {
                    None
                };
                for _ in 0..extra {
                    let k = sample_rank(&mut rng, zipf.as_ref(), cfg.preload) as u64;
                    target.update(k, cfg.update_width);
                }
            }
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

pub fn rss_bytes() -> Option<u64> {
    let pid = std::process::id();
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let kb: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(kb * 1024)
}

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
