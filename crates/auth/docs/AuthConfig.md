Verification policy for one [`Authenticator`].

`Default` allows `RS256` only, checks no issuer or audience, reads the tenant
from a claim named `tenant`, allows 60 seconds of clock skew and requires no
extra claims. The crate reads none of this itself; the caller supplies it from
deployment configuration.
