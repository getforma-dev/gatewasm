# Security

## Reporting

Email **victor@getforma.dev** with `[security]` in the subject. Please do not
open a public issue for an exploitable defect.

Useful in a report: the affected endpoint or component, what an attacker gains,
and the smallest input that reproduces it. A failing test or a `curl` is worth
more than a description.

## Scope

`gatewasm` is a multi-tenant authentication platform, so anything that crosses
a tenant boundary, forges or leaks a token, or takes the shared process down is
in scope regardless of how small the trigger looks.

Two real examples from this codebase, both fixed, both reachable with one
request and neither obviously dangerous on inspection:

- `POST /auth/token` logged a 12-byte prefix of the authorization code before
  validating anything. A mixed-width UTF-8 value put that index inside a
  character, and `panic = "abort"` turned the panic into an outage for every
  tenant. `{"code":"aéééééé"}` was enough.
- A tenant's branding CSS escaped the `<style>` block on the unauthenticated
  hosted login page, because the escape matched two literal spellings of the
  closing tag and HTML end-tag matching is case-insensitive.

## Practices

The release profile keeps `panic = "abort"`: on an auth server, a panic
part-way through a security state machine leaves invariants undefined, and
continuing to serve is the more dangerous choice. It is paired with
`#![warn(clippy::string_slice)]`, whose baseline is zero, because that lint is
what surfaced the token-endpoint abort above.

Tests are expected to be able to fail. Several here could not — asserting a
case-sensitive literal against a mixed-case payload, or exercising a handler the
request never reached. When fixing a security defect, break the fix on purpose
and confirm the test goes red before trusting it.
