//! Scenario 20 — mixed-workload driver (#218, under experiment #167).
//!
//! Run:
//!   make bench-workload                    # default: quick smoke matrix
//!   make bench-workload ARGS="--full"      # the full amplification ladder
//!
//! Why this exists, in one paragraph: #167 asks whether a second, in-place storage
//! model is worth building. Append-only's acknowledged price is churn — an update
//! writes a superseding version instead of overwriting — but that price has never
//! actually been measured, because every existing benchmark runs one operation on a
//! pristine corpus, and **the append tax only exists once a database has history**.
//! This driver produces that history and then measures through it, against two
//! in-place engines running an identical seeded workload at matched durability.
//!
//! It is an example rather than a Criterion bench on purpose: Criterion is a closed
//! loop (fire, wait, fire), which by construction removes the arrival-time variance
//! that burstiness is made of.

mod driver;
mod forgedb_target;
mod redb_target;
mod sqlite_target;

use std::time::Duration;

use driver::{
    Mix, Op, Pace, Phase, RunReport, ScanKind, UpdateWidth, WorkloadConfig, WorkloadTarget,
};
use forgedb_target::ForgeTarget;
use redb_target::RedbTarget;
use sqlite_target::SqliteTarget;

fn human(bytes: u64) -> String {
    if bytes >= 1 << 30 {
        format!("{:.2} GB", bytes as f64 / (1u64 << 30) as f64)
    } else if bytes >= 1 << 20 {
        format!("{:.1} MB", bytes as f64 / (1u64 << 20) as f64)
    } else if bytes >= 1 << 10 {
        format!("{:.0} KB", bytes as f64 / (1u64 << 10) as f64)
    } else {
        format!("{bytes} B")
    }
}

fn micros(v: u64) -> String {
    if v >= 1_000_000 {
        format!("{:.2}s", v as f64 / 1e6)
    } else if v >= 1_000 {
        format!("{:.1}ms", v as f64 / 1e3)
    } else {
        format!("{v}µs")
    }
}

fn build_target(engine: &str) -> Box<dyn WorkloadTarget> {
    match engine {
        "forgedb" => Box::new(ForgeTarget::new()),
        "sqlite" => Box::new(SqliteTarget::new()),
        "redb" => Box::new(RedbTarget::new()),
        other => panic!("unknown engine {other}"),
    }
}

