# Package namespace observation — 2026-09-04

**Claim state:** Observed at the stated instant; registry state may change.

**Query time:** 2026-09-04T10:45:42Z
**Host:** Windows x86_64
**Cargo:** 1.97.1 (`c980f4866`, 2026-06-30)
**Rustc:** 1.97.1 (`8bab26f4f`, 2026-07-14)

## Official Cargo results

`cargo search concord --limit 10` returned exact package `concord` at version
2.5.15 with description “A terminal user interface client for Discord.”

`cargo info concord --registry crates-io` resolved the unrelated registry
package, reported latest version 2.5.15, GPL-3.0-only, and repository/homepage
`https://github.com/chojs23/concord`.

`cargo info` for each candidate below returned “could not find ... in registry”:

- `concord-api`
- `concord-api-core`
- `concord-api-macros`
- `concord-api-extras`

An unauthenticated `cargo owner --list` could not supply evidence because this
Cargo configuration required a registry token. No token was added or exposed.

## Official crates.io API results

Read-only GET requests to `https://crates.io/api/v1/crates/<name>` corroborated:

| Name | Result at query time | Owners result |
|---|---|---|
| `concord` | found; newest 2.5.15; unrelated Discord TUI | `chojs23` |
| `concord-api` | HTTP 404 | no package record |
| `concord-api-core` | HTTP 404 | no package record |
| `concord-api-macros` | HTTP 404 | no package record |
| `concord-api-extras` | HTTP 404 | no package record |

## Evidence boundary

The current façade name is not a truthful or publishable identity for this
project. A 404 or Cargo “not found” response does not reserve a name and does
not establish project ownership. The candidate family remains unapproved and
unsecured until an authorized owner decision and a later registry check.
