Append `value` as 8 little-endian IEEE 754 bytes at the end of the file and advance the row count by one. Writes exactly 8 bytes, so it is only correct on a column whose `value_size` is 8.
