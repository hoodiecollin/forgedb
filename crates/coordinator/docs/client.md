Client side of the coordinator socket protocol, linked by generated coordinated writers.

[`client::CoordinatorClient`] owns one Unix-socket connection and exposes the two protocol calls a writer needs, [`client::CoordinatorClient::request_turn`] and [`client::CoordinatorClient::committed`]. It knows nothing about models, fields or the schema; every key and row it carries is opaque bytes. Failures surface as [`client::ClientError`].
