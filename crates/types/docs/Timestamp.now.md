The current system time, in microseconds.

Saturates at `i64::MAX` if the count does not fit, and yields `0` if the system clock reads earlier than the epoch.
