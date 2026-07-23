# Riot API narrative

The Riot example is intentionally split into domain types, API declarations,
usage, and tests:

```text
riot/
  domain/   routing, accounts, matches, league, status, tournaments
  api/      Riot service and Data Dragon declarations
  usage.rs  named client construction and request composition
```

The [domain module](../../crates/concord-examples/src/riot/domain/mod.rs)
keeps remote JSON/data types independent from generated transport code. The
[API module](../../crates/concord-examples/src/riot/api/mod.rs) maps regional
routing, authentication placement, pagination, and typed responses. The
[usage module](../../crates/concord-examples/src/riot/usage.rs) shows how an
application composes the generated builders and scope-oriented methods.

This separation keeps declarations readable and lets domain tests compile
without duplicating an API. It also makes routing choices explicit: platform
routes and regional routes are distinct typed values rather than arbitrary
strings scattered through call sites.

No example or CI test contacts Riot. The
[contract test](../../crates/concord-examples/tests/riot_contract.rs) imports
the real modules and checks construction. Runtime behavior uses local scripted
support in the
[local runtime test](../../crates/concord-examples/tests/local_runtime.rs).

For smaller pieces, read [Regional APIs](regional-apis.md),
[Authentication](../guide/04-authentication.md), and
[Pagination and rate limits](../guide/06-pagination-and-rate-limits.md).
