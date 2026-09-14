Rotate the WAL: archive the current file under a timestamped name and
start a fresh, empty WAL at the original path.

Returns the path of the archived WAL file.
