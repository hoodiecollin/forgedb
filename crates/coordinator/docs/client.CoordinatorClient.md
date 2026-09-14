Schema-agnostic client for one coordinator connection.

Thread-safe: the inner stream is protected by a `Mutex`.  All messages are
synchronous (blocking I/O); the coordinator protocol is strictly sequential
— one request, one reply — so no interleaving can occur.
