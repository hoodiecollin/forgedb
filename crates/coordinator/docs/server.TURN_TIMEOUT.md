How long a granted turn may remain un-committed before the coordinator
reclaims it.  A client that crashes or hangs holding a turn loses it after
this deadline, un-wedging all other writers.
