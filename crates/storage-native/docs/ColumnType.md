The logical type of the values a column stores, recorded in [`ColumnMetadata::column_type`]. This crate stores it and does not branch on it.

Every variant but [`ColumnType::String`] is fixed-width and lives in a [`FixedColumn`]; `String` lives in a [`VariableColumn`].
