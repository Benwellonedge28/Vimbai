# vimbai_sso

Enterprise single sign-on: OIDC token verification, SAML assertion verification,
hardened sessions, and messages in the caller's language. Safe Rust
(`#![forbid(unsafe_code)]`), no globals: one `SsoStore` per deployment, owned by
its caller.

## What it verifies

* **Signature, always.** RS256/384/512, PS256/384/512, ES256, HS256/384/512.
  `alg: none` never parses, so it can never verify. The algorithm is taken from
  the organisation's policy, not from the token header: an RS256 key cannot be
  replayed as an HMAC secret.
* **Key selection by `kid`,** with a single-key fallback only when exactly one
  key could have signed. An unknown `kid` returns `unknown_signing_key`, which is
  the signal to refetch the key set after a rotation.
* **Claims.** `iss`, `aud`, `exp`, `nbf`, `iat` with a configurable leeway,
  optional nonce binding, verified-email requirement, email-domain allowlist, and
  `groups`/`roles` mapped to Vimbai roles.
* **SAML.** The assertion is parsed (issuer, NameID, conditions, audience
  restriction, attributes) and the RSA-SHA256 signature is checked over the exact
  bytes received. Conditions, audience, age and email domain are enforced.

## Sessions

Absolute lifetime plus an idle window (8 hours / 30 minutes by default; eight
minutes and ten for the `strict` policy), rotation on use so a stolen token has
one use, a per-user session cap that evicts the oldest, optional address binding,
revocation by token or by user, and an audit trail that records the reason a
session ended (`session_expired`, `session_idle`, `session_ip_mismatch`,
`session_evicted`) alongside authentication rejections and their codes.

The caller supplies the timestamps and the session entropy, so behaviour is
reproducible in tests and this crate never reads a clock or a random source.

## International

Errors carry a stable machine code plus a message negotiated from
`Accept-Language` (q-values honoured) or the organisation's default locale.
Catalogue: `en`, `sn`, `fr`, `pt`, `es`, `de`, `ar`, `zh`, falling back
`language-REGION` -> `language` -> `en`. A session records the person's own
`locale` and `zoneinfo` from the provider claim, and the provider's `region`
(`ZW`, `EU`, ...) so downstream services know which rulebook applies.

## Boundaries, stated plainly

* The caller fetches discovery documents and key sets. Nothing here performs
  HTTP, so a token is never verified against a URL an attacker supplied.
* An enveloped XML signature inside a larger SAML document needs
  canonicalisation, which is not implemented: supply the assertion element.
* ES384, ES512 and EdDSA are refused rather than half-supported.
