# Provenance

Every fixture under `fixtures/` and every inline response in `tests/` is hand-authored from the
sources below or synthetic. **None was captured from a ClickHouse server.** No request was made to
any ClickHouse server or service; no local ClickHouse was available on the machine (no
`clickhouse` binary, no container runtime), so there is no compatibility evidence from a real
server. The tests prove agreement with the documented shapes only.

## Sources checked (2026-11-06)

Read on **2026-11-06, between about 06:05 and 06:25 UTC**. The four pages of the brief were
first read with the session's `WebFetch` tool (a model's summary with short quotes), then
downloaded as Markdown with `curl` from `https://clickhouse.com/docs/<path>.md` (the docs site
serves each page's Markdown source; the requests followed the site's redirects to the
`/docs/reference/...` paths shown), so the example bytes below were taken from the page text
itself, not from a summary. The pages show **no version number or last-updated date**; their
content names features up to ClickHouse 26.10 (path routing on by default since 26.10, the
`X-ClickHouse-Format` change in 26.8) and their exception examples print server version
`25.11.1.1`. `sha256` prefixes identify the downloaded files.

| Source | Version / date | sha256 (16) | Used for |
|---|---|---|---|
| <https://clickhouse.com/docs/interfaces/http> | undated; mentions 26.8/26.10 | `890cdc45178f3e7e` | port 8123, `GET` read-only, `query` parameter and POST body (joined with a line feed), 200/500 statement, the `SELECT 1` response head (`X-ClickHouse-Summary` JSON with string counters, `-Query-Id`, `-Format`, `-Timezone`, `-Server-Display-Name`, `-Exception-Tag`, `Access-Control-Expose-Headers` naming `-Exception-Code`), `X-ClickHouse-Format` semantics before/since 26.8, `{name:Type}` placeholders and `param_<name>`, the escaped format of parameter values (`\t`, `%5C%09`, `\N`), Basic auth, `X-ClickHouse-User`/`-Key`, `user`/`password` parameters (not used: logged by proxies), `database` / `X-ClickHouse-Database`, `quota_key`, `query_id` ("any string"), `X-ClickHouse-Progress` with `send_progress_in_http_headers` and its fields (`memory_usage` from v25.11), "Running requests don't stop automatically if the HTTP connection is lost", `buffer_size` / `wait_end_of_query` and the caveat that it "doesn't completely solve the problem", the exception block layout (`\r\n__exception__\r\n<TAG>\r\n<error message>\r\n<message_length> <TAG>\r\n__exception__\r\n`, 16-byte tag, ≤ 16 KiB), the JSON and CSV late-exception examples, the documented error texts (`Code: 62 ... Multi-statements are not allowed`, `Code: 457 ... BAD_QUERY_PARAMETER`), INSERT with the statement in the URL and `CSVWithNames` / `JSONEachRow` bodies, empty body for statements without a table, `http_write_exception_in_output_format`, compression (excluded) |
| <https://clickhouse.com/docs/interfaces/formats/JSONEachRow> | undated | `13bbf8edad57b29e` | one JSON object per line; aliases; the 17-row football output example |
| <https://clickhouse.com/docs/interfaces/formats/CSVWithNames> | undated | `5a143131b5c59d77` | header record, the 17-row football output example (strings and dates quoted, numbers not), `input_format_with_names_use_header` |
| <https://clickhouse.com/blog/clickhouse-release-24-11> | "Dec 6, 2024" (release 24.11) | `d9f5b0bab972572f` (HTML) | "Exceptions in the HTTP interface": before 24.11 a plain-text exception appended to a cleanly ending body (exit code 0); from 24.11 the transfer is broken (`curl: (18) transfer closed with outstanding read data remaining`); the 24.3 and 24.11 exception texts |
| <https://clickhouse.com/docs/interfaces/formats/CSV> | undated | `8f917161ba6a0057` | quoting rules (doubled quotes), unquoted fields trimmed on input, `format_csv_allow_single_quotes`, `\N` NULL — the reasons for quoting every field on insert |
| <https://clickhouse.com/docs/interfaces/formats/TabSeparated> | undated | `91b4a306de6360da` | the escape sequences of the escaped format (`\b \f \r \n \t \0 \' \\`; minimum set tab, LF, backslash) |
| <https://clickhouse.com/docs/concepts/features/interfaces/client.md> | undated | `0e69002819bdbff1` | "Queries with parameters": `{<name>:<data type>}`, `Identifier` for table, database and column names |
| <https://clickhouse.com/docs/sql-reference/statements/kill.md> | undated | `79a655c90e9870de` | `KILL QUERY WHERE .. [SYNC\|ASYNC\|TEST]`, `ASYNC` default and not waiting, `kill_status` values, read-only users stop only their own queries, `NOTHING_TO_KILL` / `kill_throw_if_noop` |
| Session settings pages `http`, `http-response`, `send`, `replace-running-query` (`https://clickhouse.com/docs/reference/settings/session-settings/<group>.md`) | undated | `6b0a843f99418d51`, `db6be7dbd87a2d63`, `f4a666e86b7c7c2b`, `e47ac0530755bd98` | `http_wait_end_of_query`, `http_write_exception_in_output_format` (default 0, JSON/XML), `http_x_clickhouse_format_overrides_output_format`, `http_response_buffer_size`, `send_progress_in_http_headers` (default 0), `replace_running_query` (a repeated `query_id` is refused by default) |

