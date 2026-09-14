The token verified, but its tenant claim is not the tenant this process serves.

The only variant whose [`AuthError::status_code`] is `403`: the caller is
authenticated, just not for this process.
