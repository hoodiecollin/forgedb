Why an RFC 3339 string could not be read as a [`Timestamp`].

Deliberately opaque: this reaches users through a 400/422 on a REST path
segment or query parameter, where the useful information is "that is not an
RFC 3339 instant", not which of fourteen sub-fields was malformed.