**ClickHouse source** (facts the pages do not state), from `raw.githubusercontent.com`. The
`master` branch was at commit `c9e18d01727ebb09cb9e06ee90a4b436d5a3fb3a` (`git ls-remote`, same
session); the first two files were fetched from `master` a minute before that check, the others
by that commit.

| File | sha256 (16) | Used for |
|---|---|---|
| `src/Processors/Formats/Impl/JSONEachRowRowOutputFormat.cpp` | `a6bbec2375e796d2` | response `Content-Type` `application/x-ndjson; charset=UTF-8` (`application/json` only with `array_of_rows`) |
| `src/Processors/Formats/Impl/CSVRowOutputFormat.cpp` | `53252e3996a687b2` | response `Content-Type` `text/csv; charset=UTF-8; header=present` for `CSVWithNames` |
| `src/Server/HTTP/WriteBufferFromHTTPServerResponse.{h,cpp}` | `90fd3ec2dc913893`, `216250c47c43e8e2` | the block as written: `\r\n` `__exception__` `\r\n` tag `\r\n` message, **`\n`** (appended unless present), `<length>` counting that `\n`, ` ` tag `\r\n` `__exception__` `\r\n`; tag length 16; 16 KiB bound; the tag header set on every response head; `X-ClickHouse-Exception-Code` added to a head still being sent; the chunked body left without its last chunk |
| `src/Server/HTTP/exceptionCodeToHTTPStatus.cpp` | `3198bb1f5c68f69e` | statuses other than 500 (`SYNTAX_ERROR` → 400, `UNKNOWN_TABLE` → 404, 500 the default) |
| `src/Server/HTTPHandler.cpp` | `818a23cb318c30a6` | the `Access-Control-Expose-Headers` list |

The page's block layout and the source differ on the message line end (`\r\n` vs `\n`). The
page's own example settles it for the source: its message is 261 bytes and the trailer says
`262` (checked by `guard::tests::the_documented_example_length_counts_the_line_feed`). The
fixtures follow the source; the parser accepts both.

Not checked: `cancel_http_readonly_queries_on_client_close` (no page found under the settings
index in this session); the version that introduced the tagged block (the pages and the 24.11
notes do not say).

## Fixtures

