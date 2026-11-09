# Mailgun SDK27

The declaration owns the provider error document: `error crate::Problem in concord::Json;`. Native prepared/decoded
paths carry that document as `D` in `Prepared<T, D>` and `concord::Error<D>`; SDK status routing
and validation remain in the policy. Concord withholds document contents in formatting.

A bounded Mailgun slice: text email with one file attachment, retained message ID,
and paged domain-event searches with per-recipient delivery inspection.
Live provider calls: **NOT_RUN**. Tests use only loopback fixtures.

This is an **example-assisted** SDK on Concord baseline
`8c27cd1a509c4cac4edde44f60c3d3ca13e2b46f`. The public hierarchy is
`Mailgun → domain → send_message/events`. Both endpoints use generated `.send()`, returning `Result<Response<T>, Error>`.

The following complete caller compiles as a doctest; it accepts only a local
fixture origin. The integration tests start the server and execute this workflow.

```no_run
use concord_sdk_mailgun::{
    Attachment, Domain, EventRange, Mailgun, PageSize, TextEmail, delivery_status,
};
use std::{num::NonZeroUsize, time::Duration};

async fn workflow(loopback_origin: &str) -> Result<(), Box<dyn std::error::Error>> {
    let api = Mailgun::local(loopback_origin, "synthetic-key", Duration::from_secs(30))?;
    let domain = api.domain(Domain::new("mg.example.test")?);
    let file = Attachment::from_file("report.txt", "report.txt", "text/plain")?;
    let email = TextEmail::new(
        "Sender <sender@mg.example.test>",
        vec!["alice@example.test".into()],
        "Report", "The report is attached.", file,
    )?
        .tag("reports")?
        .variable("customer-id", 42)?;
    let accepted = domain.send_message(email).send().await?;
    let message_id = accepted.value.id; // Persist as_str() in application storage.

    let search = domain.events(EventRange::new(1700000000, 1700000100)?)
        .set_message_id(message_id.clone())
        .set_recipient("alice@example.test")
        .set_tags("reports")
        .set_limit(PageSize::new(100)?);
    let events = search.pages(NonZeroUsize::new(20).unwrap())?
        .items().collect_complete().await?;
    let observed = delivery_status(&message_id, "alice@example.test", &events);
    println!("{observed:?}");
    Ok(())
}
```

For an application using the real service, `Mailgun::connect(Region::Us, key,
timeout)` or `Region::Eu` selects the domain's region. Construction does no I/O.
These constructors disable reqwest retries and redirects and require a nonzero
whole-request timeout. `local` additionally disables proxies and accepts only an
HTTP loopback IP. The generated `new`, `set_authorization`, `with_base_url` and
native request APIs are advanced exits: a caller supplying a different transport
must preserve these settings. A domain sending key cannot read events; the
connected workflow needs a key authorized for both operations.

`Attachment::bytes` uses Concord's `multipart` codec. `from_file` reads a local
file synchronously with a byte bound; use it outside a latency-sensitive async
task. `Attachment::stream` accepts a `Send + 'static` stream of
`Result<Bytes, std::io::Error>`. It uses reqwest multipart framing through an
`Encode<TextEmail>` adapter, with the boundary bound to `EncodedBody`. Neither
construction nor preparation polls the source. The send endpoint remains the
same generated operation. Streamed bodies are chunked and cannot be cloned.

`TextEmail::tag` adds a repeated `o:tag` field; up to three are allowed. Tags are
nonempty ASCII, at most 128 bytes, without control characters. Their spelling is
preserved; Mailgun treats tags case insensitively. `TextEmail::variable(name,
value)` adds `v:<name>` with a JSON-encoded value (`serde_json::Value` or a value
convertible to it). Strings include JSON quotes; objects, arrays, numbers,
booleans and null retain their JSON representation. Reusing a name replaces its
value. Supply a bare name matching `[A-Za-z0-9_-]+`: this conservative naming
rule belongs to this SDK because the provider docs do not specify a grammar.
A fourth tag or invalid name returns a value-free `Error::Input` immediately,
without network I/O or attachment polling. Both attachment encoders use the
same field mapping. As before, `Attachment::from_file` performs its own explicit
local read when called.

