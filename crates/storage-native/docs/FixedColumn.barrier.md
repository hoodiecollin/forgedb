Issue the single device-cache barrier for a checkpoint (#153): flushes the
drive's write cache to permanent media, making durable every column
previously `sync_to_drive`d on the same device.  See [`device_barrier`].
