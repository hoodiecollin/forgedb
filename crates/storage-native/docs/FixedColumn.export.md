Export the physical rows at `indices` as a [`ColumnExport`], taking the
**zero-copy `mmap` alias** fast path when `indices` is the contiguous
dense prefix `[0, n)` and falling back to [`gather`](Self::gather)
(an owned copy) otherwise. The returned buffer is identical bytes either
way (`indices.len() * value_size`), so the FFI / Arrow consumer is
alias-or-gather transparent.

This is the class-1 columnar-read primitive the language bindings' Arrow
export links: `indices` are opaque physical row positions the caller
(generated code) computed from the live set; `export` reads no field
name, type, or schema — it only inspects the indices' shape to decide
whether they alias a prefix.

# Errors

Returns `Err(InvalidInput)` if any index is `>= self.len()`, or an I/O
error if the `mmap` of the aliasable prefix fails.
