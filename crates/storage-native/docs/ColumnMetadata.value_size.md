Bytes per row for a fixed column; `0` for a variable column, whose widths live in its offsets file. Missing from the file means `0`.

Lets a reader compute a fixed column's committed length as `row_count * value_size`.
