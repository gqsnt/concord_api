# Adyen Checkout SDK31

`concord-sdk-adyen-checkout` implements Checkout **v71** at
`https://checkout-test.adyen.com`: available payment methods, sandbox payment submission,
`/payments/details` continuation and manual capture request. All verification uses local fixtures.
**Live provider calls: NOT_RUN.** This is an **example-assisted** implementation; see
[PROVENANCE.md](PROVENANCE.md) for the exact reading ledger and fixture sources.

The public shape is `Checkout → v71() → payment_methods/payments/details → send()`, and
`Checkout → v71() → payment(psp_reference) → captures → send()` for an existing payment.
A response contains `value`, the authoritative `meta`, and `retry` accounting.

```rust,no_run
use concord_sdk_adyen_checkout::{
    Amount, Checkout, DetailsRequest, Error, MethodsRequest, PaymentRequest,
    PaymentState, SandboxMethod, SendPolicy, Web,
};

// base_url is the URL of an already-running LOCAL TestServer, never a provider URL.
async fn local_checkout(base_url: &str) -> Result<(), Error> {
    let policy = SendPolicy::default(); // share across handles/credentials for one company
    let api = Checkout::sandbox("synthetic-api-key", policy)?
        .with_base_url(base_url)?;
    let checkout = api.v71();
    let amount = Amount { currency: "EUR".into(), value: 1000 };
    let methods = checkout.payment_methods("unique-methods-key", MethodsRequest {
        merchant_account: "TestMerchant".into(), amount: amount.clone(),
        country_code: Some("NL".into()), shopper_locale: None, channel: Web::Web,
    }).send().await?;
    let ideal = methods.value.payment_methods.iter()
        .find(|m| m.kind == "ideal").expect("fixture offers iDEAL");
    let issuer = ideal.attributes["issuers"][0]["id"].as_str().unwrap();
    let submission = checkout.payments("unique-payment-key", PaymentRequest {
        merchant_account: "TestMerchant".into(), amount,
        reference: "merchant-order-123".into(),
        return_url: "https://merchant.example/return".into(),
        payment_method: SandboxMethod::Ideal { issuer: issuer.into() },
        channel: Web::Web,
    });
    // A deliberate caller retry clones this operation, preserving its key and body.
    let payment = submission.clone().send().await?;
    if payment.value.state() == PaymentState::ActionRequired {
        let action = payment.value.action.unwrap();
        // The application presents the action and obtains this opaque result from its UI.
        // Never feed the action URL itself to the Checkout API or log its secret query.
        let mut details = DetailsRequest::redirect("synthetic-return-result");
        details.payment_data = action.payment_data;
        let continuation = checkout.details("unique-details-key", details).send().await?;
        // Continuation may itself contain another action, or still be Pending/Received.
        let _state = continuation.value.state();
    }
    Ok(())
}
```

## Manual capture

With manual capture configured for the merchant account (Customer Area; this SDK's
`PaymentRequest` has no per-payment capture field), an authorised payment is captured later:

```rust,no_run
use concord_sdk_adyen_checkout::{Amount, CaptureRequest, CheckoutV71, Error};

async fn capture_part(checkout: &CheckoutV71, psp_reference: String) -> Result<(), Error> {
    let capture = checkout.payment(psp_reference).captures("unique-capture-key", CaptureRequest {
        merchant_account: "TestMerchant".into(),
        amount: Amount { currency: "EUR".into(), value: 600 }, // full authorised amount or less
        reference: Some("merchant-order-123-capture-1".into()),
    });
    // A deliberate retry clones this operation, preserving its key and body.
    let receipt = capture.clone().send().await?.value;
    // Accepted for asynchronous processing only: no money is shown to have moved.
    let (_capture_psp, _asked) = (receipt.psp_reference, receipt.requested);
    Ok(())
}
```

