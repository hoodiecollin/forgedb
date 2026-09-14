When the broker fsyncs its log.

Two settings only: sync after every [`DurableBroker::record`], or never sync
automatically and leave the barrier to [`DurableBroker::flush`].
