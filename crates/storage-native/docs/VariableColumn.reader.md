Open a read-only [`VariableColumnReader`] over this column's data and offsets files through independently cloned descriptors (`try_clone`).

The reader derives its length from the offsets file on every access, so this `&mut self` writer can keep appending while any number of readers read concurrently without a lock. See [`FixedColumnReader`] for the model.
