Publish a change signal to every current subscriber.

Non-blocking and best-effort: returns the number of subscribers the event was
delivered to, `0` when there are none (not an error). A writer never waits on
a subscriber.
