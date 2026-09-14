# forgedb-auth

Schema-agnostic, **verify-only** JWT authentication for ForgeDB generated
servers, plus a single tenant cross-check.

This is Class-1 substrate — the same class as `forgedb-changefeed`. It knows
*less* about a schema than `forgedb-storage`
does: it decodes no field, dispatches on no model name, reconstructs no
schema surface. Its entire vocabulary is "verify a token", "extract a
configured claim", "compare two opaque strings", "carry a principal". It
never reads a `.forge` schema and holds no notion of models, rows, columns,
or policies — deliberately. The instant this crate grows a per-model map or
a "role X may read model Y" decision, it has crossed into the runtime engine
the ForgeDB identity forbids; keep that seam bright.

## What it does

Given a raw bearer token and a configured [`AuthConfig`] + a single
`process_tenant` string, [`Authenticator::authenticate`]:

1. rejects any signature algorithm outside the configured allowlist
   (defeats `alg: none` and the HS/RS confusion downgrade),
2. verifies the signature against an asymmetric key selected by the token's
   `kid` (static PEM or a JWKS document),
3. validates `exp`/`nbf` (with skew), `iss`, `aud`, and required claims,
4. extracts the configured tenant claim and **cross-checks it against the
   process's tenant** — a plain string equality; a mismatch is a 403,
5. returns an opaque [`Principal`] (subject, tenant, roles, raw claims).

It never *issues* tokens or stores users — bring your own IdP.
