The whole `value_size`-wide slot at `slot`, **borrowed** (#238).

The zero-copy counterpart of [`Self::read_bytes`], which is exactly this
plus a `.to_vec()`; both stay, because a caller that wants an owned buffer
should not have to write the copy itself.

Borrowing is sound here and nowhere else on the fixed path: this type owns
its bytes (a gathered `Vec` or an `Mmap` alias of the dense prefix), so the
`&self` lifetime is the buffer's. [`FixedColumn`] and [`FixedColumnReader`]
read through the file on every access and have nothing to lend — the same
split that put [`BufferedVariableColumn::read_str`] on the buffered tier
only (#224).
