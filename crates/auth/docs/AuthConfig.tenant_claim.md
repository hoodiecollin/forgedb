Name of the claim carrying the tenant identity, such as `tenant` or `org`.

Its value must be a JSON string; an absent or non-string value is
[`AuthError::MissingTenantClaim`]. It is compared with the process tenant by
exact string equality.
