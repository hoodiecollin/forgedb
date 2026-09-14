Errors from opening and running a [`Coordinator`].

Implements `std::error::Error` via `thiserror`, with `From<io::Error>` for the `Io` variant.
