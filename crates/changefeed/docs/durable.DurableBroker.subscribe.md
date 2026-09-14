Subscribe to the live tail. The receiver observes every event recorded
*after* this call. Combine with [`read_from`](DurableBroker::read_from)
via [`catch_up_from`](DurableBroker::catch_up_from) for a gap-free resume.