Event searches accept `.set_tags(...)` using Mailgun's `tags` filter expression.
The filter is query encoded and redacted in operation diagnostics. It is part
of the original search context for resumption; provider continuation URLs still
remain verbatim. `Event.tags` and `Event.user_variables` expose the returned
metadata and default to empty collections when absent. Inspect those fields
explicitly; event Debug remains redacted. Mailgun also exposes custom variables
in the recipient's `X-Mailgun-Variables` header. The provider documents a 16 KiB
aggregate send-options limit and truncation of variable data over 4 KiB in
events/webhooks; this SDK does not promise complete metadata beyond those limits.
Contract sources read **2026-11-07**: [send reference](https://documentation.mailgun.com/docs/mailgun/api-reference/send/mailgun/messages/post-v3--domain-name--messages),
[tagging](https://documentation.mailgun.com/docs/mailgun/user-manual/tracking-messages/track-tagging),
[custom variables](https://documentation.mailgun.com/docs/mailgun/user-manual/sending-messages/send-attachments#attaching-metadata-to-messages),
and [event filters](https://documentation.mailgun.com/docs/mailgun/api-reference/send/mailgun/events/get-v3-domain_name-events).

```compile_fail
use concord_sdk_mailgun::TextEmail;
fn cannot_replay(email: TextEmail) {
    let duplicate = email.clone(); // TextEmail intentionally does not implement Clone.
}
```

No send is retried automatically, including buffered sends, 401, 429 and 5xx.
A refused connection on a send is still `Error::AmbiguousSend`: Concord reports it as
`Delivery::Unknown` (a redirect target can refuse after the send was accepted).
There is no generic Mailgun send idempotency guarantee in this slice. A transport
failure after dispatch, timeout, or unusable success body is `Error::AmbiguousSend`:
the message may have been accepted, and the SDK may not have obtained its ID.
Cancellation after starting a send has the same uncertainty, though dropping a
future cannot return an error. Do not resubmit automatically. Application-level
reconciliation or an explicit human/business decision is needed. A late streaming
source or size failure is also conservative ambiguity.

Every non-2xx response is `Error::Provider`: HTTP 400, 401, 403, 404, 429, 5xx,
or `Unexpected` (including redirects). Its HTTP status remains authoritative
even if the bounded JSON `message` document is missing, malformed, interrupted,
oversized, or labelled with the wrong media type. Error reads are limited to
16 KiB, as are send acknowledgements; event responses are limited to 1 MiB.
`Error::Concord` preserves build/read/media/decode/limit diagnostics for reads.
Local validation and link failures have separate `Input` and `Link` variants.

Reads also execute once. `Metadata` exposes strict rate-limit numbers and the
shared `Retry-After` parser, without scheduling delays. No account quota or
process-wide admission is invented. SDK errors, responses, event models,
attachments and continuations redact their Debug output. Error Display and source
chains do not disclose keys, bodies, header values or URLs. `MessageId::Display`
is deliberately its query wire spelling; use `Debug` for redaction. Raw fields,
`as_str()`, native accessors and the base URL are explicit disclosure boundaries.

Event queries require a finite Unix-seconds interval and use ascending order.
Page size is 1–300 (default 100). `pages(cap)` counts every HTTP page, including
the empty terminal page. It follows provider URLs verbatim after verifying the
origin and domain events collection; it never reconstructs opaque cursors or
appends the original query to a continuation. Empty items end the traversal even
when a next link remains. Missing/malformed paging fields fail decoding.

`collect_complete` returns either all items or an `Incomplete` containing the
collected prefix and error. Resume with the same generated search using
`pages_from(error.resume().unwrap().clone(), cap)`; continuations bind the exact
original URL/query and credential. Cycles are refused before a duplicate send
within a traversal. A rejected next link fails its page and retains the failed
position, not that page's items. Do not treat a bounded/failed prefix as complete.
Resumption is explicit and does not promise a stable snapshot, deduplication, or
an event-retention period. Persisted checkpoints and polling are outside scope.

Use `concord_sdk_mailgun::pagination::Resume` for the position. `Resume::Start` repeats the
exact original events search request; `Resume::At(key)` takes the context-bound `EventCursor`
from `paging.next` in a returned page's `next`.

`delivery_status` checks the message ID (allowing surrounding angle brackets),
recipient and latest delivery-relevant timestamp. `Accepted` means queued;
`Delivered` means the recipient mail server accepted it, not inbox placement or
reading. Test-mode deliveries return `TestMode`. Temporary/permanent failures
remain distinct. Unknown events do not establish delivery, and equal-time
contradictions return `Conflicting`. The conclusion is only about the retrieved
window; absent events do not establish non-delivery.

Local input bounds: one attachment up to 16 MiB, text up to 1 MiB, 1–100 recipients,
998-byte sender/recipient/subject fields and a 255-byte filename. Filename control
characters, quotes, slashes and backslashes are refused. This does not predict
the final MIME size; Mailgun's server limit still applies. Full mailbox syntax is
left to the provider.

Excluded: inbound routes, MIME construction/submission, HTML/templates, inline
attachments, mailing lists, webhooks, automatic polling, resends, recipient
batch personalization, durable state, provider quota coordination and delivery
guarantees. No live provider behavior, TLS, HTTP/2 or distributed quotas were tested.

The exact official sources, retrieval date (2026-11-07), fixture derivations and
consulted examples are in `PROVENANCE.md`; costs and proposed Concord changes are
in `FRICTION.md`; commands and observed results are in `VALIDATION.md`.
