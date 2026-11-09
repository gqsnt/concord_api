# Glue inventory (P3.1)

Produced by `python3 -I tools/glue/classify.py --repo . --md v4/next/GLUE.md` on `main` (`4222656b`).
What the labels mean, the rules, and the measured accuracy (independent review: 15% top-level error
per line after one fix round, errors both ways) are in `tools/glue/README.md`. Use differences between
commits, not absolute totals.

Commit `4222656b`. Non-blank lines of `examples/*-sdk/src/**/*.rs`.

## Totals

| Label | Lines | Share of SDK source |
|---|---|---|
| glue | 21,155 | 27.5% |
| glue:error | 11,714 | 15.2% |
| glue:result | 2,397 | 3.1% |
| glue:operation | 44 | 0.1% |
| glue:redaction | 1,516 | 2.0% |
| glue:forwarding | 774 | 1.0% |
| glue:construction | 1,242 | 1.6% |
| glue:paging | 3,348 | 4.3% |
| glue:docs | 120 | 0.2% |
| model | 18,113 | 23.5% |
| behavior | 23,711 | 30.8% |
| other | 8,392 | 10.9% |
| declaration | 3,182 | 4.1% |
| test | 2,465 | 3.2% |

All non-blank lines: 77,018.

## Per SDK (sorted by glue)