fn print_run(rep: &RunReport, cfg: &WorkloadConfig) {
    println!("\n  engine: {}", rep.engine);
    if let Some(a) = rep.preload_amplification {
        print!("  amplification after preload: {a:.2}×");
        // A rung that silently fails to reach its target would make the ladder read as
        // though high amplification had been measured when it never occurred. The
        // generated code force-compacts once dead rows reach
        // COMPACTION_DEAD_THRESHOLD x COMPACTION_DEAD_CEILING_FACTOR — an ABSOLUTE row
        // count (4000 by default), not a ratio — so the ceiling is
        // `1 + 4000/live_rows`, and it gets *tighter* as the corpus grows. Reaching the
        // upper rungs at any realistic size therefore requires the `compaction_off`
        // generated variant; under the default config it is not merely slow to reach,
        // it is unreachable.
        if a < cfg.target_amplification * 0.9 {
            println!(
                "  << CAPPED (target {:.0}×) — auto-compaction ceiling, not a driver failure",
                cfg.target_amplification
            );
        } else {
            println!();
        }
    }
    // Two latency families, and the distinction matters for attribution:
    //   svc  = service time, the call itself → what the ENGINE costs.
    //   resp = measured from intended submission → what a CLIENT sees, including time
    //          spent queued behind other operations.
    // Every op runs on one thread (ForgeDB is single-writer by contract, so serializing
    // is the honest shape), which means a read issued behind a write waits out that
    // write's fsync. That inflates `resp` for reads on every engine equally — fair for
    // comparison, but it would hide engine differences if `resp` were reported alone.
    println!(
        "  {:<9} {:>7} {:>8} {:>9} {:>9} {:>9} {:>9} {:>10} {:>7}",
        "phase", "op", "count", "svc p50", "svc p99", "resp p99", "resp p99.9", "footprint", "amp"
    );
    for p in &rep.phases {
        for op in Op::ALL {
            let s = p.op(op);
            if s.count == 0 {
                continue;
            }
            println!(
                "  {:<9} {:>7} {:>8} {:>9} {:>9} {:>9} {:>9} {:>10} {:>7}",
                p.name,
                op.label(),
                s.count,
                micros(s.service.value_at_quantile(0.50)),
                micros(s.service.value_at_quantile(0.99)),
                micros(s.response.value_at_quantile(0.99)),
                micros(s.response.value_at_quantile(0.999)),
                if op == Op::ALL[0] { human(p.footprint) } else { String::new() },
                if op == Op::ALL[0] {
                    p.amplification().map(|a| format!("{a:.2}×")).unwrap_or_else(|| "—".into())
                } else {
                    String::new()
                },
            );
        }
        if let Some(false) = p.kept_up() {
            println!(
                "  {:<9} !! fell behind the offered rate: {:.1}s wall vs {:.1}s nominal \
                 — the engine could not absorb this load",
                p.name,
                p.wall.as_secs_f64(),
                p.nominal.unwrap_or_default().as_secs_f64()
            );
        }
        if p.maintain_calls > 0 {
            println!(
                "  {:<9} maintain: {} calls, {} total pause",
                p.name,
                p.maintain_calls,
                micros(p.maintain_pause.as_micros() as u64)
            );
        }
        if let Some(d) = p.reopen {
            // Scales with PHYSICAL rows: the open path rehydrates id_to_row and every
            // index across superseded versions too. Unlike scan cost this is not
            // something a smarter read path can avoid.
            println!(
                "  {:<9} reopen: {} (rss {})",
                p.name,
                micros(d.as_micros() as u64),
                p.rss.map(human).unwrap_or_else(|| "?".into())
            );
        }
    }
    let issued: usize = rep.phases.iter().map(|p| p.issued).sum();
    println!("  ops issued: {issued}");

    // Correctness check, not a timing: the driver knows exactly how many rows should
    // be live. An engine that disagrees has lost or duplicated data under mixed
    // mutation — a real consistency test the existing suite does not contain.
    let expected = driver::expected_live_rows(cfg);
    let actual = rep.phases.last().map(|p| p.live_rows).unwrap_or(0);
    if expected != actual {
        println!("  !! live-row mismatch: expected {expected}, got {actual}");
    } else {
        println!("  live rows: {actual} (matches the schedule)");
    }
}

/// One config, every engine.
fn compare(label: &str, cfg: &WorkloadConfig, engines: &[&str]) {
    println!("\n{}", "=".repeat(78));
    println!(
        "{label}\n  preload={} A_target={:.0}× skew={} width={:?} scan={:?} mix={}r/{}c/{}u/{}d/{}s",
        cfg.preload,
        cfg.target_amplification,
        cfg.skew,
        cfg.update_width,
        cfg.scan_kind,
        cfg.mix.read,
        cfg.mix.create,
        cfg.mix.update,
        cfg.mix.delete,
        cfg.mix.scan
    );
    println!("{}", "=".repeat(78));
    for e in engines {
        let mut t = build_target(e);
        let rep = driver::run(t.as_mut(), cfg);
        print_run(&rep, cfg);
    }
}

