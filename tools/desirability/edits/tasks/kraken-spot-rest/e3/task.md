# kraken-spot-rest / e3: optional query value on the list operation

**Provider changelog.** The asset-pair list (`GET /0/public/AssetPairs`) accepts an optional
`country_code` query value (for example `GB:LND` or `US:TX`) that restricts the result to the
pairs available in that country or region. Unset, every pair is listed, as before. Nothing else
changed.

Public names the hidden check uses:
- `kraken.public().pairs().set_country_code(code)` sends `country_code=<code>`
  (`code`: `&str` or `String`), alone or next to `pair`.
