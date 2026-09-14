Byte usage of one column, as measured by [`crate::StatsCollector`].

Row counts come from the model's tombstone bitmap and are the same for every column of a
model. For a [`ColumnType::Fixed`] column the byte figures assume a row width of file size
divided by row count. For a [`ColumnType::Variable`] column `total_bytes` includes the
offsets file while `used_bytes` and `dead_bytes` count payload lengths only, so the two do
not sum to `total_bytes`.
