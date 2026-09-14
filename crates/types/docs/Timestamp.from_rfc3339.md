Parses an RFC 3339 instant. Lenient about everything the rendering fixes:
any number of fractional digits (sub-microsecond digits are **truncated**,
since the stored unit is the finest thing the value can mean), a lowercase
`t`/`z` separator, and any numeric offset (`±HH:MM` / `±HHMM` / `±HH`).

An offset is applied, not recorded: the stored value is an instant.
