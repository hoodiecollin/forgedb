# Experiment #261 — inline `string(N)` slot vs pointer indirection

Gates the **soft** `string(N)` form of #238: a value that fits in a declared
inline slot is stored there, and one that does not spills to a variable-length
overflow column. The spill path is the whole cost — a discriminant, a per-row
branch in the read loop, and a second data file. This measures whether the
inline hit pays for it.

The **hard** `string(N!)` form is not on trial. It is justified independently
(it is what makes a `Copy` key possible, #252) and per #238 resolution 6 it
*deletes* the branch rather than adding one. A negative result here narrows #238;
it sinks nothing.

## Reproduce

```bash
make -C ../../.. experiment-261     # or, from this directory:
cargo run --release -- target/data > results/raw.json
bun plot.ts                          # → results/grid.svg, results/summary.svg
bun svg2png.ts results/grid.svg results/grid.png
bun svg2png.ts results/summary.svg results/summary.png
```

The crate is detached from both the root workspace and `benchmarks/` — it
measures the storage substrate directly, so it must not drag in the
comparative-DB dependencies or `benchmarks/gen/database.rs` (which tracks an
unrelated schema, and currently does not compile against the post-#187
`Manifest`).

## The grid

Three axes, because the answer plausibly depends on all three and a single curve
would hide it:

| Axis | Values |
|---|---|
| Inline capacity `N` | `tiny` 4, `small` 16, `modest` 64, `large` 256, `huge` 1024 chars |
| Overflow length, relative to `N` | `just_over` 1–2×, `larger` 3–5×, `very_large` 24–40×, `massive` 192–320× |
| Mix | 5 / 10 / 20 / 33 / 50 / 66 / 80 / 90 / 99 / 100 % of rows inline |

Row count is derived per config from a fixed value-byte target, so a
`massive` × `huge` panel does not try to materialize hundreds of gigabytes.
Every reported number is per-row, so panels stay comparable.

## Variants

| Variant | Layout | Per-row read |
|---|---|---|
| `p_real` | today's `VariableColumn` | `gather_buffered` + `read_str` — the real shipping path (#224/#228) |
| `p_hand` | data file + offsets file | hand-rolled mmap + slice — an *idealized* pointer baseline |
| `i1` | fixed `N+4` slot + overflow column | length prefix, branch on the overflow sentinel |
| `i4` | fixed `4N+4` slot + overflow column | same, at the worst-case-UTF-8 slot width `string(N)` must reserve |
| `h1` / `h4` | fixed slot, no overflow column | no branch — the hard form's read (100% inline only) |

`p_hand` is the primary baseline **on purpose**. It skips the `Vec<(u64,u64)>`
that `gather_buffered` materializes per scan, so it is strictly faster than what
ships today. A mechanism that only beats `p_real` would be beating an
implementation artifact rather than the design — and that artifact is separately
fixable without any new column kind.

`i1` vs `i4` separates the mechanism from read amplification. `4N` is what a slot
declared in *characters* must reserve for worst-case UTF-8; `N` is what all-ASCII
data actually occupies. If the two diverge, the chars-vs-bytes choice in #238 is
load-bearing rather than cosmetic.

### The slot floor

A slot must be wide enough for whichever is larger — the inline payload, or the
overflow pointer that replaces it. So a soft declaration has a floor the author
does not control: `string(4)` cannot occupy 8 bytes, because a spilled row still
needs 4 (sentinel) + 8 (overflow index) = 12. The harness encodes this. The hard
form has no such floor, because it never spills and so never reserves the
pointer.

## Method

Epic #167's, reused — it decided against a second storage model on measured
grounds, and its method is the durable output:

- **Paired A/B** — every variant sees the same values, on the same machine,
  inside the same process.
- **An in-run control** — a `u64` fixed-column scan that no variant changes,
  timed every round. Reported as drift; if it moves, the round's comparisons are
  not trustworthy.
- **Step vs slope** — a row-count sweep separates a fixed per-scan cost
  (mapping, setup) from a per-row cost. A step amortizes away; a slope does not.

Warm page cache throughout. Each variant gets an untimed warm-up pass, then the
median of `REPS` timed passes. Report absolute numbers and the crossover, never a
ratio at one size.

Every variant's checksum is compared against `p_real`'s and a mismatch aborts the
run — a timing comparison between variants that read different bytes is
worthless.

## Output

`results/raw.json` holds every measurement. `results/grid.svg` plots ns/row
against mix, one panel per (capacity × overflow class). `results/summary.svg`
reduces that to the crossover per cell, the storage amplification, and the
step-vs-slope panel. The verdict lives on the issue, not here.