| SDK | glue | error | result | operation | redaction | forwarding | construction | paging | docs | model | behavior | other | declaration | test |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| github-sdk | 1255 | 467 | 168 | 1 | 56 | 0 | 9 | 548 | 6 | 1091 | 494 | 882 | 243 | 129 |
| distribution-registry-sdk | 1180 | 605 | 238 | 0 | 49 | 24 | 31 | 212 | 21 | 404 | 1445 | 369 | 81 | 290 |
| amazon-s3-sdk | 1130 | 510 | 394 | 1 | 31 | 0 | 128 | 66 | 0 | 432 | 1247 | 266 | 161 | 152 |
| backblaze-b2-native-sdk | 986 | 624 | 155 | 0 | 70 | 29 | 49 | 59 | 0 | 404 | 866 | 110 | 54 | 53 |
| clickhouse-sdk | 969 | 433 | 273 | 0 | 51 | 60 | 146 | 0 | 6 | 284 | 822 | 216 | 53 | 293 |
| salesforce-sdk | 941 | 641 | 0 | 0 | 121 | 108 | 6 | 65 | 0 | 793 | 2369 | 690 | 123 | 548 |
| stripe-sdk | 743 | 324 | 60 | 0 | 57 | 0 | 7 | 283 | 12 | 1427 | 764 | 614 | 290 | 0 |
| gitlab-sdk | 719 | 349 | 103 | 0 | 10 | 0 | 0 | 247 | 10 | 452 | 275 | 216 | 53 | 46 |
| microsoft-graph-sdk | 563 | 181 | 8 | 0 | 29 | 30 | 110 | 204 | 1 | 212 | 143 | 104 | 20 | 0 |
| datadog-events-sdk | 555 | 437 | 0 | 2 | 8 | 17 | 54 | 27 | 10 | 560 | 187 | 106 | 51 | 0 |
| elasticsearch-sdk | 551 | 317 | 0 | 0 | 94 | 86 | 25 | 27 | 2 | 600 | 891 | 125 | 39 | 0 |
| quickbooks-online-sdk | 500 | 381 | 0 | 1 | 67 | 0 | 0 | 50 | 1 | 717 | 955 | 166 | 87 | 93 |
| kubernetes-sdk | 483 | 261 | 0 | 0 | 47 | 0 | 38 | 132 | 5 | 1091 | 373 | 414 | 163 | 150 |
| snowflake-sdk | 473 | 150 | 25 | 0 | 31 | 17 | 82 | 168 | 0 | 247 | 632 | 97 | 25 | 0 |
| spotify-web-api-sdk | 472 | 348 | 0 | 0 | 43 | 0 | 26 | 52 | 3 | 334 | 685 | 195 | 81 | 0 |
| cloudflare-zones-sdk | 453 | 270 | 94 | 3 | 0 | 0 | 0 | 86 | 0 | 283 | 693 | 365 | 72 | 153 |
| anthropic-messages-sdk | 438 | 374 | 35 | 0 | 28 | 0 | 0 | 0 | 1 | 409 | 570 | 119 | 44 | 0 |
| mastodon-sdk | 408 | 260 | 0 | 0 | 11 | 68 | 53 | 0 | 16 | 278 | 406 | 182 | 83 | 0 |
| discord-rest-sdk | 368 | 325 | 0 | 5 | 17 | 16 | 0 | 0 | 5 | 218 | 514 | 247 | 77 | 115 |
| paypal-sdk | 361 | 274 | 0 | 16 | 58 | 0 | 8 | 0 | 5 | 540 | 600 | 192 | 116 | 37 |
| slack-sdk | 354 | 181 | 106 | 0 | 13 | 0 | 36 | 17 | 1 | 144 | 107 | 38 | 23 | 32 |
| google-drive-sdk | 341 | 147 | 16 | 0 | 53 | 45 | 76 | 4 | 0 | 258 | 485 | 55 | 49 | 0 |
| shopify-sdk | 337 | 192 | 3 | 0 | 37 | 0 | 41 | 64 | 0 | 1194 | 913 | 514 | 52 | 0 |
| xero-accounting-sdk | 329 | 158 | 0 | 0 | 65 | 9 | 80 | 17 | 0 | 328 | 871 | 97 | 31 | 0 |
| zendesk-support-sdk | 326 | 254 | 0 | 0 | 21 | 0 | 0 | 51 | 0 | 328 | 344 | 154 | 60 | 0 |
| kraken-spot-rest-sdk | 306 | 189 | 31 | 5 | 33 | 48 | 0 | 0 | 0 | 323 | 284 | 85 | 38 | 0 |
| rest-countries-sdk | 304 | 276 | 0 | 0 | 0 | 0 | 0 | 23 | 5 | 309 | 200 | 171 | 54 | 0 |
| europe-pmc-sdk | 299 | 107 | 0 | 0 | 15 | 0 | 0 | 177 | 0 | 205 | 228 | 133 | 52 | 0 |
| dropbox-sdk | 298 | 132 | 65 | 0 | 23 | 15 | 25 | 38 | 0 | 330 | 91 | 42 | 25 | 34 |
| ntfy-sdk | 298 | 275 | 0 | 0 | 5 | 0 | 0 | 10 | 8 | 290 | 457 | 162 | 53 | 112 |
| digitalocean-sdk | 296 | 90 | 36 | 0 | 27 | 0 | 0 | 143 | 0 | 244 | 95 | 31 | 31 | 0 |
| twilio-sdk | 281 | 154 | 18 | 0 | 31 | 25 | 53 | 0 | 0 | 127 | 63 | 34 | 24 | 0 |
| mailgun-sdk | 271 | 131 | 46 | 0 | 32 | 0 | 0 | 62 | 0 | 165 | 460 | 97 | 35 | 0 |
| notion-sdk | 271 | 109 | 7 | 0 | 38 | 4 | 47 | 65 | 1 | 316 | 260 | 66 | 24 | 0 |
| sentry-sdk | 265 | 132 | 13 | 0 | 25 | 0 | 0 | 95 | 0 | 110 | 455 | 97 | 46 | 0 |
| usgs-earthquake-catalog-sdk | 256 | 119 | 62 | 0 | 30 | 0 | 0 | 45 | 0 | 290 | 271 | 88 | 43 | 0 |
| strava-sdk | 248 | 89 | 21 | 5 | 48 | 18 | 10 | 56 | 1 | 191 | 671 | 68 | 60 | 81 |
| intercom-sdk | 213 | 117 | 28 | 0 | 7 | 0 | 0 | 61 | 0 | 179 | 35 | 65 | 28 | 0 |
| adyen-checkout-sdk | 202 | 147 | 16 | 0 | 24 | 0 | 0 | 15 | 0 | 266 | 422 | 66 | 42 | 0 |
| openai-sdk | 194 | 38 | 81 | 0 | 0 | 53 | 22 | 0 | 0 | 158 | 104 | 24 | 42 | 0 |
| open-meteo-sdk | 188 | 156 | 9 | 0 | 0 | 0 | 23 | 0 | 0 | 114 | 322 | 34 | 16 | 0 |
| airtable-sdk | 173 | 62 | 38 | 0 | 45 | 0 | 26 | 2 | 0 | 57 | 75 | 11 | 17 | 31 |
| square-payments-sdk | 173 | 134 | 0 | 4 | 10 | 0 | 0 | 25 | 0 | 124 | 75 | 35 | 46 | 0 |
| hubspot-crm-sdk | 159 | 68 | 0 | 1 | 15 | 54 | 0 | 21 | 0 | 199 | 209 | 20 | 37 | 83 |
| world-bank-sdk | 159 | 64 | 74 | 0 | 0 | 19 | 0 | 2 | 0 | 77 | 72 | 12 | 17 | 0 |
| mediawiki-action-api-sdk | 148 | 52 | 39 | 0 | 5 | 0 | 0 | 52 | 0 | 107 | 302 | 57 | 46 | 0 |
| frankfurter-sdk | 144 | 122 | 22 | 0 | 0 | 0 | 0 | 0 | 0 | 32 | 28 | 25 | 34 | 0 |
| linear-sdk | 125 | 54 | 24 | 0 | 0 | 16 | 31 | 0 | 0 | 103 | 115 | 19 | 14 | 33 |
| postmark-sdk | 115 | 59 | 30 | 0 | 8 | 0 | 0 | 18 | 0 | 102 | 39 | 19 | 22 | 0 |
| fred-sdk | 114 | 49 | 25 | 0 | 8 | 0 | 0 | 32 | 0 | 126 | 212 | 218 | 68 | 0 |
| pokeapi-sdk | 109 | 68 | 18 | 0 | 0 | 13 | 0 | 10 | 0 | 57 | 64 | 19 | 19 | 0 |
| arxiv-sdk | 101 | 64 | 0 | 0 | 20 | 0 | 0 | 17 | 0 | 212 | 365 | 44 | 37 | 0 |
| open-library-sdk | 94 | 94 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 117 | 29 | 78 | 38 | 0 |
| nager-holidays-sdk | 82 | 66 | 16 | 0 | 0 | 0 | 0 | 0 | 0 | 54 | 12 | 19 | 24 | 0 |
| nasa-neows-sdk | 34 | 34 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 101 | 45 | 20 | 19 | 0 |

