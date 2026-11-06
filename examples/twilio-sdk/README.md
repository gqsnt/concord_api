# Twilio Messaging fixture SDK

`concord-sdk-twilio` is an unpublished Concord SDK slice for creating a text Message
(`To`, `From`, `Body`, optional `MessageIntent` and `StatusCallback`) and fetching it
by SID to inspect status and processing errors. It targets Twilio's `2010-04-01` REST paths. All tests
use synthetic credentials and the local `concord-example-testserver`; no SMS is sent.

Excluded: listing/pagination, updates/deletion, media, other channels, templates,
Messaging Services, scheduling, callback handling, signature validation, retries, polling
and delivery guarantees. The status enum recognizes the resource's documented
values, including values belonging to excluded workflows, and retains future values.

## Construction and workflow A: submit a synthetic message

The caller supplies a `concord::reqwest::Client`, an account for the URL path and
separate Basic credentials. These can represent API-key authentication or
account-SID/auth-token authentication; this SDK does not acquire credentials.
The visibly synthetic strings below are not valid Twilio credentials or SIDs.
`with_base_url` takes the fixture server's origin, without `/2010-04-01`.

```no_run
use concord::reqwest;
use concord_sdk_twilio::{BasicAuth, CreateMessage, MessageStatus, Twilio};
use std::time::Duration;

async fn submit(fixture_url: &str) -> Result<String, Box<dyn std::error::Error>> {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(5))
        .build()?;
    let client = Twilio::new(
        http,
        "AC_fixture_account",
        BasicAuth::new("fixture-user", "fixture-password")?,
    ).with_base_url(fixture_url)?;

    let submitted = client.create_message(
        CreateMessage::new("+15005550006", "+15005550001", "Hello café & + 🌍")
            .with_message_intent("notifications")
            .with_status_callback("https://callbacks.example.invalid/status"),
    ).await?;
    assert_eq!(submitted.meta.status().as_u16(), 201);
    assert_eq!(submitted.value.status, MessageStatus::Queued);
    Ok(submitted.value.sid)
}
```

The default base is `https://api.twilio.com`, but no live provider verification is
part of this SDK. Tests always override it before executing. A live application
would need valid account access, credentials and appropriate senders/recipients;
sending may incur charges. Transport policy remains with the supplied client;
the example disables transport retries and redirects as well as SDK-level retries.