/// The Gate-3 checks, runnable on their own (`ARGS="--verify"`) because they are fast
/// and are about correctness rather than timing. They guard the properties the whole
/// comparison rests on — if any of these fail, every number the driver produces is
/// meaningless, so they belong next to the driver rather than in a report footnote.
fn verify() -> bool {
    let mut ok = true;

    // 1. Determinism: same seed → identical (op, key) sequence. This is what makes a
    //    cross-engine comparison a replay of one workload rather than three similar ones.
    let cfg = WorkloadConfig { preload: 500, ..WorkloadConfig::default() };
    let a = driver::build_schedules(&cfg);
    let b = driver::build_schedules(&cfg);
    let same = a.len() == b.len()
        && a.iter().zip(b.iter()).all(|(x, y)| x.ops == y.ops);
    println!("  [{}] same seed → identical op sequence", if same { "ok" } else { "FAIL" });
    ok &= same;

    // 2. Different seed → different sequence (otherwise check 1 passes vacuously).
    let c = driver::build_schedules(&WorkloadConfig { seed: cfg.seed + 1, ..cfg.clone() });
    let differs = a.iter().zip(c.iter()).any(|(x, y)| x.ops != y.ops);
    println!("  [{}] different seed → different op sequence", if differs { "ok" } else { "FAIL" });
    ok &= differs;

    // 3. The schedule never targets a key it has already deleted, and never reads a key
    //    that was never created — so a `miss` from an engine is a real defect rather
    //    than the schedule asking for something impossible.
    let mut live: std::collections::HashSet<u64> = (0..cfg.preload as u64).collect();
    let mut bad = 0usize;
    for s in &a {
        for &(op, key) in &s.ops {
            match op {
                Op::Create => {
                    live.insert(key);
                }
                Op::Read | Op::Update => {
                    if !live.contains(&key) {
                        bad += 1;
                    }
                }
                Op::Delete => {
                    if !live.remove(&key) {
                        bad += 1;
                    }
                }
                Op::Scan => {}
            }
        }
    }
    println!(
        "  [{}] schedule only targets live keys ({bad} violations)",
        if bad == 0 { "ok" } else { "FAIL" }
    );
    ok &= bad == 0;

    // 4. Every engine ends with exactly the row count the schedule implies. This is a
    //    real mixed-mutation consistency test — ForgeDB resolving version chains and
    //    tombstones correctly under interleaved create/update/delete — and no such test
    //    exists anywhere else in the suite.
    let tiny = WorkloadConfig {
        preload: 200,
        phases: vec![Phase { name: "check", pace: Pace::Closed { ops: 2_000 } }],
        target_amplification: 2.0,
        ..WorkloadConfig::default()
    };
    let expected = driver::expected_live_rows(&tiny);
    for engine in ["forgedb", "sqlite", "redb"] {
        let mut t = build_target(engine);
        let rep = driver::run(t.as_mut(), &tiny);
        let actual = rep.phases.last().map(|p| p.live_rows).unwrap_or(0);
        let good = actual == expected;
        println!(
            "  [{}] {engine}: {actual} live rows (schedule implies {expected})",
            if good { "ok" } else { "FAIL" }
        );
        ok &= good;
    }

    // 5. Amplification actually rose — if the preload churn did not produce history,
    //    the entire ladder would be measuring a pristine corpus under a different name.
    let mut t = build_target("forgedb");
    let amp = driver::preload(t.as_mut(), &WorkloadConfig { preload: 500, target_amplification: 4.0, ..cfg.clone() });
    let rose = amp.map(|a| a >= 3.5).unwrap_or(false);
    println!(
        "  [{}] preload reached the requested amplification ({})",
        if rose { "ok" } else { "FAIL" },
        amp.map(|a| format!("{a:.2}×")).unwrap_or_else(|| "none".into())
    );
    ok &= rose;

    ok
}