`POST /v71/payments/{paymentPspReference}/captures` sends the merchant account, amount and
optional reference with an `Idempotency-Key`; Adyen answers `201` with `status: received`
([captures reference](https://docs.adyen.com/api-explorer/Checkout/71/post/payments/(paymentPspReference)/captures),
[capture guide](https://docs.adyen.com/online-payments/capture), both read 2026-11-07).
`send()` returns `CaptureReceived` — the capture's own `psp_reference`, the
`payment_psp_reference`, the `requested` amount, merchant account and reference — only when the
document says `received` and echoes exactly what was sent: the path's payment reference,
merchant account, amount and reference (absent when none was sent). It has no state or
outcome field; the final result arrives as a CAPTURE or CAPTURE_FAILED webhook, which this SDK
does not handle, and a partial capture may cancel the remainder unless multiple partial captures
are enabled. Another status or a missing field is an unreadable success document
(`UncertainDelivery`, Decode cause). A document for a different request — for example the same
key used elsewhere in the company for another capture — is `GuaranteeNotConfirmed` and revokes
the key, since replaying it cannot submit this capture.

Captures share the payment ledger and its rules: a retry reuses the key with identical bytes; a
changed amount (or any change) under a used key, including the payment's own key, is
`ChangedRequest` before sending. A capture whose total deadline passes is `Deadline` and is not
replayed automatically; transport failures before any head are replayed automatically with the
same key and bytes while the guarantee holds, exactly as for payments. The payment reference is
one percent-encoded path segment (empty, `.` and `..` are refused before sending) and is withheld
from Debug output.

For cards, `SandboxMethod::test_card` accepts only Adyen-style `test_` tokens. No raw
PAN/CVC or live encrypted card constructor is provided. The API validates merchant, currency,
amount and payment-method availability. Amounts use integer minor units.

`Action` retains its type, optional payment data and all unknown fields. The caller can inspect
redirect method/URL/data or pass opaque component data to its own integration. The SDK never
visits the URL. A result code is retained verbatim; `state()` treats the presence of an action
as requiring action, `Received`/`Pending` as pending, and partial authorisation or unfamiliar
codes as `Other`. Authorisation does not establish capture or settlement. A refusal in HTTP 200
is a normal response with `Unsuccessful`, not a transport failure. No GET-by-payment-reference
endpoint is invented; existing flows continue by submitting their details.

The policy binds each key to the exact encoded body, method, full destination and declared
headers (including credential). Automatic attempts clone one prepared request. A caller retry
must use the same key and identical inputs. A changed request under an existing key, expired
window, revoked guarantee, concurrent same-key call, or unavailable idempotency store produces
`Error::Reconciliation`; it never generates a replacement key or removes the header.

Adyen documents a 64-character key maximum, company-wide key scope, regional isolation and
7–14 day retention. This SDK conservatively stops replay at seven days from local reservation.
It checks monotonic and wall-clock age after backoff and immediately before dispatch;
a wall-clock rollback also refuses replay. Supply unique random keys from
the application. A successful response must echo one identical key; a differing or repeated echo
on an error also revokes the local guarantee. Missing echoes on errors do not grant replay.
See [Adyen idempotency](https://docs.adyen.com/development-resources/api-idempotency/)
(read 2026-11-07).

Automatic retries cover transport failures before a response head, and non-success responses
with exactly one `transient-error: true` field. Absent, false or malformed permission stops the
loop. `Retry-After` is a minimum; invalid/repeated hints stop replay. Defaults are three attempt
factories, a ten-second head/attempt budget and capped exponential waits starting at 100 ms
(cap two seconds), with deterministic delays and no random jitter. `with_retry` changes these
bounds and requires a finite nonzero timeout. A deadline preserves the last completed head if
one exists; it may precede the dropped attempt and is not proof of that attempt's outcome.
Body consumption follows the retry loop, is never retried automatically, and is bounded by the
transport's 30-second request timeout. `retry.attempts` counts factory starts: a local guard
refusal can count without a wire send. No limiter claim is made about Adyen's distributed quota.

Non-success statuses remain `Error::Provider` even if their error document is malformed,
truncated, oversized or mislabeled. Codes 703/704 are promoted to reconciliation with the
provider failure retained. Unreadable success documents and unresolved transport/deadline
failures also require reconciliation. Inspect `Reconciliation` for its reason, last metadata,
optional provider failure and Concord cause. Reconcile against durable merchant records and
provider records using the original reference/key; never assume a fresh payment is safe.

The ledger is **process-local**, shared by cloned policies, and bounded (128 keys by default;
`SendPolicy::new` changes capacity). It retains expired entries and fails closed at capacity.
It retains up to 256 KiB per request; success bodies are bounded to 256 KiB and error bodies to
16 KiB. Cancellation releases only local in-flight admission and keeps the key/request binding.
An application must retain original keys, complete requests, first-attempt times and business
outcomes durably, coordinate across processes, and refuse unverified historical retries after
restart. A newly constructed policy cannot detect earlier keys. Do not replace a full ledger to
make uncertain keys appear new. There is no persistence or external reconciliation service here.

`Checkout::sandbox` disables redirects and reqwest retries and accepts only the Adyen test host
or HTTP IP loopback as the generated send destination. `Checkout::new` accepts a caller transport;
that transport must also disable redirects/retries and must not add changing default headers.
`prepare()` is the native inspection/escape path: executing it directly skips SDK input, ledger,
retry, destination and provider-error policy. It retains the declared response limit and JSON
format. The policy itself decomposes a prepared request solely to perform fallible checks before
every retry dispatch; it does not rewrite bodies, routes or headers.

SDK and generated Debug/Display withhold credentials, idempotency keys, payloads and provider
messages. Header/body accessors, serialization and native requests deliberately expose wire data;
redaction does not zeroize memory. Even apparently harmless provider strings can contain secrets,
so public provider models use redacted Debug implementations.

Excluded: recurring payments, stored methods, live card capture, capture outcomes, line items,
splits, refund/cancel/accounting operations, webhook verification, browser redirect execution
and universal 3DS orchestration.
Applications own shopper interaction, asynchronous finality and accounting reconciliation.

Verification commands (run from the workspace, one Cargo command at a time):

```sh
CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-adyen-checkout
CARGO_BUILD_JOBS=4 cargo clippy --locked -p concord-sdk-adyen-checkout --all-targets -- -D warnings
CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-adyen-checkout --no-deps
```
