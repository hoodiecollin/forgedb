Open a read-only, positionally-reading view over this column's file
(#56 Direction B).  The returned [`FixedColumnReader`] shares the file
via an independent (`try_clone`d) descriptor, so a single `&mut self`
writer can keep appending while many `&self` readers read concurrently
without a lock.  See [`FixedColumnReader`] for the full model.