/// Smoke phases, sized so the default `make bench-workload` finishes in a sitting.
///
/// The binding constraint is not the measured phases — it is the **preload**, which
/// must issue `(A - 1) x preload` real updates to manufacture the history, each paying
/// an `F_FULLFSYNC` barrier. At A = 16 that is 16 fsyncs per preloaded row, so preload
/// size, not phase duration, is what sets wall-clock. Hence a small corpus here and the
/// real one behind `--full`.
/// Focused scan sweep — the measurement that actually targets the two suspected cliffs.
///
/// The mixed workload proves the system behaves under realistic load, but scans are ~1%
/// of a realistic mix, which yields ~10 samples per phase: nowhere near enough to
/// characterize a distribution, let alone locate a cliff. So this runs the scan path
/// directly at each amplification rung, on a corpus large enough for scan cost to
/// dominate, with enough samples for the percentiles to mean something.
///
/// Reported per rung for both scan paths:
///   * `Projection` → `FixedColumn::export` — whose zero-copy `mmap` requires the live
///     row set to be the dense prefix `[0, n)`. Amplification destroys that property, so
///     if the fast path's loss is the story, cost jumps as soon as A > 1 and then stays
///     roughly flat (the fallback gather is already per-index).
///   * `Narrow` → the full filterable/sortable column set.
///
/// Those two shapes are distinguishable in the output, which is the point: a jump-then-flat
/// curve indicts the dense-prefix condition, while a curve that keeps climbing with A
/// indicts reading dead versions. They imply different fixes.
fn scan_sweep(live: usize, ladder: &[f64], samples: usize, engines: &[&str]) {
    println!("\n{}", "=".repeat(78));
    println!("SCAN SWEEP — {live} live rows, {samples} scans per point");
    println!("{}", "=".repeat(78));
    println!(
        "  {:<9} {:>5} {:>8} {:>10} {:>10} {:>10} {:>10}",
        "engine", "A", "scan", "p50", "p99", "footprint", "amp"
    );

    // The in-place engines are measured ONCE, at A = 1, as a reference line. They have
    // no version chain, so their scan cost cannot depend on amplification — sweeping
    // them across the ladder would re-pay a fsync-bound preload to reproduce the same
    // number. Only ForgeDB sweeps.
    let points: Vec<(&str, f64)> = engines
        .iter()
        .flat_map(|&e| {
            if e == "forgedb" {
                ladder.iter().map(|&a| (e, a)).collect::<Vec<_>>()
            } else {
                vec![(e, 1.0)]
            }
        })
        .collect();

    {
        for (engine, a) in points {
            let a = a;
            let mut t = build_target(engine);
            let cfg = WorkloadConfig {
                preload: live,
                target_amplification: a,
                update_width: UpdateWidth::OneField,
                ..WorkloadConfig::default()
            };
            let actual_amp = driver::preload(t.as_mut(), &cfg);

            for kind in [ScanKind::Projection, ScanKind::Narrow] {
                let mut h = hdr();
                let mut rows = 0u64;
                for _ in 0..samples {
                    let start = std::time::Instant::now();
                    let out = t.scan(kind, usize::MAX);
                    let _ = h.record(start.elapsed().as_micros().max(1) as u64);
                    rows = out.rows;
                }
                println!(
                    "  {:<9} {:>5} {:>8} {:>10} {:>10} {:>10} {:>10}",
                    engine,
                    format!("{a:.0}×"),
                    format!("{kind:?}"),
                    micros(h.value_at_quantile(0.50)),
                    micros(h.value_at_quantile(0.99)),
                    if kind == ScanKind::Projection { human(t.footprint()) } else { String::new() },
                    if kind == ScanKind::Projection {
                        // Requested A is in the `A` column; this is what was ACTUALLY
                        // reached. They diverge above the auto-compaction ceiling
                        // (`1 + 4000/live_rows`), which tightens as the corpus grows.
                        actual_amp.map(|x| format!("{x:.2}×")).unwrap_or_else(|| "—".into())
                    } else {
                        String::new()
                    },
                );
                let _ = rows;
            }
        }
    }
}

fn hdr() -> hdrhistogram::Histogram<u64> {
    hdrhistogram::Histogram::<u64>::new_with_bounds(1, 300_000_000, 3).unwrap()
}

