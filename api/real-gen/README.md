# Compile-only real descriptions

The eight current descriptions and the three `again/` descriptions named in
`docs/apis/README.md` are the eleven probes. `first/` contains four historical
versions of the same providers; it is not four additional members of that
inventory. `inventory.json` identifies every page. The provenance test compares
each complete macro block byte for byte with that page.

Each module supplies inert, **nominally distinct** model structs with serde
implementations. Generic page models retain their type parameters. Helpers and
middlewares are inert; these probes establish type checking, not provider
schemas or runtime behavior. No request is sent. `tools/models.py` regenerates
only the stub declarations, never the descriptions.

Ten descriptions compile: github, riot, s3, stripe, slack, kubernetes,
elasticsearch, again_slack, again_telegram, again_kubernetes. Main Telegram is
kept verbatim in `cases/fail/telegram.rs`, with its compiler message. Its page
passes an optional Secret to `serde_json::json!` in set_webhook. Secret deliberately
does not implement Serialize (C2.4); docs/apis/README.md already records this
page fault. Its separate unpublished case crate fails for exactly E0277:

    cargo check --offline --manifest-path api/real-gen/cases/fail/Cargo.toml --target-dir api/target

Normal workspace checks do not enable the failing case. The Telegram module
retains its model stubs, so the case uses the same scaffolding as all other pages.

## Measurements

`tools/measure.py` prepares standalone clients of 100 and 500 endpoints only,
cycling through ordinary GETs, JSON POSTs, paged calls, HEAD status arms, Result
arms and body-entry templates. Dependencies use the existing workspace target
cache; each client has a distinct package name. It checks then builds each
client once and records `/usr/bin/time -v` output, elapsed time and exit status.
An explicitly ignored macro unit test records expansion time alone, excluding
lexing and output formatting. Run sequentially under the required memory scope:

    build/limited.sh python3 api/real-gen/tools/measure.py --run

All generated files and measurements stay under `api/target/p43-measure`.
