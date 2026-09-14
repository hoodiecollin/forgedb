Copy the physical rows at `indices` into one contiguous owned buffer, in
the order given (`indices.len() * value_size` bytes).

This is the class-1 columnar-read primitive behind the language bindings'
Arrow / columnar-export **gather** path — the fallback taken whenever the
live selection is *not* an aliasable dense prefix (update-heavy or
tombstoned tables) and a contiguous copy of exactly the live rows is
needed. It is schema-agnostic: `indices` are opaque physical row
positions the caller computed (generated code derives the live set from
`id_to_row` + tombstone liveness, exactly as `all()` / `all_at()` do
today); `gather` never reads a field name, type, or schema.

# Performance (#221)

For a selection of at least [`GATHER_MMAP_MIN_ROWS`] rows this maps the
spanned region `[min(indices), max(indices)]` **once** and copies out of
that mapping in contiguous runs, rather than issuing one `read_exact_at`
per index. Live selections under append-only churn are long runs broken
by the holes superseded rows leave behind, so runs are typically far
longer than one row and the copy collapses to a handful of `memcpy`s.

The mapping is held only for the duration of the call and the returned
buffer is owned, so this relies on a strictly weaker form of the
aliasing invariant documented on [`ColumnExport`]: the spanned rows are
committed (`< row_count`, so within the file's length) and no concurrent
truncation runs, which the single-writer discipline already guarantees.

Known-unmeasured case: a *sparse* selection spanning a very large file
faults in one page per isolated row, reading more bytes than the
per-row path would. It still trades syscalls for faults plus readahead,
and real live sets stay dense (amplification is bounded by the generated
compaction ceiling), but it has not been benchmarked.

# Errors

Returns `Err(InvalidInput)` if any index is `>= self.len()`.
