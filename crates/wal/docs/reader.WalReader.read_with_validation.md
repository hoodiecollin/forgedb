Decode the whole file without stopping at damage, returning the entries that
decoded and a [`CorruptionInfo`] for each offset that did not.

Where [`Self::read_all`] stops, this records the offset and the error message,
advances one byte, and tries again. One damaged region can therefore yield many
corruption records, one per byte until a record decodes, and entries that
decode after the damage are included.
