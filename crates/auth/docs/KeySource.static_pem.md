Build a [`KeySource::StaticPem`] holding a single [`StaticKey`] with the given
`kid`, PEM text and algorithm.

The PEM is not validated here; a bad one surfaces as [`AuthError::Key`] at
verification.
