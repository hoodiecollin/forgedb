Error returned by [`CoordinatorClient`] requests.

Implements `Display` and `From<io::Error>`. Only the `Io` variant poisons the connection; the other three describe a well-formed reply from the coordinator.
