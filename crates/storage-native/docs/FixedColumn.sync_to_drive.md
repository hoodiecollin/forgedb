Push this column's data to the drive cache **without** a device barrier
(#153).  Pair with a single [`FixedColumn::barrier`] (on any column of the
same device) to make a whole checkpoint's columns durable with ONE
barrier instead of N.  See [`fsync_to_drive`].
