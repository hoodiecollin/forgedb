The LSN of the last commit this client acknowledged, or `0` if it has not committed since connecting.

Updated only by a successful [`Self::committed`]; [`Self::reconnect`] does not reset it. Generated writers pass it as `snapshot_lsn` on the next [`Self::request_turn`].