## Largest excerpts per glue category (three SDKs each)

- **error**: `examples/salesforce-sdk/src/error.rs:18-631` (558 lines); `examples/datadog-events-sdk/src/error.rs:21-418` (359 lines); `examples/quickbooks-online-sdk/src/error.rs:92-479` (352 lines)
- **result**: `examples/amazon-s3-sdk/src/object.rs:242-387` (136 lines); `examples/distribution-registry-sdk/src/blob.rs:159-285` (119 lines); `examples/backblaze-b2-native-sdk/src/upload.rs:153-268` (108 lines)
- **operation**: `examples/paypal-sdk/src/lib.rs:232-248` (16 lines); `examples/discord-rest-sdk/src/limiter.rs:85-88` (4 lines); `examples/kraken-spot-rest-sdk/src/policy.rs:87-90` (4 lines)
- **redaction**: `examples/elasticsearch-sdk/src/error.rs:111-143` (33 lines); `examples/notion-sdk/src/error.rs:168-198` (31 lines); `examples/airtable-sdk/src/lib.rs:140-165` (26 lines)
- **forwarding**: `examples/elasticsearch-sdk/src/lib.rs:182-263` (79 lines); `examples/hubspot-crm-sdk/src/lib.rs:542-595` (54 lines); `examples/openai-sdk/src/lib.rs:374-419` (46 lines)
- **construction**: `examples/amazon-s3-sdk/src/client.rs:128-260` (127 lines); `examples/microsoft-graph-sdk/src/client.rs:83-180` (95 lines); `examples/snowflake-sdk/src/client.rs:82-152` (71 lines)
- **paging**: `examples/github-sdk/src/paging.rs:331-579` (234 lines); `examples/distribution-registry-sdk/src/pages.rs:122-288` (162 lines); `examples/europe-pmc-sdk/src/pagination.rs:12-175` (157 lines)
- **docs**: `examples/mastodon-sdk/src/lib.rs:44-50` (7 lines); `examples/clickhouse-sdk/src/wire.rs:4-9` (4 lines); `examples/ntfy-sdk/src/lib.rs:44-47` (4 lines)
