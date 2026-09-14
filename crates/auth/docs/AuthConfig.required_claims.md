Names of claims that must be present in the verified token, beyond `exp`,
which is always required.

Presence only, checked after signature verification; a missing one is
[`AuthError::MissingRequiredClaim`].
