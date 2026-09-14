Append `value` as 4 little-endian bytes at the end of the file and advance the row count by one. Writes exactly 4 bytes, so it is only correct on a column whose `value_size` is 4.