`CreateMessage::new` omits both optional fields. Use `with_message_intent` or
`with_status_callback` on an individual message to include one; `None` omits the
field and `Some("")` sends an explicit empty value. Pass the original value without
form pre-encoding: Concord's `Form` handles Unicode, `&` and literal `+` characters.
Twilio documents [`StatusCallback`](https://www.twilio.com/docs/messaging/api/message-resource#request-body-parameters)
as the URL for message status notifications, with a valid hostname and no underscores.
This SDK passes the URL through without validation or normalization. Receiving and
verifying callbacks remains the application's responsibility.

## Workflow B: retrieve status and expose a processing error

Use the SID returned by creation. A fetched resource with `Failed` or `Undelivered`
is an HTTP success containing a message-processing failure, not a rejected GET.

```no_run
use concord_sdk_twilio::{MessageStatus, Twilio, TwilioError};

async fn inspect(client: &Twilio, submitted_sid: &str) -> Result<(), TwilioError> {
    let observed = client.get_message(submitted_sid).await?;
    match observed.value.status {
        MessageStatus::Failed | MessageStatus::Undelivered => {
            let code = observed.value.error_code;
            let explanation = observed.value.error_message.as_deref();
            // Present these deliberately: explanation may contain submitted data.
            let _ = (code, explanation);
        }
        MessageStatus::Queued | MessageStatus::Sending | MessageStatus::Sent => {
            // No delivery conclusion follows from these intermediate states.
        }
        _ => {} // Inspect other/unknown states according to application policy.
    }
    Ok(())
}
```

An empty SID is refused locally (`TwilioError::EmptySid`, nothing sent): it would
address `Messages/.json`. Other SID spellings are sent as one encoded path segment.

## Errors and completion

Every operation method (`create_message`, `get_message` and both `prepare_*`)
returns the SDK's `TwilioError`. Only `with_base_url` returns Concord's own
`concord::Error` (a `Build` error for an unusable base); `?` converts it. The old
name `Error` remains as a hidden alias of `TwilioError` for existing code.

`TwilioError::Provider` represents any non-2xx HTTP response. It keeps actual HTTP
status, version, all headers and Concord's generated operation identity, plus a
typed `ProviderError` (`status`, optional `code`, `message`, optional `more_info`).
The body's claimed status never replaces the HTTP status. The document is
JSON-checked and read with Concord's `Received::bytes_limited` up to 64 KiB; wrong
media type, truncation, oversized bodies and malformed documents remain HTTP
failures with an `ErrorDocumentProblem` and metadata.

`TwilioError::Concord` keeps the original build/transport/metadata/decode error and
response metadata whenever a head arrived. `error.concord()`, `error.meta()` and
`error.operation()` provide inspection; `error.provider_error()` exposes a decoded
provider exception. There is no fabricated status for a lost response.

The operations are declared on a generated client named `TwilioApi`, held inside
`Twilio` and not exported (its configuration takes a raw header; `Twilio::new` takes
validated `BasicAuth`). Diagnostics use that name: operation identities are
`TwilioApi.create_message` and `TwilioApi.get_message`, error `Display` starts with
`POST TwilioApi.create_message:` or `GET TwilioApi.get_message:`, and `Debug` of the
client is `Twilio { api: TwilioApi { .. } }` with the credentials and account redacted.

Each operation makes one Concord execution and completes only after decoding the
whole JSON resource. There is no streaming or pagination in this scope. A successful
POST confirms a resource response, not delivery. An interrupted or undecodable POST
may already have created a message: this SDK never retries it. If creation succeeded
and the later GET failed, the caller still owns the earlier `Response<Message>`;
keep its SID and retry only according to application policy. There is no transaction
or rollback across those calls. Canceling a future similarly cannot undo a write.

## Limits and native inspection

Successful JSON bodies are buffered by Concord without an SDK byte cap; configure
transport timeouts for stalled responses. The SDK validates Basic credential
structure and refuses an empty message SID, but not SID formats, phone numbers,
message lengths, intent vocabulary, callback URLs or whether the authenticated
principal can access the path account. Provider
acceptance is outside fixture verification. Missing/null optional resource fields
stay `None`; missing identity/status fields or wrong field types are decode errors.

`prepare_create_message` and `prepare_get_message` build native inspectable Concord
requests without I/O. Sending those prepared values directly bypasses the SDK error
adapter and metadata envelope. The normal `create_message`/`get_message` methods
always use it. SDK diagnostics withhold credentials, the account SID, message
contents, addresses, header values and provider explanations. `Debug` of a
`Message` (and of a `Response<Message>`) shows its `sid`, `status` and `error_code`:
the message SID is a resource identifier, not a credential, so the fetch path does
not mark it sensitive either and a prepared fetch's `Debug` shows it. Explicit
field/header access and native request inspection are unredacted; credentials are
marked sensitive on the header.

## Evidence and maintenance perturbation

`tests/protocol.rs` asserts raw methods, targets, headers and body bytes;
`tests/workflows.rs` exercises both public workflows and partial outcomes. The
optional `MessageIntent` test adds `通知 & + café` to one POST, verifies exact
percent encoding, and proves a later unmodified POST and GET are unchanged. This
stress value is synthetic, not a claim that Twilio accepts it as an intent.

The `StatusCallback` maintenance test sends
`https://callbacks.example.invalid/通知?label=café&tag=a+b` and compares the entire
POST body with hand-authored fixture bytes. It also checks an explicit empty value,
coexistence with `MessageIntent`, subsequent POSTs with the callback omitted and a
bodyless GET. The URL is a synthetic encoding probe; no callback destination is
contacted and provider acceptance of the Unicode URL is unverified. Existing
workflows and first-use tests keep their original calls. See `MAINTENANCE.md` for
the change scope and verification.

Fixtures are hand-authored from the official documentation and include deliberate
malformations. They are **not vendor conformance evidence**. See `PROVENANCE.md`
for the source/date and transformations, and `FRICTION.md` for author evidence.
The Stripe SDK's `receive`/metadata/error adapter and the GitHub SDK error model
were reading material; Stripe supplied the response-adapter pattern used here.
