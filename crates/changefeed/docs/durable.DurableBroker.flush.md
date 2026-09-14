Fsync the log file. Under [`FsyncPolicy::Never`] this is the only barrier;
under [`FsyncPolicy::Always`] every [`DurableBroker::record`] already did it.
