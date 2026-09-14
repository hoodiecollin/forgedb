Publish a change signal to all current subscribers.

Best-effort and non-blocking: returns the number of subscribers the event
reached (`0` when there are none — not an error). A writer never waits on a
subscriber.
