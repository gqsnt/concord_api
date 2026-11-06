# Between waves 2-3

Coordinator integration of the client **send policy** (`send P;`: every generated `send()`
returns `<P as concord::SendWith<T>>::Output`). "Before" is `1f71432c`, "after" the commit that
adds this file. `Prepared::follow`, `Received::bytes_limited` and `Received::decode_limited` have
no use here (no server-returned URLs, no error document is parsed, no documented size bound).

## Verdict: adopted

The author's friction entry 2 ("SDK metadata/errors still need an additional caller method beside
generated send") is exactly what a send policy removes. Before, the SDK had two terminal methods
on one operation: the documented `fetch()` (`HolidayResponse`, classified `Error`) and the
generated `send()` (`Vec<Holiday>`, Concord's error), and the README spent a paragraph telling
callers which one to use. After, the generated `send()` is the SDK path; Concord's path is the
explicit `prepare()?.send()`. The adapter body is unchanged and the same size.

Decisive for the whole path:

- caller: one terminal method, no import, and the method that loses metadata and the 400
  classification is no longer the one with the conventional name;
- maintainer: a second operation with another output is a compile error naming the policy
  (below), where before it would silently have had only the generated `send()` and no `fetch()`;
- cost: rustdoc of `send()` shows a projection instead of the concrete type (below), so the
  classification is documented on `KeepHead` and referenced from the operation's doc comment.

## Declaration

Before (`src/lib.rs`):

```rust
    pub client NagerHolidays() {
        base "https://nagerholidays.com";
        format Json;

        /// ...
        /// claim to validate provider coverage or year availability. Use `fetch()` to
        /// preserve metadata and classify HTTP 400 as an invalid request. Generated
        /// `send()` remains available and returns Concord's own error instead.
        GET holidays(country_code: String, year: u16) -> Vec<Holiday> {
```

After:

```rust
    pub client NagerHolidays() {
        base "https://nagerholidays.com";
        send KeepHead;
        format Json;

        /// ...
        /// claim to validate provider coverage or year availability. `send()` returns a
        /// [`HolidayResponse`] with the response metadata, or this SDK's [`Error`], which
        /// classifies HTTP 400 as an invalid request ([`KeepHead`]). `prepare()` builds the
        /// native request; `prepare()?.send()` returns Concord's own error instead.
        GET holidays(country_code: String, year: u16) -> Vec<Holiday> {
```

## SDK adapter

Before, an inherent method on the generated endpoint type (doc comment omitted):

```rust
impl NagerHolidaysHolidays {
    pub async fn fetch(self) -> Result<HolidayResponse, Error> {
        let received = self.prepare()?.receive().await?;
        let meta = ResponseMeta::capture(&received.head());
        match received.decode().await {
            Ok(holidays) => Ok(HolidayResponse { holidays, meta }),
            Err(error) => Err(Error::with_meta(error, meta)),
        }
    }
}
```

After, the policy the declaration names (doc comment omitted). It is written for the one output
the client declares, since `HolidayResponse` is specific to holidays:

```rust
pub struct KeepHead;

impl SendWith<Vec<Holiday>> for KeepHead {
    type Output = Result<HolidayResponse, Error>;

    async fn send<E: Endpoint<Vec<Holiday>> + Send>(endpoint: E) -> Self::Output {
        let received = endpoint.prepare()?.receive().await?;
        let meta = ResponseMeta::capture(&received.head());
        match received.decode().await {
            Ok(holidays) => Ok(HolidayResponse { holidays, meta }),
            Err(error) => Err(Error::with_meta(error, meta)),
        }
    }
}
```

## One caller call

Before (`README.md`, workflow A, and `tests/protocol.rs`):

```rust
    let response = api.holidays("AT", 2026).fetch().await?;
```

After:

```rust
    let response = api.holidays("AT", 2026).send().await?;
```

## Lines of SDK adapter code

| | lines | code (no blank/comment lines) |
|---|---|---|
| before: `impl NagerHolidaysHolidays { fetch }` | 19 | 10 |
| after: `KeepHead` + `impl SendWith<Vec<Holiday>>` | 22 | 12 |

Plus one declaration line (`send KeepHead;`) and `Endpoint, SendWith` in the import.

## Adding an operation (say `GET countries() -> Vec<Country>`)

| Before | After |
|---|---|
| 1. the `api!` entry | 1. the `api!` entry |
| 2. an inherent `fetch()` on the new endpoint type (and a response type); nothing points at a forgotten one, and that operation's only terminal method is then Concord's `send()` | 2. `impl SendWith<Vec<Country>> for KeepHead` (or a generic impl with a generic response type); the compiler asks for it |
| 3. README | 3. README |

Observed with a temporary second operation `GET countries() -> Vec<String>` (added, checked,
removed):

```text
error[E0277]: the send policy `KeepHead` does not send a `Vec<String>` output
  --> examples/nager-holidays-sdk/src/lib.rs:24:14
   |
24 |         send KeepHead;
   |              ^^^^^^^^ named by the client's `send` statement; it needs `KeepHead: SendWith<Vec<String>>`
   |
   = note: a send policy implements `concord::SendWith<T>` for every output `T` its endpoints declare (usually generically: `impl<T: Send> SendWith<T> for Policy`)
help: the trait `SendWith<Vec<String>>` is not implemented for `KeepHead`
      but trait `SendWith<Vec<Holiday>>` is implemented for it
```

The same error is reported a second time on the whole `concord::api!` invocation (span
`src/lib.rs:15`), with the same message: two errors for one missing impl.

## What a caller sees (observed)

Rustdoc of `NagerHolidaysHolidays::send`:

```text
pub fn send(self) -> impl Future<Output = <KeepHead as SendWith<Vec<Holiday>>>::Output> + Send
```

with Concord's generated sentence ("Sends the request with the client's send policy
`KeepHead` ..."). `fetch()` showed `Result<HolidayResponse, Error>` and its `# Errors` section
directly; now the reader follows `KeepHead`, whose doc carries that section, and the operation's
doc comment names `HolidayResponse`, `Error` and `KeepHead`. rustc normalises the projection in
type errors (observed in the PokéAPI record: "expected `Pokemon`, found `Response<Pokemon>`").

## Tests and behaviour changes

- 31 call sites `.fetch()` → `.send()` (`first_use.rs` 10, `protocol.rs` 15, `workflows.rs` 6)
  and four in the README's three examples (compiled as doctests), whose prose now names
  `send()` and `prepare()?.send()`; no expectation changed.
- One site changed its path, not its expectation: `protocol.rs`
  `optional_unknown_fields_and_null_subdivisions_preserve_known_fields` read the AT array a third
  time through the generated `send()` to show that Concord's own path shares the response model.
  That path is now `prepare()?.send()`, so the line reads
  `api.holidays("AT", 2026).prepare().unwrap().send().await.unwrap()` and is still compared with
  `original.holidays`; the three-request and connection-cleanup assertions are unchanged.
- Results: `first_use` 9, `protocol` 14, `workflows` 3, doctests 4 — all pass, as at `1f71432c`;
  `cargo clippy --all-targets -D warnings` and `cargo doc` (`-D warnings`) clean.
- `FIRST_USE.md` and `MAINTENANCE.md` carry a short coordinator note on the edited test lines;
  `PROVENANCE.md` names the policy instead of the `fetch()` method.