fn smoke_phases() -> Vec<Phase> {
    vec![
        Phase { name: "warmup", pace: Pace::Open { rate: 200, duration: Duration::from_secs(2) } },
        Phase { name: "steady", pace: Pace::Open { rate: 400, duration: Duration::from_secs(3) } },
        Phase { name: "burst", pace: Pace::Open { rate: 2000, duration: Duration::from_secs(2) } },
        Phase { name: "recover", pace: Pace::Open { rate: 400, duration: Duration::from_secs(3) } },
    ]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let full = args.iter().any(|a| a == "--full");
    let engines: Vec<&str> = if args.iter().any(|a| a == "--forgedb-only") {
        vec!["forgedb"]
    } else {
        vec!["forgedb", "sqlite", "redb"]
    };

    if args.iter().any(|a| a == "--verify") {
        println!("Driver self-checks (#218 Gate 3):");
        std::process::exit(if verify() { 0 } else { 1 });
    }

    if args.iter().any(|a| a == "--scan-sweep") {
        // Scans are ~1% of a realistic mix, so the mixed run cannot sample them well
        // enough to locate a cliff. This measures the scan path directly.
        let (live, ladder, samples): (usize, &[f64], usize) = if full {
            (100_000, &[1.0, 2.0, 4.0, 8.0, 16.0, 32.0], 100)
        } else {
            (10_000, &[1.0, 2.0, 4.0, 8.0], 50)
        };
        scan_sweep(live, ladder, samples, &engines);
        return;
    }

    println!(
        "ForgeDB mixed-workload driver (#218)\n\
         Open-loop arrival schedule, seeded and identical across engines; latency is\n\
         recorded against intended submission time, so stalls appear in the tail rather\n\
         than being absorbed. Durability is matched: ForgeDB FsyncPolicy::Always,\n\
         SQLite synchronous=FULL + fullfsync=1, redb Durability::Immediate, one\n\
         barrier per operation everywhere."
    );

    let base = WorkloadConfig {
        phases: if full { WorkloadConfig::default().phases } else { smoke_phases() },
        // Reaching amplification A inherently costs `(A - 1) x preload` real updates,
        // each paying a durability barrier — so the full ladder issues ~57x preload
        // updates in total. 20k keeps that a long-but-finite deliberate run rather than
        // an overnight one. (Speedup available if this ever binds: the preload only
        // needs to CONSTRUCT history, not measure it, so it could legitimately run
        // against the generated `fsync_never` variant for byte-identical on-disk state
        // at a fraction of the barriers. Left undone because it means making the target
        // generic over the variant module, which is a refactor, not a tuning knob.)
        preload: if full { 20_000 } else { 1_000 },
        ..WorkloadConfig::default()
    };

    // --- The amplification ladder ------------------------------------------------
    // The headline. Same workload, same seed, only the amount of accumulated history
    // differs. A flat curve means the append tax is a bounded constant; a rising one
    // is the concrete case for an in-place variant.
    let ladder: &[f64] = if full { &[1.0, 2.0, 4.0, 8.0, 16.0, 32.0] } else { &[1.0, 4.0, 16.0] };
    for &a in ladder {
        compare(
            &format!("LADDER — amplification {a:.0}×"),
            &WorkloadConfig { target_amplification: a, ..base.clone() },
            &engines,
        );
    }

    // --- Secondary axes, varied one at a time against the ladder's midpoint -------
    // Full runs only: the ladder is the headline, and in smoke mode the axes would
    // multiply the fsync-bound preload cost for results too small to read anyway.
    if !full {
        println!(
            "\n(smoke mode: ladder only. Secondary axes — scan kind, update width, skew,\n\
             churn-heavy mix, compaction — run under ARGS=\"--full\".)"
        );
        return;
    }
    let mid = WorkloadConfig { target_amplification: 8.0, ..base.clone() };

    // Which scan path pays: Projection exercises FixedColumn::export (whose zero-copy
    // mmap needs a dense prefix); Narrow drags gather_buffered's whole-region read.
    // Run separately so a cliff can be attributed rather than merely observed.
    compare(
        "AXIS scan-kind — narrow scan (gather_buffered path)",
        &WorkloadConfig { scan_kind: ScanKind::Narrow, ..mid.clone() },
        &engines,
    );

    // Update width: ForgeDB writes the whole row either way, so this measures what
    // in-place would save by writing only the changed column.
    compare(
        "AXIS update-width — all 22 columns rewritten",
        &WorkloadConfig { update_width: UpdateWidth::AllFields, ..mid.clone() },
        &engines,
    );

    // Uniform churn spreads dead bytes across the corpus instead of concentrating
    // them in a few long version chains — opposite stress on compaction.
    compare(
        "AXIS skew — uniform key selection",
        &WorkloadConfig { skew: 0.0, ..mid.clone() },
        &engines,
    );

    // Update-dominated mix: the append model's worst shape.
    compare(
        "AXIS mix — churn-heavy (70% updates)",
        &WorkloadConfig { mix: Mix::churn(), ..mid.clone() },
        &engines,
    );

    // Compaction is what BOUNDS the append tax, so its cost belongs in the comparison
    // rather than being excluded from it. This is also the threshold test: does the
    // curve recover after a compaction cycle?
    compare(
        "AXIS maintenance — compaction every 5k ops",
        &WorkloadConfig { maintain_every: Some(5_000), ..mid.clone() },
        &engines,
    );
}
