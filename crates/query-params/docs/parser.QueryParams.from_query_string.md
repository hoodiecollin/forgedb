Decodes a URL-encoded query string and parses it with [`Self::from_map`].

The input is the part after `?`, without the `?` itself; percent-encoding and `+` are decoded by `serde_urlencoded`. A repeated key keeps its last value, and an empty string yields the default parameters. The only error is a `serde_urlencoded` decode failure; an unparseable `limit`, `offset` or `order` is never an error, it falls back to its default.

```text
status=active&age=30&sort=name&order=desc&limit=20&offset=40
```
