Whether [`Self::start`] has been called without a subsequent [`Self::stop`].

Reflects the requested state, not whether the thread has exited or a pass is executing;
use [`Self::status`] for the latter.
