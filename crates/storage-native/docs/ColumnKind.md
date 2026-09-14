Whether a column is fixed-width (one file, positional I/O) or variable-length (a data file indexed by an offsets file).

A physical-layout fact only, never a schema semantic; it lets a schema-blind reader bound each column's files without guessing.
