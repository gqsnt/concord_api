# Wave 3 status

Frozen library baseline: `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e` (`sdk50/integration`): Wave 2
SDKs, the between-waves changes (B23-*), their adoption and six review rounds (the last found no
defect). Gate on it: `scripts/verify.sh --package --bench` 66/66 PASS, clean, 7m11s;
`--nightly-only` PASS (16 tests). Routing R-1: Codex roles are `gpt-6.1-sol` only.

| SDK | Worktree / branch | Author (requested) | Status |
|---|---|---|---|
| SDK31 Adyen Checkout | `work/w3-sdk31-adyen-checkout`, `sdk50/w3-sdk31-adyen-checkout` | Codex `gpt-6.1-sol` xhigh | authoring |
| SDK32 PayPal | `work/w3-sdk32-paypal`, `sdk50/w3-sdk32-paypal` | Claude subagent (Opus) xhigh | authoring |
| SDK33 Square Payments | `work/w3-sdk33-square-payments`, `sdk50/w3-sdk33-square-payments` | Codex `gpt-6.1-sol` xhigh | authoring |
| SDK34 QuickBooks Online | `work/w3-sdk34-quickbooks-online`, `sdk50/w3-sdk34-quickbooks-online` | Claude subagent (Opus) xhigh | authoring |
| SDK35 Xero Accounting | `work/w3-sdk35-xero-accounting`, `sdk50/w3-sdk35-xero-accounting` | Codex `gpt-6.1-sol` xhigh | authoring |

Observed settings are recorded in `../../MODELS.md` when each role ends. Slice notes:
`../../roles/w3-extra-*.md`; capsules are copied when the wave closes.
