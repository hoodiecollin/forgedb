Mark the coordinator as shutting down and notify the condvar that turn waiters block on.

Also opens one connection to the socket. The flag is only consulted when the accept loop in [`Self::run`] hits an error, so this call does not by itself make `run` return, and handler threads already serving a client are unaffected.
