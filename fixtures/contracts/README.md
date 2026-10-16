# V3 contract recordings

These committed V3 recordings are exact contract oracles. Changes require an
explicit contract decision; source cleanup must preserve their bytes.

- `v3_riot_recording.json` records all 46 Riot endpoints in two origin contexts.
  The examples recording suite checks the full output, schema, identity, auth
  descriptors and rate-limit sharing.
- `v3_catalog_recording.json` supplies the Catalog rate-key oracle used by core,
  facade and packaged downstream tests. The canonical Catalog application and
  full generated interface are tested in `crates/concord-examples/tests/catalog_v3.rs`.

The source tests compare current V3 behavior directly with these recordings.
