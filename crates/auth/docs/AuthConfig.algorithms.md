The `alg` values a token may carry; any other is
[`AuthError::AlgorithmNotAllowed`].

All entries must belong to one key family (RSA, EC or Ed): `jsonwebtoken`
rejects a list whose families differ from the selected key's, so a mixed list
fails every token with [`AuthError::Invalid`]. [`parse_algorithm`] builds
entries from names and admits only asymmetric algorithms.
