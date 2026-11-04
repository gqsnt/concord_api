# Application over the shared SDK

An application crate over [`examples/sdk`](../sdk) (the Catalog and Ledger clients). It shows the
code an application writes between its own domain and two independent Concord clients. Every
rule here is the application's policy, not Concord's. The tests run against the loopback fake
services of [`examples/fixture`](../fixture), which prove nothing about any real service.

| Module | Shows | Requirement | Tests |
|---|---|---|---|
| `src/domain.rs` | `AccountId` and `Moment`, the application's own values; no service encoding, no service-tuned `Display` | D01 | `tests/adapters.rs` |
| `src/catalog_repr.rs`, `src/ledger_repr.rs` | each service's representation with its own typed error; `activity` / `entries` convert synchronously and return the generated `CatalogActivity` / `LedgerEntries`, so the caller keeps setters, `prepare()` and `send()`. Unrepresentable values (fractional second, year outside 0000–9999, sub-millisecond, before 1970, `u64` overflow) are refused before any request exists | D01, D02 | `tests/adapters.rs` |
| `src/report.rs` | `account_report`: each service attempted independently, one `Outcome` per service (`Rejected`, `Failed(concord::Error)`, `Succeeded`); no aggregate `?` | D02, D03 | `tests/adapters.rs` |
| `src/workflow.rs` | `Browser`: one tenant's listing with the application's featured tag as default; `unfiltered_page()` and `featured_page()` return the generated `CatalogTenantListingList` | W01 | `tests/workflow.rs` |
| `src/traversal.rs` | `traverse`: one bounded page walk for both services (`NonZeroUsize` bound, `Complete` or `Bounded` with a `Continuation`, resumption, `Interrupted` keeping partial records and the failed page's position); `catalog_items` / `ledger_entries` are the per-service adapters | W02 | `tests/traversal.rs` |

Wire facts the tests pin: account 42 is `/accounts/42/activity` for Catalog and
`account=acct-002a` for Ledger; 2026-01-15T10:00:00Z is `since=2026-01-15T10%3A00%3A00Z` (Concord
percent-encodes `:` in a query value; the decoded value is exact) and `from_ms=1768471200000`.

Policies chosen here, stated in each module's documentation:

- **Report.** Catalog then Ledger, one after the other; a rejection or failure in one never hides
  the other's answer. An application whose report is useless unless both parts exist could
  short-circuit instead.
- **Traversal.** On a failed page the walk stops and returns the error together with the records
  already collected in that call and the `from` value that refetches the failed page (`None` if it
  was the first page). The caller keeps the records and resumes, or discards them and restarts.
  Nothing is retried. A continuation belongs to the request template it came from.

```sh
cargo test -p concord-example-app --locked
```
