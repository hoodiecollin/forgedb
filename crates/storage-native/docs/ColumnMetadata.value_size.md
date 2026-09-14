Bytes per row for a fixed column; `0` for a variable column (its width
lives in the offsets file). Lets backup compute a fixed column's exact
committed length as `row_count * value_size` without guessing.
