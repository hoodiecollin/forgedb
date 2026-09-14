Open a read-only [`FixedColumnReader`] over this column's file through an independently cloned descriptor (`try_clone`).

The reader derives its length from the file on every access, so this `&mut self` writer can keep appending while any number of readers read concurrently without a lock. See [`FixedColumnReader`] for the model.
