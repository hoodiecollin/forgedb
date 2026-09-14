Open a read-only, positionally-reading view over this column's data +
offsets files (#56 Direction B).  The returned [`VariableColumnReader`]
shares both files via independent (`try_clone`d) descriptors, so a single
`&mut self` writer can keep appending while many `&self` readers read
concurrently without a lock.  See [`FixedColumnReader`] for the model.
