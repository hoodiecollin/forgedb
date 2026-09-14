Read and parse the manifest at `path`.

Returns the I/O error if the file cannot be read, or an `InvalidData` error wrapping the JSON parse failure. Fields absent from the file take their defaults and unknown keys are ignored.
