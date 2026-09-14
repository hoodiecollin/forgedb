The failure of an RFC 3339 parse ([`Timestamp::from_rfc3339`]).

Deliberately carries no detail: the useful message at a request boundary is that the input
is not an RFC 3339 instant, not which sub-field was malformed.
