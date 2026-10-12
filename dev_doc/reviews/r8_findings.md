# Concord V3 R8 finding dispositions

Baseline `09c560ad38b49021c5a10c80060674a45366fc3f`, tree
`1497b8c163c44e0c5e9bcb2dd8e2d07af56f2517`; remote branch had not advanced.
Correction candidate `cef236aa2502df5653d77de2975051b5a6d49565`. All four findings are
**CONFIRMED_FIXED**, after native discriminating BEFORE execution and AFTER
controls. No finding was refuted or left unverified. Setup errors are distinct
from defect evidence; the independent reviewer had not executed their fragments.
The additional correction pins JSON and captured migration-harness LF endings;
fixture values, historic digests and test expectations remain unchanged.

| Finding | Native BEFORE | Repair and native AFTER |
|---|---|---|
| R8-01 | `before-auth-native`: 30s recovery refresh #2 failed, one application handoff; allowing #2 succeeded but counted two refreshes. 120s control passed with one refresh and two handoffs. | Recovery returns an owned prepared plan seeded with the exact recovered slot preparation. 30s recovery succeeds with one refresh/two handoffs even when #2 would fail. Shared placements use one identity; independent provider retains ordinary preflight (one independent proactive refresh). |
| R8-02 | `before-body-cooldown-native`: two sequential 429 known-length errors remained ResponseTooLarge but made two helper exchanges. `before-wire-native`: chunked excess also made two. `before-provider-native`: returned body timeout lost Retry-After rather than retaining 3s after a 2s acquisition and a 5s cap. | Parse once at head arrival before length checks, retain the existing anchored hint, decorate returned body errors with positive capped remaining delay. TLS known-length 429/503, unknown-length chunked excess, truncated body and returned request-local timeout make one helper exchange, no application handoff. Underlying error kind/contact journal remain accurate. |
| R8-03 | `before-oauth-native`: real TLS head/partial JSON followed by 1.2s delayed tail with expires_in=1 still issued one protected application handoff. Delayed expires_in=120 control passed. | Private std::time::Instant send-start anchor feeds checked decoder addition. Same delayed 1s TLS token produces zero application handoffs and fails after bounded two helper exchanges; 120s control succeeds. Native decoder probes separately cover zero, elapsed TTL, checked-add overflow and missing optional expires_in. |
| R8-04 | `before-oauth-native`: missing token_type installed a usable token and issued a protected application handoff. Present malformed-type negatives and valid Bearer controls passed. | Require a string equal to Bearer ignoring ASCII case. Missing/null/numeric/empty/mac fail with Acquisition, repeated malformed results make another helper call and no application request. bearer/BeArEr and unknown extra fields pass; expires_in remains separately optional. |

The TTL and token-type requirements derive from
[RFC 6749 section 5.1](https://www.rfc-editor.org/rfc/rfc6749#section-5.1).
Send start is conservative because server generation is not observable.
CredentialMaterial and decoder use the same std monotonic clock; these TTL
probes use real elapsed time, with Tokio unpaused. Core delay probes use the
existing operation clock, including paused-time tests with no std TTL mixing.

R8-01 admission controls cover expiry, clear, replacement and rejection after
recovery, already-running refresh/followers, unrelated-operation proactive
policy and configured 403. One-shot body, non-idempotent effect, zero replay
budget and capped sends retain one handoff and zero recovery refreshes.
R8-02 controls cover no head, accepted bad body, absent/zero/malformed/duplicate/
past hints, overlarge hints capped before elapsed subtraction, configured
HTTP-date clock read once, elapsed hint and natural cooldown recovery.
Existing response-progress, cancellation and provider leader/journal suites
remain in the full matrix. Cancellation/enclosing-deadline abandonment does
not manufacture a returned body result or completed cooldown.

No product public signature or dependency changes. The intentional behavior
change rejects malformed OAuth responses that omit Bearer token_type. Test
support adds bounded raw TLS segments/delays and close notification, identically
available in BEFORE/AFTER. One valid unused OAuth fixture gains token_type;
negative expiration fixtures keep their invalid values and now declare a valid
type so their original expiration assertions remain discriminating. R7 tests,
prior fixes, DSL, executor architecture and authentication authority are retained.
The final TLS fixture follow-up accepts normal peer-disconnect errors during
proxy writes and declares Connection: close when the raw fixture closes. Its
TTL oracle requires the expiry error and two complete helper exchanges; the
corrected BEFORE still reproduces a protected handoff and the delayed long-lived
control still passes. These setup corrections do not substitute for a defect
reproduction or weaken any malformed-response/body-limit assertion.
