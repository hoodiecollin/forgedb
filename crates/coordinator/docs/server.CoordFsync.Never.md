Never flush in the commit path; the OS writes the log back on its own schedule, and a crash can lose the unflushed tail.
