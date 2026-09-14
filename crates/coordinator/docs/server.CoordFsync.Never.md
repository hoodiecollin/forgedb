Never fsync in the commit path — rely on the OS to flush the log. A
coordinator crash rewinds the replication tail (no client data lost).
