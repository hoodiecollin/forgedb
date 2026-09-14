Issue the single device-cache barrier for a checkpoint (#153).  Barriers
are device-wide, so one call on the data file covers the offsets file too.
