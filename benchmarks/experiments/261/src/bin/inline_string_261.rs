//! Experiment #261 — does an inline `string(N)` slot beat pointer indirection?
//!
//! Gates the **soft** `string(N)` form of #238. The hard `string(N!)` form is
//! justified independently (#252) and is not on trial here.
//!
//! # The grid
//!
//! Three axes, because the answer plausibly depends on all three and a single
//! curve would hide it:
//!
//! 1. **Inline capacity N** — `tiny` 4, `small` 16, `modest` 64, `large` 256,
//!    `huge` 1024 chars. This is the author-declared slot width, so it is the
//!    knob the design actually exposes.
//! 2. **Overflow value length**, relative to N — `just_over` (1–2×), `larger`
//!    (3–5×), `very_large` (24–40×), `massive` (192–320×). What spills is not a
//!    fixed size: a column that overflows by a few characters and one that
//!    overflows into blobs stress completely different parts of the trade.
//! 3. **Mix** — 5/10/20/33/50/66/80/90/99/100 % of rows inline.
//!
//! # What is compared
//!
//! One string column, scanned end to end, reading every value.
//!
//! | Variant | Layout | Per-row read |
//! |---|---|---|
//! | `p_real` | today's `VariableColumn` | `gather_buffered` + `read_str` — the real code path |
//! | `p_hand` | data file + offsets file | hand-rolled mmap + slice — an *idealized* pointer baseline |
//! | `i1` | fixed `N+4` slot + overflow column | length prefix, branch on sentinel |
//! | `i4` | fixed `4N+4` slot + overflow column | same, with the worst-case-UTF-8 slot width #238 declares |
//! | `h1` / `h4` | fixed slot, no overflow | no branch — the hard form's read (p=100 only) |
//!
//! `p_hand` is the primary baseline on purpose. It skips the `Vec<(u64,u64)>`
//! that `gather_buffered` materializes, so it is *faster* than what ships — which
//! makes it the conservative comparison. A mechanism that only wins against
//! `p_real` would be beating an implementation artifact, not the design.
//!
//! `i1` vs `i4` isolates read amplification from the mechanism: `4N` is what a
//! slot declared in *characters* must reserve, `N` is what all-ASCII data
//! actually occupies. If the two diverge, the chars-vs-bytes choice in #238 is
//! load-bearing rather than cosmetic.
//!
//! # Method (epic #167's, reused)
//!
//! Paired A/B — every variant sees the same values, on the same machine, inside
//! the same process. An in-run control (a `u64` fixed-column scan, which no
//! variant changes) is timed in every round to catch drift. Step vs slope is
//! separated by a row-count sweep. Warm page cache throughout; each variant gets
//! an untimed warm-up pass before its timed reps, and the median of `REPS` is
//! reported.
//!
//! Row count is **derived per config** from a fixed value-byte target, so a
//! `massive` panel does not try to materialize hundreds of gigabytes. Every
//! reported number is per-row or per-byte, so panels stay comparable.
//!
//! Every variant's checksum is compared against `p_real`'s. A mismatch aborts —
//! a timing comparison between variants that read different bytes is worthless.

use std::fs::{self, File, OpenOptions};
use std::hint::black_box;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use forgedb_storage::{FixedColumn, VariableColumn};
use memmap2::Mmap;

/// Timed repetitions per variant per config; the median is reported.
const REPS: usize = 7;

/// Marks a slot whose value did not fit inline and lives in the overflow column.
/// `u32::MAX` cannot collide with a real length: a slot only ever holds a value
/// of at most `slot_bytes - 4`.
const OVERFLOW: u32 = u32::MAX;

/// Approximate total *value* bytes per config. Row count is derived from this so
/// the `massive` panels stay finite; timings are reported per row and per byte.
const TARGET_VALUE_BYTES: u64 = 128 << 20;

/// Row-count bounds around that target: enough rows for the per-row cost to be
/// resolvable, few enough that a huge-value config still fits on disk.
const MIN_ROWS: usize = 2_000;
const MAX_ROWS: usize = 200_000;

