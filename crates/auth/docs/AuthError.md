A verification failure. [`AuthError::status_code`] maps each to the HTTP
status the middleware returns: `401` for an authentication failure, `403`
for a valid token whose tenant is not authorized for this process.