| File | Source | Transformations | Kind |
|---|---|---|---|
| `football.csvwithnames` | CSVWithNames page, "Reading data" output | the 18 lines verbatim, LF after each (the page shows LF-separated lines; CR/LF cannot be seen in the rendering) | documented example |
| `football.jsoneachrow` | JSONEachRow page, "Reading data" output | the 17 lines verbatim, LF after each | documented example |
| `late_exception.csvwithnames` | HTTP page, "HTTP response codes caveats", the CSV example tail | a `CSVWithNames` header line added (the example is `FORMAT CSV`) naming the example's two columns; `0,0\n0,0\n`; the block with the documented tag `rumfyutuqkncbgau` and the documented 261-byte message; CR/LF placement from the source (the rendered page cannot show it) | documented example, adapted |
| `late_exception.jsoneachrow` | HTTP page, the JSON example | its two data rows rewritten as `JSONEachRow` lines (the example is `FORMAT JSON`); the block with the documented tag `dmrdfnujjqvszhav` and the documented message | documented example, adapted |
| `error_multi_statements.txt` | HTTP page, "Setting a role with query parameters" | the documented error line plus one LF (the source appends one) | documented example |
| `error_bad_query_parameter.txt` | HTTP page, "Tabs in URL Parameters" | the documented line; the gap the page renders between `abc` and `123` written as one tab (the parameter value in the example contains a tab); plus LF | documented example, adapted |
| `untagged_plain_exception.csvwithnames`, `.jsoneachrow` | 24.11 release notes (the 24.3 exception text, plain text after the rows) | synthetic table `a,b,c` with one row, then the 24.3 text and LF; the CSV case has exactly two commas so the text is a valid 3-field row | synthetic body, documented message |
| `perturbed.csvwithnames` | CSVWithNames page (header, first row) | synthetic rows: a field with doubled quotes and a comma, a field with a quoted line break, a 2026 row; a block with the synthetic tag `pqtrsvwxyzabcdef` and a synthetic `Code: 241 ... MEMORY_LIMIT_EXCEEDED` message | synthetic |
| `perturbed.jsoneachrow` | JSONEachRow page (first row) | synthetic rows with `ö` (2 bytes) and `😀` (4 bytes); the same synthetic block | synthetic |
| `kill_query.jsoneachrow` | KILL page (`kill_status` values) | one synthetic row; the `user` and `query` members are assumed (the page does not list the output columns; the SDK reads only `kill_status` and `query_id`) | synthetic |

`.gitattributes` marks `fixtures/** -text`: the blocks contain CR LF and the rows LF only.

## Synthetic cases in the tests

- Every response header value other than the documented `X-ClickHouse-Summary` of `SELECT 1`
  and the three documented `X-ClickHouse-Progress` lines: query IDs (`server-assigned-0001`,
  echoes of the request's `query_id`), the tag `kqzvmhwtyaxbnrce`, `fixture-server`,
  `Europe/London` (documented example value), insert summaries.
- Status codes 400 (`Code: 62`) and 404 (`Code: 60 ... UNKNOWN_TABLE`, text synthetic) follow the
  source's status mapping; 500 is the page's general statement; 503 and 307 are generic HTTP.
- The `200` head carrying `X-ClickHouse-Exception-Code: 159` and progress headers, followed by the
  block, is reconstructed from the source (`writeExceptionCode`), not from a documented example;
  the `TIMEOUT_EXCEEDED` and `CANNOT_PARSE_INPUT_ASSERTION_FAILED` texts are synthetic.
- The `wait_end_of_query` cases (an empty-body 500, then rows and a block after a 200) model the
  documented behaviour and its documented limit; no server was observed.
- The cleanly terminated chunked responses containing the synthetic `perturbed` exception blocks
  and the empty exception-tag header are synthetic robustness cases, not recorded server responses.
- The fake server evaluates workflow A's two parameters and workflow B's table and season itself
  (unescaping `param_*`), and serves workflow B's inserted bytes back; it runs no SQL. The CSV
  query-back echoes the SDK's all-quoted CSV, which a real server would re-format.
- The endless `system.numbers` stream (one row per 20 ms) and the KILL answer of the cancellation
  test.
- Credentials `analyst` / `fixture-password` are synthetic.
