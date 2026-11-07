# Review l0-instrument-review-1-sol (INSTRUMENT, on 197bed2e)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from the rollout turn_context),
2026-11-07T20:33:17–20:39:56Z. Transcript: `$WS/work/roles/l0-instrument-review-1-sol/`.
**Verdict: REJECT** (6 blocking, 3 non-blocking). The baseline of the affected instruments does not
count until the fixes are re-reviewed.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | R3 execution guard bypassable (regex) | blocking | **fix**: compile outside, run the test binary inside bubblewrap (ro root, one writable scratch dir, no network, cleared env, timeout); self-test with escape attempts — I3 fix lane |
| 2 | R3 passes wrong body/headers; results unchecked | blocking | **fix**: compare body and Concord-set headers to the reference recording; result fixtures; wrong-body/header controls — I3 fix lane |
| 3 | R2 credits a wrong error owner (`made_up::Error`) | blocking | **fix**: resolve only documented imports/aliases; wrong-owner controls — I3 fix lane |
| 4 | Worked answers survive material filtering (PokeAPI inline chains) | blocking | **fix**: strip inline call chains; rewrite and validate the leakage detector — I3 fix lane |
| 5 | S3 signing control does not establish key use | blocking | **fix**: verify the recorded SigV4 signature with the canary key via the SDK's `tests/common` verifier — coordinator |
| 6 | ClickHouse header username is a credential input | blocking | **accepted; my earlier triage reversed**: the rule includes credentials independently of `#[sensitive]`, and Basic mode already withholds the user. Restore the canary; fix the SDK (`user` marked sensitive, `Credentials::Headers` Debug withholds it) with fail-before/pass-after — coordinator |
| 7 | I1 counters differ from their names (S2 misses "withheld", S3 matches prose, D3 overlaps, D1c reachability, R4c divisor) | non-blocking | **fixed** `5469b5e0` (diagnostics only) |
| 8 | I5 never supplied generated names; missing error dropped from denominator | non-blocking | **fixed** `268aca50`; UI baseline 95/102 (was 100/102) |
| 9 | Discrimination unestablished (ceiling) | non-blocking | **accepted as a limit**: the broad I3 bank is a regression guard; H04's decision uses its own pre-registered task set (amendment §7.1) with the witnessed problems, policy SDKs and paired repetitions; unresolved ceilings are INCONCLUSIVE. The I3 fix lane may widen eligibility with SDK public constructors. |

Fix status: #5 fixed `d893895f` (S3 canary verifies the SigV4 signature with the canary key, fails with another); #6 fixed `dac69fe4` (ClickHouse username withheld, fail-before/pass-after); #1–#4 in the I3 fix lane; #7 `5469b5e0`; #8 `268aca50`. Re-review after the I3 fix lane.
