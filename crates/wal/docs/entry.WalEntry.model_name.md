Routing tag stored verbatim in the record header. Its length is written as a
`u16`, so a name longer than 65 535 UTF-8 bytes is not rejected but does not
round-trip through [`Self::to_bytes`] and [`Self::from_bytes`].