/// Hard ceiling on a single value, so `huge` × `massive` cannot run away.
const MAX_VALUE_BYTES: usize = 1 << 20;

// ---------------------------------------------------------------------------
// Axes
// ---------------------------------------------------------------------------

/// An inline capacity, in characters — the author-declared `N` in `string(N)`.
struct Capacity {
    label: &'static str,
    n: usize,
}

const CAPACITIES: &[Capacity] = &[
    Capacity { label: "tiny", n: 4 },
    Capacity { label: "small", n: 16 },
    Capacity { label: "modest", n: 64 },
    Capacity { label: "large", n: 256 },
    Capacity { label: "huge", n: 1024 },
];

/// How far past the inline capacity an overflowing value goes, as a multiple of
/// N. Overflow length is uniform in `[lo·N, hi·N]`, capped at [`MAX_VALUE_BYTES`].
struct Overflow {
    label: &'static str,
    lo: usize,
    hi: usize,
}

const OVERFLOWS: &[Overflow] = &[
    Overflow { label: "just_over", lo: 1, hi: 2 },
    Overflow { label: "larger", lo: 3, hi: 5 },
    Overflow { label: "very_large", lo: 24, hi: 40 },
    Overflow { label: "massive", lo: 192, hi: 320 },
];

const MIXES: &[u32] = &[5, 10, 20, 33, 50, 66, 80, 90, 99, 100];

// ---------------------------------------------------------------------------
// Data
// ---------------------------------------------------------------------------

/// xorshift64*, so a run is reproducible from its seed without a `rand` dep.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
}

/// Mean value length for a config, used to derive the row count before any data
/// is generated.
fn mean_len(n: usize, ovf: &Overflow, pct_inline: u32) -> f64 {
    let short = (1.0 + n as f64) / 2.0;
    let long =
        ((ovf.lo * n).max(n + 1) as f64 + (ovf.hi * n).min(MAX_VALUE_BYTES) as f64) / 2.0;
    let p = f64::from(pct_inline) / 100.0;
    p * short + (1.0 - p) * long
}

fn rows_for(n: usize, ovf: &Overflow, pct_inline: u32) -> usize {
    let est = (TARGET_VALUE_BYTES as f64 / mean_len(n, ovf, pct_inline)) as usize;
    est.clamp(MIN_ROWS, MAX_ROWS)
}

