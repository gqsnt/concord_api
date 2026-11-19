# Fuzz targets

One target for each reader of response data, each driven only through the public surface:

| Target | Reader |
| --- | --- |
| `sse` | `Sse`: a body cut into chunks, under a bound of up to 255 bytes |
| `json_lines` | `JsonLines`: the same |
| `link` | `by_link`: `Link` fields, one per line of the input |
| `next_url` | `by_next_url` and the URL that the request is sent to, with and without a `Secret` query entry |
| `retry_after` | `Retry-After`: `Headers::retry_after` and the decision to retry, on any clock |
| `date` | `Date`, as a limit reads it to count an RFC 3339 reset, against `Retry-After` |
| `rfc3339` | `Seen::until_rfc3339` |
| `counts` | `concord::limit::counts` |
| `remaining` | `concord::limit::remaining` and `remaining_rfc3339` |
| `content_length` | `Content-Length` as the size that a response announces |
| `json_label` | the content type that `Json.strict()` accepts |
| `transfer_at` | `transfer::at` |

Each target asserts what the contracts promise of every input: it never panics, a stream ends,
a value that is accepted has the form that the grammar gives, and an error holds no input.

This folder is a workspace of its own. The library's workspace, its checks and its packages
never build it or ship it.

## Running

The targets need a nightly toolchain and `cargo-fuzz`:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
```

Then, from the root of the repository:

```sh
cargo +nightly fuzz list
cargo +nightly fuzz run sse
cargo +nightly fuzz run link -- -max_total_time=600
```

`cargo fuzz` keeps what it finds under `fuzz/corpus/<target>` and `fuzz/artifacts/<target>`,
which git ignores. The tests of the crates hold a corpus of their own, in
`concord-extras/tests/corpus` and `concord-core/tests/corpus`, one case per line; a case
worth keeping from a fuzzing run belongs there, as a line of the file of its reader.
