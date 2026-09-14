Run the coordinator: bind the Unix socket, accept connections, dispatch
each to a handler thread.  Blocks until the process is killed or
`shutdown()` is called.