/// `rows` ASCII values, `pct_inline` percent of which fit within `n` chars.
///
/// The inline/overflow decision is made per row from the same stream that picks
/// lengths, so the two classes are **interleaved randomly** rather than
/// clustered. That is deliberate: a clustered column would let the branch
/// predictor learn the pattern and would understate the branch's cost at exactly
/// the mixes where it is worst.
fn generate(rows: usize, n: usize, ovf: &Overflow, pct_inline: u32, seed: u64) -> Vec<String> {
    let mut rng = Rng(seed);
    let alphabet = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let lo = (ovf.lo * n).max(n + 1).min(MAX_VALUE_BYTES);
    let hi = (ovf.hi * n).max(lo + 1).min(MAX_VALUE_BYTES.max(lo + 1));
    (0..rows)
        .map(|_| {
            let inline = rng.below(100) < u64::from(pct_inline);
            let len = if inline {
                1 + rng.below(n as u64) as usize
            } else {
                lo + rng.below((hi - lo) as u64) as usize
            };
            let mut s = String::with_capacity(len);
            for _ in 0..len {
                s.push(alphabet[rng.below(alphabet.len() as u64) as usize] as char);
            }
            s
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Layouts
// ---------------------------------------------------------------------------

fn build_pointer(dir: &Path, values: &[String]) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let mut col = VariableColumn::new(dir.join("v.data"), dir.join("v.offsets"))?;
    for v in values {
        col.append_string(v)?;
    }
    col.flush()
}

/// A fixed slot column plus an overflow variable column.
///
/// Slot layout: `[u32 length | payload…]`, where a length of [`OVERFLOW`] means
/// the payload's first 8 bytes are instead a `u64` row index into the overflow
/// column. This is #238 resolution 5's shape — the length is *stored*, never
/// recovered by scanning (which is why the issue's measurement 3 is moot).
struct Inline {
    slot_bytes: usize,
    /// True when no value overflowed, so the no-branch (hard-form) reader is valid.
    all_inline: bool,
}

fn build_inline(dir: &Path, values: &[String], n: usize, mult: usize) -> std::io::Result<Inline> {
    fs::create_dir_all(dir)?;
    let cap = n * mult;
    // A slot must be wide enough for whichever is larger: the inline payload, or
    // the overflow pointer that replaces it. So a *soft* declaration has a floor
    // the author does not control — `string(4)` cannot occupy 8 bytes, because a
    // spilled row still needs 4 (sentinel) + 8 (overflow index) = 12. The hard
    // form has no such floor: it never spills, so it never reserves the pointer.
    let slot_bytes = 4 + cap.max(8);

    let mut fixed = File::create(dir.join("i.fixed"))?;
    let mut ovf = VariableColumn::new(dir.join("o.data"), dir.join("o.offsets"))?;
    let mut ovf_rows: u64 = 0;
    let mut all_inline = true;
    let mut slot = vec![0u8; slot_bytes];

    for v in values {
        slot.fill(0);
        let bytes = v.as_bytes();
        if bytes.len() <= cap {
            slot[..4].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
            slot[4..4 + bytes.len()].copy_from_slice(bytes);
        } else {
            all_inline = false;
            slot[..4].copy_from_slice(&OVERFLOW.to_le_bytes());
            slot[4..12].copy_from_slice(&ovf_rows.to_le_bytes());
            ovf.append_string(v)?;
            ovf_rows += 1;
        }
        fixed.write_all(&slot)?;
    }
    fixed.sync_all()?;
    ovf.flush()?;
    Ok(Inline { slot_bytes, all_inline })
}

/// The in-run control: a `u64` column no variant under test touches. Its timing
/// must hold steady across rounds; if it drifts, the round's comparisons are not
/// trustworthy, and the drift is reported rather than averaged away.
fn build_control(dir: &Path, rows: usize) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let path = dir.join("control.col");
    let mut col = FixedColumn::new(path.clone(), 8)?;
    for i in 0..rows {
        col.append_u64(i as u64)?;
    }
    col.flush()?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// Readers — each returns a checksum so the variants can be proven equivalent
// ---------------------------------------------------------------------------

fn consume(acc: u64, s: &str) -> u64 {
    // Touches the length AND a byte of the payload, so neither the slice nor the
    // page fault behind it can be optimized away.
    acc.wrapping_mul(31)
        .wrapping_add(s.len() as u64)
        .wrapping_add(u64::from(s.as_bytes()[0]))
}

fn map(path: &Path) -> std::io::Result<Mmap> {
    let f = OpenOptions::new().read(true).open(path)?;
    // SAFETY: the bench owns these files for the duration of the run and nothing
    // writes to them while a mapping is live — the same append-only,
    // single-writer discipline `ColumnExport::Mapped` documents.
    unsafe { Mmap::map(&f) }
}

/// Today's shipping path: `gather_buffered` + zero-copy `read_str` (#224/#228).
fn scan_p_real(col: &VariableColumn, indices: &[usize]) -> std::io::Result<u64> {
    let buf = col.gather_buffered(indices)?;
    let mut acc = 0u64;
    for slot in 0..buf.len() {
        acc = consume(acc, buf.read_str(slot)?);
    }
    Ok(acc)
}

/// An idealized pointer baseline: both files mapped, offsets read in place, no
/// per-scan `Vec<(u64, u64)>`. Strictly cheaper than `p_real`, and therefore the
/// bar the inline design actually has to clear.
fn scan_p_hand(dir: &Path, rows: usize) -> std::io::Result<u64> {
    let data = map(&dir.join("v.data"))?;
    let offs = map(&dir.join("v.offsets"))?;
    let (data, offs) = (data.as_ref(), offs.as_ref());
    let mut acc = 0u64;
    for row in 0..rows {
        let e = row * 16;
        let off = u64::from_le_bytes(offs[e..e + 8].try_into().unwrap()) as usize;
        let len = u64::from_le_bytes(offs[e + 8..e + 16].try_into().unwrap()) as usize;
        acc = consume(acc, std::str::from_utf8(&data[off..off + len]).unwrap());
    }
    Ok(acc)
}

/// The soft `string(N)` read: length prefix, branch on the overflow sentinel.
fn scan_inline(dir: &Path, rows: usize, slot_bytes: usize) -> std::io::Result<u64> {
    let fixed = map(&dir.join("i.fixed"))?;
    let odata = map(&dir.join("o.data"))?;
    let ooffs = map(&dir.join("o.offsets"))?;
    let (fixed, odata, ooffs) = (fixed.as_ref(), odata.as_ref(), ooffs.as_ref());
    let mut acc = 0u64;
    for row in 0..rows {
        let base = row * slot_bytes;
        let len = u32::from_le_bytes(fixed[base..base + 4].try_into().unwrap());
        let s = if len == OVERFLOW {
            let idx = u64::from_le_bytes(fixed[base + 4..base + 12].try_into().unwrap()) as usize;
            let e = idx * 16;
            let off = u64::from_le_bytes(ooffs[e..e + 8].try_into().unwrap()) as usize;
            let olen = u64::from_le_bytes(ooffs[e + 8..e + 16].try_into().unwrap()) as usize;
            std::str::from_utf8(&odata[off..off + olen]).unwrap()
        } else {
            std::str::from_utf8(&fixed[base + 4..base + 4 + len as usize]).unwrap()
        };
        acc = consume(acc, s);
    }
    Ok(acc)
}

/// The hard `string(N!)` read — identical to [`scan_inline`] minus the branch and
/// minus the overflow mappings. Valid only when nothing overflowed.
fn scan_hard(dir: &Path, rows: usize, slot_bytes: usize) -> std::io::Result<u64> {
    let fixed = map(&dir.join("i.fixed"))?;
    let fixed = fixed.as_ref();
    let mut acc = 0u64;
    for row in 0..rows {
        let base = row * slot_bytes;
        let len = u32::from_le_bytes(fixed[base..base + 4].try_into().unwrap()) as usize;
        acc = consume(acc, std::str::from_utf8(&fixed[base + 4..base + 4 + len]).unwrap());
    }
    Ok(acc)
}

fn scan_control(path: &Path, rows: usize) -> std::io::Result<u64> {
    let col = FixedColumn::new(path.to_path_buf(), 8)?;
    let buf = col.gather_buffered(&(0..rows).collect::<Vec<_>>())?;
    let mut acc = 0u64;
    for slot in 0..buf.len() {
        acc = acc.wrapping_mul(31).wrapping_add(buf.read_u64(slot)?);
    }
    Ok(acc)
}

// ---------------------------------------------------------------------------
// Timing
// ---------------------------------------------------------------------------

/// One untimed warm-up pass, then [`REPS`] timed passes; returns the median
/// nanoseconds and the checksum every pass agreed on.
fn measure(mut f: impl FnMut() -> std::io::Result<u64>) -> std::io::Result<(f64, u64)> {
    let expect = f()?;
    let mut times = Vec::with_capacity(REPS);
    for _ in 0..REPS {
        let t = Instant::now();
        let got = black_box(f()?);
        times.push(t.elapsed().as_secs_f64() * 1e9);
        assert_eq!(got, expect, "checksum varied between repetitions");
    }
    times.sort_by(f64::total_cmp);
    Ok((times[REPS / 2], expect))
}

fn du(dir: &Path) -> std::io::Result<u64> {
    let mut total = 0;
    for entry in fs::read_dir(dir)? {
        total += entry?.metadata()?.len();
    }
    Ok(total)
}

#[allow(clippy::too_many_lines)]
fn main() -> std::io::Result<()> {
    let root = std::env::args()
        .nth(1)
        .map_or_else(|| PathBuf::from("target/inline261"), PathBuf::from);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root)?;

    let mut records: Vec<serde_json::Value> = Vec::new();
    let mut control_samples: Vec<f64> = Vec::new();

    let control_rows = 100_000usize;
    let control = build_control(&root.join("control"), control_rows)?;

    eprintln!(
        "# grid: {} capacities x {} overflow classes x {} mixes",
        CAPACITIES.len(),
        OVERFLOWS.len(),
        MIXES.len()
    );

    for cap in CAPACITIES {
        for ovf in OVERFLOWS {
            eprintln!("## N={} ({}), overflow={}", cap.n, cap.label, ovf.label);
            for &pct in MIXES {
                let rows = rows_for(cap.n, ovf, pct);
                let values = generate(rows, cap.n, ovf, pct, 0x5EED_0000 + u64::from(pct));
                let value_bytes: u64 = values.iter().map(|v| v.len() as u64).sum();
                let case = root.join(format!("n{}_{}_p{}", cap.n, ovf.label, pct));

                build_pointer(&case.join("p"), &values)?;
                let i1 = build_inline(&case.join("i1"), &values, cap.n, 1)?;
                let i4 = build_inline(&case.join("i4"), &values, cap.n, 4)?;

                // Control first, every round: drift shows up as a moving baseline.
                let (c_ns, _) = measure(|| scan_control(&control, control_rows))?;
                control_samples.push(c_ns / control_rows as f64);

                let col = VariableColumn::new(
                    case.join("p").join("v.data"),
                    case.join("p").join("v.offsets"),
                )?;
                let indices: Vec<usize> = (0..rows).collect();
                let (p_real_ns, truth) = measure(|| scan_p_real(&col, &indices))?;
                let (p_hand_ns, ck) = measure(|| scan_p_hand(&case.join("p"), rows))?;
                assert_eq!(ck, truth, "p_hand disagrees with the shipping path");
                let (i1_ns, ck) = measure(|| scan_inline(&case.join("i1"), rows, i1.slot_bytes))?;
                assert_eq!(ck, truth, "i1 disagrees with the shipping path");
                let (i4_ns, ck) = measure(|| scan_inline(&case.join("i4"), rows, i4.slot_bytes))?;
                assert_eq!(ck, truth, "i4 disagrees with the shipping path");

                let bytes_p = du(&case.join("p"))?;
                let bytes_i1 = du(&case.join("i1"))?;
                let bytes_i4 = du(&case.join("i4"))?;

                let mut push = |variant: &str, ns_total: f64, on_disk: u64| {
                    records.push(serde_json::json!({
                        "capacity": cap.label,
                        "n_chars": cap.n,
                        "overflow": ovf.label,
                        "pct_inline": pct,
                        "rows": rows,
                        "value_bytes": value_bytes,
                        "variant": variant,
                        "ns_total": ns_total,
                        "ns_per_row": ns_total / rows as f64,
                        "bytes_on_disk": on_disk,
                    }));
                };
                push("p_real", p_real_ns, bytes_p);
                push("p_hand", p_hand_ns, bytes_p);
                push("i1", i1_ns, bytes_i1);
                push("i4", i4_ns, bytes_i4);

                // The no-branch reader only exists when nothing overflowed — this
                // is measurement 2, the branch's own cost, data held identical.
                if i1.all_inline && i4.all_inline {
                    let (h1_ns, ck) =
                        measure(|| scan_hard(&case.join("i1"), rows, i1.slot_bytes))?;
                    assert_eq!(ck, truth, "h1 disagrees with the shipping path");
                    let (h4_ns, ck) =
                        measure(|| scan_hard(&case.join("i4"), rows, i4.slot_bytes))?;
                    assert_eq!(ck, truth, "h4 disagrees with the shipping path");
                    push("h1", h1_ns, bytes_i1);
                    push("h4", h4_ns, bytes_i4);
                }

                eprintln!(
                    "   p={pct:>3}% rows={rows:>6}  p_hand {:>8.1}  i1 {:>8.1}  i4 {:>8.1} ns/row   (p_real {:>8.1})",
                    p_hand_ns / rows as f64,
                    i1_ns / rows as f64,
                    i4_ns / rows as f64,
                    p_real_ns / rows as f64,
                );

                drop(col);
                let _ = fs::remove_dir_all(&case);
            }
        }
    }

    // -- step vs slope -------------------------------------------------------
    // A fixed per-scan cost (mapping, setup) and a per-row cost are separated by
    // how each moves with row count: a step amortizes away, a slope does not.
    eprintln!("# step vs slope — N=16, overflow=larger, p=50");
    let mut scale: Vec<serde_json::Value> = Vec::new();
    let ovf = &OVERFLOWS[1];
    for &rows in &[1_000usize, 10_000, 100_000, 1_000_000] {
        let values = generate(rows, 16, ovf, 50, 0xA11CE);
        let case = root.join(format!("scale_{rows}"));
        build_pointer(&case.join("p"), &values)?;
        let i1 = build_inline(&case.join("i1"), &values, 16, 1)?;
        let i4 = build_inline(&case.join("i4"), &values, 16, 4)?;

        let idx: Vec<usize> = (0..rows).collect();
        let col = VariableColumn::new(
            case.join("p").join("v.data"),
            case.join("p").join("v.offsets"),
        )?;
        let (p_real_ns, truth) = measure(|| scan_p_real(&col, &idx))?;
        let (p_hand_ns, ck) = measure(|| scan_p_hand(&case.join("p"), rows))?;
        assert_eq!(ck, truth);
        let (i1_ns, ck) = measure(|| scan_inline(&case.join("i1"), rows, i1.slot_bytes))?;
        assert_eq!(ck, truth);
        let (i4_ns, ck) = measure(|| scan_inline(&case.join("i4"), rows, i4.slot_bytes))?;
        assert_eq!(ck, truth);

        for (variant, ns_total) in [
            ("p_real", p_real_ns),
            ("p_hand", p_hand_ns),
            ("i1", i1_ns),
            ("i4", i4_ns),
        ] {
            scale.push(serde_json::json!({
                "rows": rows,
                "variant": variant,
                "ns_total": ns_total,
                "ns_per_row": ns_total / rows as f64,
            }));
        }
        eprintln!(
            "  rows={rows:>7}  p_hand {:>7.1}  i1 {:>7.1}  i4 {:>7.1} ns/row",
            p_hand_ns / rows as f64,
            i1_ns / rows as f64,
            i4_ns / rows as f64,
        );
        drop(col);
        let _ = fs::remove_dir_all(&case);
    }

    let control_min = control_samples.iter().copied().fold(f64::MAX, f64::min);
    let control_max = control_samples.iter().copied().fold(0.0, f64::max);
    let drift = (control_max - control_min) / control_min * 100.0;
    eprintln!(
        "# control drift across {} rounds: {drift:.1}%",
        control_samples.len()
    );

    let json = serde_json::json!({
        "experiment": 261,
        "reps": REPS,
        "target_value_bytes": TARGET_VALUE_BYTES,
        "row_bounds": [MIN_ROWS, MAX_ROWS],
        "max_value_bytes": MAX_VALUE_BYTES,
        "capacities": CAPACITIES.iter().map(|c| serde_json::json!({"label": c.label, "n": c.n})).collect::<Vec<_>>(),
        "overflows": OVERFLOWS.iter().map(|o| serde_json::json!({"label": o.label, "lo": o.lo, "hi": o.hi})).collect::<Vec<_>>(),
        "mixes": MIXES,
        "control": {
            "ns_per_row_min": control_min,
            "ns_per_row_max": control_max,
            "drift_pct": drift,
            "samples": control_samples,
        },
        "grid": records,
        "scale": scale,
    });
    println!("{}", serde_json::to_string_pretty(&json).unwrap());
    Ok(())
}
