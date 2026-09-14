Signal the background thread to stop.

Does not block; use `Drop` (or explicitly drop this struct) to wait for
the thread to fully exit.
